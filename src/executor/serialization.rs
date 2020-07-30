/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use crate::{
    error::TvmError,
    executor::{
    	Mask, engine::{Engine, data::convert, storage::{fetch_stack, fetch_reference}},
        gas::gas_state::Gas, microcode::{BUILDER, CC, CELL, VAR},
        types::{Ctx, InstructionOptions, Instruction}
    },
    stack::{
        StackItem,
        integer::{
            IntegerData, 
            serialization::{
                Encoding, IntoSliceExt, SignedIntegerBigEndianEncoding, 
                SignedIntegerLittleEndianEncoding, UnsignedIntegerBigEndianEncoding,
                UnsignedIntegerLittleEndianEncoding
            }
        }
    },
    types::{Exception, Failure}
};
use ton_types::{BuilderData, Cell, CellType, GasConsumer, error, IBitstring, Result, types::ExceptionCode};
use std::sync::Arc;

const QUIET: u8 = 0x01; // quiet variant
const STACK: u8 = 0x02; // length of int in stack
const CMD:   u8 = 0x04; // length of int in cmd parameter
const BITS:  u8 = 0x08; // check bits
const REFS:  u8 = 0x10; // check refs
const INV:   u8 = 0x20; // Remain free in builder

// Cell serialization related instructions ************************************

// used of free bits or/and refs in builder 
fn size_b(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        match ctx.engine.cmd.var(0).as_builder()? {
            b if how.bit(INV) => {
                if how.bit(BITS) {
                    ctx.engine.cc.stack.push(int!(b.bits_free()));
                }
                if how.bit(REFS) {
                    ctx.engine.cc.stack.push(int!(b.references_free()));
                }
            }
            b => {
                if how.bit(BITS) {
                    ctx.engine.cc.stack.push(int!(b.bits_used()));
                }
                if how.bit(REFS) {
                    ctx.engine.cc.stack.push(int!(b.references_used()));
                }
            }
        }
        Ok(ctx)
    })
    .err()
}

/// BBITS (b - x), returns the number of data bits already stored in Builder b.
pub fn execute_bbits(engine: &mut Engine) -> Failure {
    size_b(engine, "BBITS", BITS)
}

/// BREFS (b - y), returns the number of cell references already stored in b.
pub fn execute_brefs(engine: &mut Engine) -> Failure {
    size_b(engine, "BREFS", REFS)
}

/// BBITREFS (b - x y), returns the numbers of both data bits and cell references in b.
pub fn execute_bbitrefs(engine: &mut Engine) -> Failure {
    size_b(engine, "BBITREFS", BITS | REFS)
}

/// BREMBITS (b - x`), returns the number of data bits that can still be stored in b.
pub fn execute_brembits(engine: &mut Engine) -> Failure {
    size_b(engine, "BREMBITS", INV | BITS)
}

/// BREMREFS (b - y`), returns the number of references that can still be stored in b.
pub fn execute_bremrefs(engine: &mut Engine) -> Failure {
    size_b(engine, "BREMREFS", INV | REFS)
}

/// BREMBITREFS (b - x` y`).
pub fn execute_brembitrefs(engine: &mut Engine) -> Failure {
    size_b(engine, "BREMBITREFS", INV | BITS | REFS)
}

// (builder - cell)
pub fn execute_endc(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ENDC")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| convert(ctx, var!(0), CELL, BUILDER))
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// (builder x - cell)
pub fn execute_endxc(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ENDXC")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let special = ctx.engine.cmd.var(0).as_bool()?;
        let mut b = ctx.engine.cmd.var_mut(1).as_builder_mut()?;
        if special {
            if b.length_in_bits() < 8 {
                ctx.engine.try_use_gas(Gas::finalize_price())?;
                return err!(ExceptionCode::CellOverflow, "Not enough data for a special cell")
            }
            let cell_type = CellType::from(b.data()[0]);
            b.set_type(cell_type);
        }
        let cell = ctx.engine.finalize_cell(b)?;
        ctx.engine.cc.stack.push(StackItem::Cell(cell));
        Ok(ctx)
    })
    .err()
}

