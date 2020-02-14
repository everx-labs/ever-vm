/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.  You may obtain a copy of the
* License at: https://ton.dev/licenses
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use executor::engine::Engine;
use stack::integer::serialization::{
    Encoding,
    SignedIntegerBigEndianEncoding,
    SignedIntegerLittleEndianEncoding,
    UnsignedIntegerBigEndianEncoding,
    UnsignedIntegerLittleEndianEncoding
};
use stack::serialization::Deserializer;
use stack::{Cell, StackItem, IntegerData, SliceData, BuilderData};
use types::{Exception, ExceptionCode, Result, UInt256};
use executor::engine::data::convert;
use executor::engine::storage::fetch_stack;
use executor::gas::gas_state::Gas;
use executor::microcode::{SLICE, CELL, VAR};
use executor::types::{Ctx, InstructionOptions, Instruction};
use executor::Mask;
use std::sync::Arc;
use std::collections::HashSet;

const QUIET: u8 = 0x01; // quiet variant
const STACK: u8 = 0x02; // length of int in stack
const CMD:   u8 = 0x04; // length of int in cmd parameter
const PARAM: u8 = 0x08; // length of int in function parameter
const STAY:  u8 = 0x10; // return slice to stack
const INV:   u8 = 0x20; // invert (result remainder) on push
const CEL:   u8 = 0x02; // argument is Cell, otherwise Slice

fn load_slice<'a>(engine: &'a mut Engine, name: &'static str, len: &mut usize, how: u8) -> Result<Ctx<'a>> {
    let params = if how.bit(STACK) {
        2
    } else {
        1
    };
    let mut instruction = Instruction::new(name);
    if how.bit(CMD) {
        instruction = instruction
            .set_opts(InstructionOptions::LengthMinusOne(0..*len))
    }
    engine.load_instruction(instruction)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        if how.bit(STACK) {
            *len = ctx.engine.cmd.var(0).as_integer()?.into(0..=*len)?
        } else if how.bit(CMD) {
            *len = ctx.engine.cmd.length();
        }
        Ok(ctx)
    })
}

fn proc_slice<F>(ctx: Ctx, len: usize, how: u8, f: F) -> Result<Ctx>
where F: FnOnce(&mut SliceData, &mut Gas) -> Result<StackItem> {
    let mut slice = ctx.engine.cmd.vars.last().unwrap().as_slice()?.clone();
    if slice.remaining_bits() < len {
        if how.bit(STAY) {
            ctx.engine.cc.stack.push(StackItem::Slice(slice));
        }
        if how.bit(QUIET) {
            ctx.engine.cc.stack.push(boolean!(false));
        } else {
            return err!(ExceptionCode::CellUnderflow);
        }
    } else {
        let value = f(&mut slice, &mut ctx.engine.gas)?;
        if how.bit(INV) {
            if how.bit(STAY) {
                ctx.engine.cc.stack.push(StackItem::Slice(slice));
            }
            ctx.engine.cc.stack.push(value);
        } else {
            ctx.engine.cc.stack.push(value);
            if how.bit(STAY) {
                ctx.engine.cc.stack.push(StackItem::Slice(slice));
            }
        }
        if how.bit(QUIET) {
            ctx.engine.cc.stack.push(boolean!(true));
        }
    }
    Ok(ctx)
}

// (slice <bits> - x <slice> <-1> - <slice> <0>)
fn ld_int<T: Encoding>(engine: &mut Engine, name: &'static str, mut len: usize, how: u8)
-> Option<Exception> {
    load_slice(engine, name, &mut len, how)
    .and_then(|ctx| proc_slice(ctx, len, how,
        |slice, _| {
            let value = T::new(len).deserialize(slice.get_next_bits(len)?.as_slice());
            Ok(StackItem::Integer(Arc::new(value)))
        }
    ))
    .err()
}

// (slice <bits> - x <slice> <-1> - <slice> <0>)
fn ld_slice(engine: &mut Engine, name: &'static str, mut len: usize, how: u8) -> Option<Exception> {
    load_slice(engine, name, &mut len, how)
    .and_then(|ctx| proc_slice(ctx, len, how,
        |slice, _| {
            let value = slice.get_next_slice(len)?;
            Ok(StackItem::Slice(value))
        }
    ))
    .err()
}

pub fn execute_ldsliceq(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "LDSLICEQ", 256, CMD | QUIET | STAY)
}