// ( - builder)
pub fn execute_newc(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("NEWC")
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.push_builder(BuilderData::new());
        Ok(ctx)
    })
    .err()
}

// store data from one builder to another
fn store_data(ctx: Ctx, var: usize, x: Result<BuilderData>, quiet: bool, finalize: bool) -> Result<Ctx> {
    let result = match x {
        Ok(ref x) => {
            let b = ctx.engine.cmd.var(var).as_builder()?;
            if b.can_append(x) {
                let mut b = ctx.engine.cmd.var_mut(var).as_builder_mut()?;
                b.append_builder(x)?;
                if finalize {
                    ctx.engine.try_use_gas(Gas::finalize_price())?;
                }
                ctx.engine.cc.stack.push_builder(b);
                0
            } else if quiet {
                -1
            } else {
                return err!(ExceptionCode::CellOverflow)
            }
        }
        Err(e) => if quiet {
            1
        } else {
            return Err(e)
        }
    };
    if result != 0 {
        let len = ctx.engine.cmd.var_count();
        ctx.engine.cc.stack.push(ctx.engine.cmd.var(len - 1).clone());
        ctx.engine.cc.stack.push(ctx.engine.cmd.var(len - 2).clone());
        ctx.engine.cc.stack.push(int!(result));
    } else if quiet {
        ctx.engine.cc.stack.push(int!(0));
    }
    Ok(ctx)
}

// stores data from one builder ot another
fn store_b(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x;
        let b = if how.bit(INV) {
            x = ctx.engine.cmd.var(0).as_builder()?;
            ctx.engine.cmd.var(1).as_builder()?;
            1
        } else {
            ctx.engine.cmd.var(0).as_builder()?;
            x = ctx.engine.cmd.var(1).as_builder()?;
            0
        };
        let x = Ok(x.clone());
        store_data(ctx, b, x, how.bit(QUIET), false)
    })
    .err()
}

/// STB (b` b - b``), appends all data from Builder b` to Builder b.
pub fn execute_stb(engine: &mut Engine) -> Failure {
    store_b(engine, "STB", 0)
}

/// STBR (b b` - b``), concatenates two Builders, equivalent to SWAP; STB.
pub fn execute_stbr(engine: &mut Engine) -> Failure {
    store_b(engine, "STBR", INV)
}

/// STBQ (builder builder - (builder builder -1) | (builder 0)).
pub fn execute_stbq(engine: &mut Engine) -> Failure {
    store_b(engine, "STBQ", QUIET)
}

/// STBRQ (builder builder - (builder builder -1) | (builder 0)).
pub fn execute_stbrq(engine: &mut Engine) -> Failure {
    store_b(engine, "STBRQ", INV | QUIET)
}

// appends the cell as a reference to the builder
fn store_r(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x;
        let b = if how.bit(INV) {
            x = ctx.engine.cmd.var(0).as_cell()?;
            ctx.engine.cmd.var(1).as_builder()?;
            1
        } else {
            ctx.engine.cmd.var(0).as_builder()?;
            x = ctx.engine.cmd.var(1).as_cell()?;
            0
        };
        let x = BuilderData::with_raw_and_refs(vec![], 0, vec![x.clone()])
            .map_err(|err| err.into());
        store_data(ctx, b, x, how.bit(QUIET), false)
    })
    .err()
}

// (cell builder - builder)
pub fn execute_stref(engine: &mut Engine) -> Failure {
    store_r(engine, "STREF", 0)
}

/// STREFR (b c - b`).
pub fn execute_strefr(engine: &mut Engine) -> Failure {
    store_r(engine, "STREFR", INV)
}

// (cell builder - (cell builder -1) | (builder 0))
pub fn execute_strefq(engine: &mut Engine) -> Failure {
    store_r(engine, "STREFQ", QUIET)
}

// (builder cell - (builder cell -1) | (builder 0))
pub fn execute_strefrq(engine: &mut Engine) -> Failure {
    store_r(engine, "STREFRQ", INV | QUIET)
}

// store one builder to another as reference
fn store_br(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x;
        let b = if how.bit(INV) {
            x = ctx.engine.cmd.var(0).as_builder()?;
            ctx.engine.cmd.var(1).as_builder()?;
            1
        } else {
            ctx.engine.cmd.var(0).as_builder()?;
            x = ctx.engine.cmd.var(1).as_builder()?;
            0
        };
        let x = BuilderData::with_raw_and_refs(vec![], 0, vec![x.into()])
            .map_err(|err| err.into());
        store_data(ctx, b, x, how.bit(QUIET), true)
    })
    .err()
}

/// STBREF (b` b - b``), equivalent to SWAP; STBREFREV
pub fn execute_stbref(engine: &mut Engine) -> Failure {
    store_br(engine, "STBREF", 0)
}

// (builder_outer builder_inner - builder)
pub fn execute_endcst(engine: &mut Engine) -> Failure {
    store_br(engine, "ENDCST", INV)
}

/// STBREFQ
pub fn execute_stbrefq(engine: &mut Engine) -> Failure {
    store_br(engine, "STBREFQ", QUIET)
}

/// STBREFQ
pub fn execute_stbrefrq(engine: &mut Engine) -> Failure {
    store_br(engine, "STBREFRQ", INV | QUIET)
}

fn store_s(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x;
        let b = if how.bit(INV) {
            x = ctx.engine.cmd.var(0).as_slice()?;
            ctx.engine.cmd.var(1).as_builder()?;
            1
        } else {
            ctx.engine.cmd.var(0).as_builder()?;
            x = ctx.engine.cmd.var(1).as_slice()?;
            0
        };
        let x = Ok(BuilderData::from_slice(x));
        store_data(ctx, b, x, how.bit(QUIET), false)
    })
    .err()
}

// (D b - b')
pub(crate) fn execute_stdict(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("STDICT")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_builder()?;
        let x = match ctx.engine.cmd.var(1).as_dict()? {
            Some(x) => BuilderData::with_raw_and_refs(vec![0xC0], 1, vec![x.clone()]),
            None => BuilderData::with_raw(vec![0x40], 1)
        };
        store_data(ctx, 0, x.map_err(|err| err.into()), false, false)
    })
    .err()
}

// (s b - b)
pub fn execute_stslice(engine: &mut Engine) -> Failure {
    store_s(engine, "STSLICE", 0)
}

/// STSLICER (b s - b`)
pub fn execute_stslicer(engine: &mut Engine) -> Failure {
    store_s(engine, "STSLICER", INV)
}

// (slice builder - (slice builder -1) | (builder 0))
pub fn execute_stsliceq(engine: &mut Engine) -> Failure {
    store_s(engine, "STSLICEQ", QUIET)
}

// (builder slice - (builder slice -1 ) | (builder 0))
pub fn execute_stslicerq(engine: &mut Engine) -> Failure {
    store_s(engine, "STSLICERQ", INV | QUIET)
}

fn check_b(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    let mut instruction = Instruction::new(name);
    let mut params = 1;
    if how.bit(BITS) {params += 1}
    if how.bit(REFS) {params += 1}
    if how.bit(CMD) {
        params -= 1;
        instruction = instruction.set_opts(InstructionOptions::LengthMinusOne(0..256))
    }
    engine.load_instruction(instruction)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        // TODO: right order of type check
        let l = if how.bit(CMD) {
            ctx.engine.cmd.length()
        } else if how.bit(BITS) {
            ctx.engine.cmd.var(params - 2).as_integer()?.into(0..=1023)?
        } else {
            0
        };
        let r = if how.bit(REFS) {
            ctx.engine.cmd.var(0).as_integer()?.into(0..=4)?
        } else {
            0
        };
        match ctx.engine.cmd.var(params - 1).as_builder()? {
            b => {
                let mut status = true;
                if how.bit(BITS) {
                    status &= b.check_enough_space(l)
                }
                if how.bit(REFS) {
                    status &= b.check_enough_refs(r)
                }
                if how.bit(QUIET) {
                    ctx.engine.cc.stack.push(boolean!(status)); 
                } else if !status {
                    return err!(ExceptionCode::CellOverflow)
                }
            }
        }
        Ok(ctx)
    })
    .err()
}