/// LDSLICE cc+1 (s - s`` s`), cuts the next cc+1 bits of s into a separate Slice s``.
pub fn execute_ldslice(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "LDSLICE", 256, CMD | STAY)
}

pub fn execute_pldsliceq(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "PLDSLICEQ", 256, CMD | QUIET)
}

/// PLDSLICE cc+1 (s - s``), cuts the next cc+1 bits of s into a separate Slice s``.
pub fn execute_pldslice(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "PLDSLICE", 256, CMD)
}

pub fn execute_ldslicexq(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "LDSLICEXQ", 1023, STACK | QUIET | STAY)
}

/// LDSLICEX(sl - s`` s`), loads the first 0 =< l =< 1023 bits from Slice s
/// into a separate Slice s``, returning the remainder of s as s`.
pub fn execute_ldslicex(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "LDSLICEX", 1023, STACK | STAY)
}

pub fn execute_pldslicexq(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "PLDSLICEXQ", 1023, STACK | QUIET)
}

/// PLDSLICEX(sl - s``)
pub fn execute_pldslicex(engine: &mut Engine) -> Option<Exception> {
    ld_slice(engine, "PLDSLICEX", 1023, STACK)
}

// (cell - slice)
pub fn execute_ctos(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CTOS")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| convert(ctx, var!(0), SLICE, CELL))
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// (slice - )
pub fn execute_ends(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("ENDS")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        if !ctx.engine.cmd.var(0).as_slice()?.is_empty() {
            err!(ExceptionCode::CellUnderflow)
        } else {
            Ok(ctx)
        }
    })
    .err()
}

// (slice - x slice)
pub fn execute_ldu(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDU", 256, CMD | STAY)
}

// (slice - x slice)
pub fn execute_ldi(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDI", 256, CMD | STAY)
}

pub fn execute_ldiq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDIQ", 256, CMD | QUIET | STAY)
}

pub fn execute_lduq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDUQ", 256, CMD | QUIET | STAY)
}

pub fn execute_ldixq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDIXQ", 257, STACK | QUIET | STAY)
}

pub fn execute_lduxq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDUXQ", 256, STACK | QUIET | STAY)
}

// (slice length - x slice)
pub fn execute_ldix(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDIX", 257, STACK | STAY)
}

// (slice length - x slice) 256
pub fn execute_ldux(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDUX", 256, STACK | STAY)
}

pub fn execute_pldixq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDIXQ", 257, STACK | QUIET)
}

pub fn execute_plduxq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDUXQ", 256, STACK | QUIET)
}

// (slice length - x)
pub fn execute_pldix(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDIX", 257, STACK)
}

// (slice length - x)
pub fn execute_pldux(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDUX", 256, STACK)
}

// (slice - cell slice)
pub fn execute_ldref(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("LDREF")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| proc_slice(ctx, 0, STAY,
        |slice, _| {
            Ok(StackItem::Cell(slice.checked_drain_reference()?.clone()))
        }
    ))
    .err()
}

// (slice - slice' slice'')
pub fn execute_ldrefrtos(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("LDREFRTOS")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| proc_slice(ctx, 0, STAY | INV, |slice, gas|
        Ok(StackItem::Slice(SliceData::from_cell(slice.checked_drain_reference()?, gas)))
    ))
    .err()
}

// (slice - x -1 or 0)
pub fn execute_pldiq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDIQ", 256, CMD | QUIET)
}

// (slice - x -1 or 0)
pub fn execute_plduq(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDUQ", 256, CMD | QUIET)
}

// (slice - x)
pub fn execute_pldu(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDU", 256, CMD)
}

// (slice - x)
pub fn execute_pldi(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDI", 256, CMD)
}

// (slice - x s)
pub fn execute_plduz(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("PLDUZ").set_opts(InstructionOptions::LengthMinusOne(0..8))
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let l = 32 * ctx.engine.cmd.length();
        let slice = ctx.engine.cmd.var(0).as_slice()?.clone();
        let n = slice.remaining_bits();
        let mut data = slice.clone().get_next_slice(std::cmp::min(n, l))?;
        if n < l {
            let r = l - n;
            let mut builder = BuilderData::from_slice(&data);
            builder.append_raw(vec![0; 1 + r / 8].as_slice(), r).unwrap();
            data = builder.into();
        }
        let encoder = UnsignedIntegerBigEndianEncoding::new(l);
        let value = encoder.deserialize(&data.get_bytestring(0));
        ctx.engine.cc.stack.push(StackItem::Slice(slice));
        ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(value)));
        Ok(ctx)
    })
    .err()
}

fn sdbegins(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    let mut inst = Instruction::new(name);
    let params = if how.bit(STACK) {
        2
    } else {
        inst = inst.set_opts(InstructionOptions::Bitstring(14, 0, 7, 0));
        1
    };
    engine.load_instruction(inst)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        let prefix = if how.bit(CMD) {
            ctx.engine.cmd.slice()
        } else if how.bit(STACK) {
            ctx.engine.cmd.var(0).as_slice()?
        } else {
            return err!(ExceptionCode::FatalError)
        };
        let mut tested = ctx.engine.cmd.var(params - 1).as_slice()?.clone();
        let len = prefix.remaining_bits();
        if len > tested.remaining_bits() {
            if how.bit(QUIET) {
                ctx.engine.cc.stack.push(StackItem::Slice(tested));
                ctx.engine.cc.stack.push(boolean!(false));
                return Ok(ctx)
            } else {
                return err!(ExceptionCode::CellUnderflow)
            }
        }
        let result = SliceData::common_prefix(&tested, prefix).2.is_none();
        if result {
            tested.shrink_data(len..);
        } else if !how.bit(QUIET) {
            return err!(ExceptionCode::CellUnderflow);
        }
        ctx.engine.cc.stack.push(StackItem::Slice(tested));
        if how.bit(QUIET) {
            ctx.engine.cc.stack.push(boolean!(result));
        }
        Ok(ctx)
    })
    .err()
}

pub fn execute_sdbegins(engine: &mut Engine) -> Option<Exception> {
    sdbegins(engine, "SDBEGINS", CMD)
}

pub fn execute_sdbeginsq(engine: &mut Engine) -> Option<Exception> {
    sdbegins(engine, "SDBEGINSQ", CMD | QUIET)
}

/// SDBEGINSX(s s` - s``), checks whether s begins with 
/// (the data bits of) s`, and removes s` from s on success.
/// On failure throws a cell deserialization exception.
pub fn execute_sdbeginsx(engine: &mut Engine) -> Option<Exception> {
    sdbegins(engine, "SDBEGINSX", STACK)
}

pub fn execute_sdbeginsxq(engine: &mut Engine) -> Option<Exception> {
    sdbegins(engine, "SDBEGINSXQ", STACK | QUIET)
}

const DROP: u8 = 0x01;   // drop all
const FROM: u8 = 0x02;   // starting position
const LAST: u8 = 0x04;   // last portion
const SIZE: u8 = 0x08;   // portion size
const UPTO: u8 = 0x10;   // ending position

const FROM_SIZE: u8 = FROM | SIZE;
const NOT_LAST:  u8 = INV | LAST;