pub fn execute_bchkrefs(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKREFS", REFS | STACK)
}

pub fn execute_bchkrefsq(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKREFSQ", REFS | STACK | QUIET)
}

pub fn execute_bchkbitrefs(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKBITREFS", BITS | REFS | STACK)
}

pub fn execute_bchkbitrefsq(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKBITREFSQ", BITS | REFS | STACK | QUIET)
}

pub fn execute_bchkbits_short(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKBITS", BITS | CMD)
}

pub fn execute_bchkbits_long(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKBITS", BITS | STACK)
}

pub fn execute_bchkbitsq_short(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKBITS", BITS | CMD | QUIET)
}

pub fn execute_bchkbitsq_long(engine: &mut Engine) -> Failure {
    check_b(engine, "BCHKBITS", BITS | STACK | QUIET)
}

fn store<T: Encoding>(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::LengthMinusOne(0..256))
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let len = ctx.engine.cmd.length();
        let x;
        let b = if how.bit(INV) {
            x = ctx.engine.cmd.var(0).as_integer()?.into_builder::<T>(len);
            ctx.engine.cmd.var(1).as_builder()?;
            1
        } else {
            ctx.engine.cmd.var(0).as_builder()?;
            x = ctx.engine.cmd.var(1).as_integer()?.into_builder::<T>(len);
            0
        };
        store_data(ctx, b, x, how.bit(QUIET), false)
    })
    .err()
}

// (x builder - builder)
pub fn execute_sti(engine: &mut Engine) -> Failure {
    store::<SignedIntegerBigEndianEncoding>(engine, "STI", 0)
}

// (x builder - builder)
pub fn execute_stu(engine: &mut Engine) -> Failure {
    store::<UnsignedIntegerBigEndianEncoding>(engine, "STU", 0)
}

// (x builder - builder)
pub fn execute_stir(engine: &mut Engine) -> Failure {
    store::<SignedIntegerBigEndianEncoding>(engine, "STIR", INV)
}

// (x builder - builder)
pub fn execute_stur(engine: &mut Engine) -> Failure {
    store::<UnsignedIntegerBigEndianEncoding>(engine, "STUR", INV)
}

// (x builder - builder)
pub fn execute_stiq(engine: &mut Engine) -> Failure {
    store::<SignedIntegerBigEndianEncoding>(engine, "STIQ", QUIET)
}

// (x builder - builder)
pub fn execute_stuq(engine: &mut Engine) -> Failure {
    store::<UnsignedIntegerBigEndianEncoding>(engine, "STUQ", QUIET)
}

// (x builder - builder)
pub fn execute_stirq(engine: &mut Engine) -> Failure {
    store::<SignedIntegerBigEndianEncoding>(engine, "STIRQ", QUIET | INV)
}

// (x builder - builder)
pub fn execute_sturq(engine: &mut Engine) -> Failure {
    store::<UnsignedIntegerBigEndianEncoding>(engine, "STURQ", QUIET | INV)
}

fn store_x<T: Encoding>(engine: &mut Engine, name: &'static str, how: u8, limit: usize) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let len = ctx.engine.cmd.var(0).as_integer()?;
        let x;
        let b = if how.bit(INV) {
            x = ctx.engine.cmd.var(1).as_integer()?;
            ctx.engine.cmd.var(2).as_builder()?;
            2
        } else {
            ctx.engine.cmd.var(1).as_builder()?;
            x = ctx.engine.cmd.var(2).as_integer()?;
            1
        };
        let len = len.into(0..=limit)?;
        let x = x.into_builder::<T>(len);
        store_data(ctx, b, x, how.bit(QUIET), false)
    })
    .err()
}