fn sdcut(ctx: Ctx, bits: u8, refs: u8) -> Result<Ctx> {
    let mut i = 0;
    let r1 = if (refs & SIZE) == SIZE {
        i += 1;
        ctx.engine.cmd.var(i - 1).as_integer()?.into(0..=4)?    
    } else {
        0
    };
    let l1 = if (bits & SIZE) == SIZE {
        i += 1;
        ctx.engine.cmd.var(i - 1).as_integer()?.into(0..=1023)?    
    } else {
        0
    };
    let r0 = if (refs & (FROM | LAST | UPTO)) != 0 {
        i += 1;
        ctx.engine.cmd.var(i - 1).as_integer()?.into(0..=4)?
    } else {
        0
    };
    let l0 = ctx.engine.cmd.var(i).as_integer()?.into(0..=1023)?;
    let mut slice = ctx.engine.cmd.var(i + 1).as_slice()?.clone();
    let data_len = slice.remaining_bits();
    let refs_count = slice.remaining_references();
    if (l0 + l1 > data_len) || (r0 + r1 > refs_count) {
        return err!(ExceptionCode::CellUnderflow);
    }
    match refs {
        DROP | UPTO => slice.shrink_references(..r0),
        FROM => slice.shrink_references(r0..),
        FROM_SIZE => slice.shrink_references(r0..r0 + r1),
        LAST => slice.shrink_references(refs_count - r0..),
        NOT_LAST => slice.shrink_references(..refs_count - r0),
        _ => vec![]
    };
    match bits {
        FROM => slice.shrink_data(l0..),
        FROM_SIZE => slice.shrink_data(l0..l0 + l1),
        LAST => slice.shrink_data(data_len - l0..),
        NOT_LAST => slice.shrink_data(..data_len - l0),
        UPTO => slice.shrink_data(..l0),
        _ => SliceData::default()
    };
    ctx.engine.cc.stack.push(StackItem::Slice(slice));
    Ok(ctx)
}

/// SDSKIPFIRST(sl - s`), returns all but the first 0 ≤ l ≤ 1023 bits of s
pub fn execute_sdskipfirst(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SDSKIPFIRST")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| sdcut(ctx, FROM, 0))
    .err()
}

pub fn execute_sdcutlast(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SDCUTLAST")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| sdcut(ctx, LAST, DROP))
    .err()
}

/// SDSKIPLAST(sl - s`), returns all but the first 0 ≤ l ≤ 1023 bits of s
pub fn execute_sdskiplast(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SDSKIPLAST")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| sdcut(ctx, INV | LAST, DROP))
    .err()
}

/// SDSUBSTR(s l` l`` - s`), returns 0 ≤ l′ ≤ 1023 bits of s 
/// starting from offset 0 ≤ l ≤ 1023, thus extracting a bit 
/// substring out of the data of s.
pub fn execute_sdsubstr(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SDSUBSTR")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| sdcut(ctx, FROM | SIZE, DROP))
    .err()
}

/// (s l r – s`), returns the first 0 <= l <= 1023 bits and first 0 <= r <= 4 references of s
pub fn execute_scutfirst(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SCUTFIRST")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| sdcut(ctx, UPTO, UPTO))
    .err()
}

/// (s l r – s`), skips the first 0 <= l <= 1023 bits and first 0 <= r <= 4 references of s
pub fn execute_sskipfirst(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SSKIPFIRST")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| sdcut(ctx, FROM, FROM))
    .err()
}

/// (s l r – s`), returns the last 0 <= l <= 1023 data bits
///  and last 0 <= r <= 4 references of s.
pub fn execute_scutlast(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SCUTLAST")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| sdcut(ctx, LAST, LAST))
    .err()
}

/// (s l r – s`)
pub fn execute_sskiplast(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SSKIPLAST")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| sdcut(ctx, INV | LAST, INV | LAST))
    .err()
}

/// (s l r l` r` – s`), returns 0 <= l`<= 1023 bits and 0 <= r` <= 4
/// references from Slice s, after skipping the first 0 <= l <= 1023 
/// bits and first 0 <= r <= 4 references.
pub fn execute_subslice(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SUBSLICE")
    )
    .and_then(|ctx| fetch_stack(ctx, 5))
    .and_then(|ctx| sdcut(ctx, FROM | SIZE, FROM | SIZE))
    .err()
}