// (integer builder nbits - builder)
pub fn execute_stix(engine: &mut Engine) -> Failure {
    store_x::<SignedIntegerBigEndianEncoding>(engine, "STIX", 0, 257)
}

// (integer builder nbits - builder)
pub fn execute_stux(engine: &mut Engine) -> Failure {
    store_x::<UnsignedIntegerBigEndianEncoding>(engine, "STUX", 0, 256)
}

// (builder integer nbits - builder)
pub fn execute_stixr(engine: &mut Engine) -> Failure {
    store_x::<SignedIntegerBigEndianEncoding>(engine, "STIXR", INV, 257)
}

// (builder integer nbits - builder)
pub fn execute_stuxr(engine: &mut Engine) -> Failure {
    store_x::<UnsignedIntegerBigEndianEncoding>(engine, "STUXR", INV, 256)
}

// (integer builder nbits - (integer builder integer) | (builder integer))
pub fn execute_stixq(engine: &mut Engine) -> Failure {
    store_x::<SignedIntegerBigEndianEncoding>(engine, "STIXQ", QUIET, 257)
}

// (integer builder nbits - (integer builder integer) | (builder integer))
pub fn execute_stuxq(engine: &mut Engine) -> Failure {
    store_x::<UnsignedIntegerBigEndianEncoding>(engine, "STUXQ", QUIET, 256)
}

// (builder integer nbits - (builder integer integer) | (builder integer))
pub fn execute_stixrq(engine: &mut Engine) -> Failure {
    store_x::<SignedIntegerBigEndianEncoding>(engine, "STIXRQ", QUIET | INV, 257)
}

// (builder integer nbits - (builder integer integer) | (builder integer))
pub fn execute_stuxrq(engine: &mut Engine) -> Failure {
    store_x::<UnsignedIntegerBigEndianEncoding>(engine, "STUXRQ", QUIET | INV, 256)
}

// stores the integer to the builder in little-endian order
fn store_l<T: Encoding>(engine: &mut Engine, name: &'static str, bits: usize) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_builder()?;
        let x = ctx.engine.cmd.var(1).as_integer()?.into_builder::<T>(bits);
        store_data(ctx, 0, x, false, false)
    })
    .err()
}

/// STILE4 (x b - b`), stores a little-endian signed 32-bit integer.
pub fn execute_stile4(engine: &mut Engine) -> Failure {
    store_l::<SignedIntegerLittleEndianEncoding>(engine, "STILE4", 32)
}

/// STULE4 (x b - b`), stores a little-endian unsigned 32-bit integer.
pub fn execute_stule4(engine: &mut Engine) -> Failure {
    store_l::<UnsignedIntegerLittleEndianEncoding>(engine, "STULE4", 32)
}

/// STILE8 (x b - b`), stores a little-endian signed 64-bit integer.
pub fn execute_stile8(engine: &mut Engine) -> Failure {
    store_l::<SignedIntegerLittleEndianEncoding>(engine, "STILE8", 64)
}

/// STULE8 (x b - b`), stores a little-endian unsigned 64-bit integer.
pub fn execute_stule8(engine: &mut Engine) -> Failure {
    store_l::<UnsignedIntegerLittleEndianEncoding>(engine, "STULE8", 64)
}

fn store_bits(mut builder: BuilderData, n: usize, bit: bool) -> Result<BuilderData> {
    if n != 0 {
        builder.append_raw(vec![if bit {0xFF} else {0}; n / 8 + 1].as_slice(), n)?;
    }
    Ok(builder)
}

fn stbits(engine: &mut Engine, name: &'static str, bit: bool) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let n = ctx.engine.cmd.var(0).as_integer()?;
        ctx.engine.cmd.var(1).as_builder()?;
        let n = n.into(0..=1023)?;
        let b = ctx.engine.cmd.var_mut(1).as_builder_mut()?;
        ctx.engine.cc.stack.push_builder(store_bits(b, n, bit)?);
        Ok(ctx)
    })
    .err()
}