#[derive(PartialEq)]
enum Target {
    Bits,
    Refs,
    BitRefs,
}

fn sbitrefs(engine: &mut Engine, name: &'static str, target: Target) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let s = ctx.engine.cmd.var(0).as_slice()?.clone();
        if (target == Target::Bits) || (target == Target::BitRefs) {
            let l = s.remaining_bits();
            ctx.engine.cc.stack.push(int!(l));
        }
        if (target == Target::Refs) || (target == Target::BitRefs) {
            let r = s.remaining_references();
            ctx.engine.cc.stack.push(int!(r));
        }
        Ok(ctx)
    })
    .err()
}

fn schkbits(engine: &mut Engine, name: &'static str, limit: usize, quiet: bool) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let l = ctx.engine.cmd.var(0).as_integer()?.into(0..=limit)?;
        let s = ctx.engine.cmd.var(1).as_slice()?;
        if quiet {
            ctx.engine.cc.stack.push(boolean!(s.remaining_bits() >= l));
        } else if s.remaining_bits() < l {
            return err!(ExceptionCode::CellUnderflow);
        }
        Ok(ctx)
    })
    .err()
}

fn schkrefs(engine: &mut Engine, name: &'static str, quiet: bool) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let r = ctx.engine.cmd.var(0).as_integer()?.into(0..=4)?;
        let s = ctx.engine.cmd.var(1).as_slice()?;
        let refs_count = s.remaining_references();
        if quiet {
            ctx.engine.cc.stack.push(boolean!(refs_count >= r));
        } else if refs_count < r {
            return err!(ExceptionCode::CellUnderflow);
        }
        Ok(ctx)
    })
    .err()
}

fn schkbitrefs(engine: &mut Engine, name: &'static str, quiet: bool) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let r = ctx.engine.cmd.var(0).as_integer()?.into(0..=4)?;
        let l = ctx.engine.cmd.var(1).as_integer()?.into(0..=1023)?;
        let s = ctx.engine.cmd.var(2).as_slice()?;
        let data_len = s.remaining_bits();
        let refs_count = s.remaining_references();
        let status = l <= data_len && r <= refs_count;
        if quiet {
            ctx.engine.cc.stack.push(boolean!(status));
        } else if !status {
            return err!(ExceptionCode::CellUnderflow);
        } 
        Ok(ctx)
    })
    .err()
}

pub fn execute_schkbitsq(engine: &mut Engine) -> Option<Exception> {
    schkbits(engine, "SCHKBITSQ", 1023, true)
}

pub fn execute_schkbits(engine: &mut Engine) -> Option<Exception> {
    schkbits(engine, "SCHKBITS", 1023, false)
}

pub fn execute_schkrefsq(engine: &mut Engine) -> Option<Exception> {
    schkrefs(engine, "SCHKREFSQ", true)
}

pub fn execute_schkrefs(engine: &mut Engine) -> Option<Exception> {
    schkrefs(engine, "SCHKREFS", false)
}

pub fn execute_schkbitrefsq(engine: &mut Engine) -> Option<Exception> {
    schkbitrefs(engine, "SCHKBITREFSQ", true)
}

pub fn execute_schkbitrefs(engine: &mut Engine) -> Option<Exception> {
    schkbitrefs(engine, "SCHKBITREFS", false)
}

fn pldref(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    let mut inst = Instruction::new(name);
    let mut params = 1;
    if how.bit(STACK) {
        params += 1;
    } else if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Length(0..4));
    }
    engine.load_instruction(inst)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        let n = if how.bit(STACK) {
            ctx.engine.cmd.var(0).as_integer()?.into(0..=3)?
        } else if how.bit(CMD) {
            ctx.engine.cmd.length()
        } else {
            0
        };
        proc_slice(ctx, 0, 0, |slice, _| Ok(StackItem::Cell(slice.reference(n)?.clone())))
    })
    .err()
}

// (slice - cell)
pub fn execute_pldref(engine: &mut Engine) -> Option<Exception> {
    pldref(engine, "PLDREF", 0)
}

// (slice - cell)
pub fn execute_pldrefidx(engine: &mut Engine) -> Option<Exception> {
    pldref(engine, "PLDREFIDX", CMD)
}

// (slice n - cell)
pub fn execute_pldrefvar(engine: &mut Engine) -> Option<Exception> {
    pldref(engine, "PLDREFVAR", STACK)
}

pub fn execute_sbits(engine: &mut Engine) -> Option<Exception> {
    sbitrefs(engine, "SBITS", Target::Bits)
}

pub fn execute_srefs(engine: &mut Engine) -> Option<Exception> {
    sbitrefs(engine, "SREFS", Target::Refs)
}

pub fn execute_sbitrefs(engine: &mut Engine) -> Option<Exception> {
    sbitrefs(engine, "SBITREFS", Target::BitRefs)
}

// (slice - x slice)
pub fn execute_ldile4(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE4", 32, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_ldule4(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE4", 32, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_ldile8(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE8", 64, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_ldule8(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE8", 64, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_pldile4(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE4", 32, PARAM)
}

// (slice - x slice)
pub fn execute_pldule4(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE4", 32, PARAM)
}

// (slice - x slice)
pub fn execute_pldile8(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE8", 64, PARAM)
}

// (slice - x slice)
pub fn execute_pldule8(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE8", 64, PARAM)
}

// (slice - x slice)
pub fn execute_ldile4q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE4Q", 32, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_ldule4q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE4Q", 32, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_ldile8q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE8Q", 64, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_ldule8q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE8Q", 64, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_pldile4q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE4Q", 32, PARAM | QUIET)
}

// (slice - x slice)
pub fn execute_pldule4q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE4Q", 32, PARAM | QUIET)
}

// (slice - x slice)
pub fn execute_pldile8q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE8Q", 64, PARAM | QUIET)
}

// (slice - x slice)
pub fn execute_pldule8q(engine: &mut Engine) -> Option<Exception> {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE8Q", 64, PARAM | QUIET)
}

fn trim_leading_bits(slice: &mut SliceData, bit: u8) -> usize {
    let mut skipped = 0;
    let n = slice.remaining_bits();
    for i in 0..n {
        if slice.get_bits(i, 1).unwrap() != bit {
            break;
        } else {
            skipped += 1;
        }
    }
    slice.shrink_data(skipped..);
    skipped
}

fn ldbit(engine: &mut Engine, name: &'static str, bit: u8) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut slice = ctx.engine.cmd.var(0).as_slice()?.clone();
        let skipped = trim_leading_bits(&mut slice, bit);
        ctx.engine.cc.stack.push(int!(skipped));
        ctx.engine.cc.stack.push(StackItem::Slice(slice));
        Ok(ctx)
    })
    .err()
}

pub fn execute_ldzeroes(engine: &mut Engine) -> Option<Exception> {
    ldbit(engine, "LDZEROES", 0)
}

pub fn execute_ldones(engine: &mut Engine) -> Option<Exception> {
    ldbit(engine, "LDONES", 1)
}

pub fn execute_ldsame(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("LDSAME")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x = ctx.engine.cmd.var(0).as_integer()?.into(0..=1)?;
        let mut slice = ctx.engine.cmd.var(1).as_slice()?.clone();
        let skipped = trim_leading_bits(&mut slice, x as u8);
        ctx.engine.cc.stack.push(int!(skipped));
        ctx.engine.cc.stack.push(StackItem::Slice(slice));
        Ok(ctx)
    })
    .err()
}