/// STZEROES (b n – b`), stores n binary zeroes into Builder b.
pub fn execute_stzeroes(engine: &mut Engine) -> Failure {
    stbits(engine, "STZEROES", false)
}

/// stores n binary ones into Builder b.
pub fn execute_stones(engine: &mut Engine) -> Failure {
    stbits(engine, "STONES", true)
}

pub fn execute_stsame(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("STSAME")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let x = ctx.engine.cmd.var(0).as_integer()?;
        let n = ctx.engine.cmd.var(1).as_integer()?;
        ctx.engine.cmd.var(2).as_builder()?;
        let x = x.into(0..=1)?;
        let n = n.into(0..=1023)?;
        let b = ctx.engine.cmd.var_mut(2).as_builder_mut()?;
        ctx.engine.cc.stack.push_builder(store_bits(b, n, x != 0)?);
        Ok(ctx)
    })
    .err()
}

pub fn execute_stsliceconst(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("STSLICECONST").set_opts(InstructionOptions::Bitstring(9, 2, 3, 0))
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut builder = ctx.engine.cmd.var_mut(0).as_builder_mut()?;
        let slice = ctx.engine.cmd.slice();
        builder.checked_append_references_and_data(slice)?;
        ctx.engine.cc.stack.push_builder(builder);
        Ok(ctx)
    })
    .err()
}

pub fn execute_strefconst(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("STREFCONST")
    )
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut b = {
            ctx.engine.cmd.var(0).as_cell()?;
            ctx.engine.cmd.var_mut(1).as_builder_mut()?
        };
        b.checked_append_reference(ctx.engine.cmd.var(0).as_cell()?.clone())?;
        ctx.engine.cc.stack.push_builder(b);
        Ok(ctx)
    })
    .err()
}

pub fn execute_stref2const(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("STREF2CONST")
    )
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut b = {
            ctx.engine.cmd.var(0).as_cell()?;
            ctx.engine.cmd.var(1).as_cell()?;
            ctx.engine.cmd.var_mut(2).as_builder_mut()?
        };
        b.checked_append_reference(ctx.engine.cmd.var(0).as_cell()?.clone())?;
        b.checked_append_reference(ctx.engine.cmd.var(1).as_cell()?.clone())?;
        ctx.engine.cc.stack.push_builder(b);
        Ok(ctx)
    })
    .err()
}

fn calc_depth(cell: &Cell) -> usize {
    let mut depth = 0;
    let n = cell.references_count();
    for i in 0..n {
        if let Ok(cell) = cell.reference(i) {
            depth = std::cmp::max(depth, 1 + calc_depth(&cell));
        }
    }
    depth
}

/// BDEPTH (b - x), returns the depth of Builder b.
pub fn execute_bdepth(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("BDEPTH"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut depth = 0;
        let b = ctx.engine.cmd.var(0).as_builder()?;
        for cell in b.references() {
            depth = std::cmp::max(depth, 1 + calc_depth(cell));
        }
        ctx.engine.cc.stack.push(int!(depth));
        Ok(ctx)
    })
    .err()
}

/// CDEPTH (c - x), returns the depth of Cell c.
pub fn execute_cdepth(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("CDEPTH"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let depth = if ctx.engine.cmd.var(0).is_null() {
            0
        } else {
            let c = ctx.engine.cmd.var(0).as_cell()?;
            if c.references_count() == 0 {
                0
            } else {
                calc_depth(c)
            }
        };
        ctx.engine.cc.stack.push(int!(depth));
        Ok(ctx)
    })
    .err()
}

/// SDEPTH (s - x), returns the depth of Slice s.
pub fn execute_sdepth(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("SDEPTH"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut depth = 0;
        let s = ctx.engine.cmd.var(0).as_slice()?;
        let n = s.remaining_references();
        for i in 0..n {
            depth = std::cmp::max(depth, 1 + calc_depth(&s.reference(i)?));
        }
        ctx.engine.cc.stack.push(int!(depth));
        Ok(ctx)
    })
    .err()
}