fn split(engine: &mut Engine, name: &'static str, quiet: bool) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let r = ctx.engine.cmd.var(0).as_integer()?.into(0..=4)?;
        let l = ctx.engine.cmd.var(1).as_integer()?.into(0..=1023)?;
        let mut slice = ctx.engine.cmd.var(2).as_slice()?.clone();
        let data_len = slice.remaining_bits();
        let refs_count = slice.remaining_references();
        if (l > data_len) || (r > refs_count) {
            if quiet {
                ctx.engine.cc.stack.push(StackItem::Slice(slice));
                ctx.engine.cc.stack.push(boolean!(false));
                return Ok(ctx);
            } else {
                return err!(ExceptionCode::CellUnderflow);
            }
        }
        let mut slice1 = slice.clone();
        slice.shrink_references(0..r);
        slice.shrink_data(0..l);
        slice1.shrink_references(r..);
        slice1.shrink_data(l..);
        ctx.engine.cc.stack.push(StackItem::Slice(slice));
        ctx.engine.cc.stack.push(StackItem::Slice(slice1));
        if quiet {
            ctx.engine.cc.stack.push(boolean!(true));
        }
        Ok(ctx)
    })
    .err()
}

pub fn execute_split(engine: &mut Engine) -> Option<Exception> {
    split(engine, "SPLIT", false)
}

pub fn execute_splitq(engine: &mut Engine) -> Option<Exception> {
    split(engine, "SPLITQ", true)
}

struct DataCounter {
    visited: HashSet<UInt256>,
    max: usize,
    cells: usize,
    bits: usize,
    refs: usize
}

impl DataCounter {
    fn new(max: usize) -> Self {
        Self {
            visited: HashSet::new(),
            max,
            cells: 0,
            bits: 0,
            refs: 0
        }
    }
    fn count_cell(&mut self, cell: Cell, engine: &mut Engine) -> bool {
        if !self.visited.insert(cell.repr_hash()) {
            return true
        }
        if self.max == 0 {
            return false
        }
        self.max -= 1;
        self.cells += 1;
        self.count_slice(SliceData::from_cell(cell, &mut engine.gas), engine)
    }
    fn count_slice(&mut self, slice: SliceData, engine: &mut Engine) -> bool {
        let refs = slice.remaining_references();
        self.refs += refs;
        self.bits += slice.remaining_bits();
        for i in 0..refs {
            if !self.count_cell(slice.reference(i).unwrap().clone(), engine) {
                return false
            }
        }
        true
    }
}

fn datasize(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|mut ctx| {
        let n = ctx.engine.cmd.var(0).as_integer()?.into(0..=std::usize::MAX)?;
        let mut counter = DataCounter::new(n);
        let result = if !how.bit(CEL) {
            let slice = ctx.engine.cmd.var(1).as_slice()?.clone();
            counter.count_slice(slice, &mut ctx.engine)
        } else if ctx.engine.cmd.var(1).is_null() {
            true
        } else {
            let cell = ctx.engine.cmd.var(1).as_cell()?.clone();
            counter.count_cell(cell, &mut ctx.engine)
        };
        if result {
            ctx.engine.cc.stack.push(int!(counter.cells));
            ctx.engine.cc.stack.push(int!(counter.bits));
            ctx.engine.cc.stack.push(int!(counter.refs));
        } else if !how.bit(QUIET) {
            return err!(ExceptionCode::CellOverflow)
        }
        if how.bit(QUIET) {
            ctx.engine.cc.stack.push(boolean!(result));
        }
        Ok(ctx)
    })
    .err()
}

pub(crate) fn execute_cdatasize(engine: &mut Engine) -> Option<Exception> {
    datasize(engine, "CDATASIZE", CEL)
}

pub(crate) fn execute_cdatasizeq(engine: &mut Engine) -> Option<Exception> {
    datasize(engine, "CDATASIZEQ", QUIET | CEL)
}

pub(crate) fn execute_sdatasize(engine: &mut Engine) -> Option<Exception> {
    datasize(engine, "SDATASIZE", 0)
}

pub(crate) fn execute_sdatasizeq(engine: &mut Engine) -> Option<Exception> {
    datasize(engine, "SDATASIZEQ", QUIET)
}
