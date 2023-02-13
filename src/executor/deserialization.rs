/*
* Copyright (C) 2019-2023 TON Labs. All Rights Reserved.
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
        Mask, engine::{Engine, data::convert, storage::fetch_stack},
        microcode::{SLICE, CELL, VAR}, types::{InstructionOptions, Instruction}
    },
    stack::{
        StackItem,
        continuation::ContinuationData,
        integer::{
            IntegerData,
            serialization::{
                Encoding, SignedIntegerBigEndianEncoding, SignedIntegerLittleEndianEncoding,
                UnsignedIntegerBigEndianEncoding, UnsignedIntegerLittleEndianEncoding
            }
        },
        serialization::Deserializer
    },
    types::{Exception, Status}
};
use ton_block::GlobalCapabilities;
use ton_types::{
    error, CellType, GasConsumer, Result, SliceData, ExceptionCode
};
use std::collections::HashSet;

const QUIET: u8 = 0x01; // quiet variant
const STACK: u8 = 0x02; // length of int in stack
const CMD:   u8 = 0x04; // length of int in cmd parameter
const PARAM: u8 = 0x08; // length of int in function parameter
const STAY:  u8 = 0x10; // return slice to stack
const INV:   u8 = 0x20; // invert (result remainder) on push
const CEL:   u8 = 0x02; // argument is Cell, otherwise Slice

fn load_slice(engine: &mut Engine, name: &'static str, len: &mut usize, how: u8) -> Status {
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
    engine.load_instruction(instruction)?;
    fetch_stack(engine, params)?;
    if how.bit(STACK) {
        *len = engine.cmd.var(0).as_integer()?.into(0..=*len)?
    } else if how.bit(CMD) {
        *len = engine.cmd.length();
    }
    Ok(())
}

fn proc_slice<F>(engine: &mut Engine, len: usize, how: u8, f: F) -> Status
where F: FnOnce(&mut SliceData, &mut dyn GasConsumer) -> Result<StackItem> {
    let mut slice = engine.cmd.last_var()?.as_slice()?.clone();
    if slice.remaining_bits() < len {
        if how.bit(STAY) {
            engine.cc.stack.push(StackItem::Slice(slice));
        }
        if how.bit(QUIET) {
            engine.cc.stack.push(boolean!(false));
        } else {
            return err!(ExceptionCode::CellUnderflow);
        }
    } else {
        let value = f(&mut slice, engine)?;
        if how.bit(INV) {
            if how.bit(STAY) {
                engine.cc.stack.push(StackItem::Slice(slice));
            }
            engine.cc.stack.push(value);
        } else {
            engine.cc.stack.push(value);
            if how.bit(STAY) {
                engine.cc.stack.push(StackItem::Slice(slice));
            }
        }
        if how.bit(QUIET) {
            engine.cc.stack.push(boolean!(true));
        }
    }
    Ok(())
}

// (slice <bits> - x <slice> <-1> - <slice> <0>)
fn ld_int<T: Encoding>(engine: &mut Engine, name: &'static str, mut len: usize, how: u8)
-> Status {
    load_slice(engine, name, &mut len, how)?;
    proc_slice(engine, len, how,
        |slice, _| {
            let value = T::new(len).deserialize(slice.get_next_bits(len)?.as_slice());
            Ok(StackItem::int(value))
        }
    )
}

// (slice <bits> - x <slice> <-1> - <slice> <0>)
fn ld_slice(engine: &mut Engine, name: &'static str, mut len: usize, how: u8) -> Status {
    load_slice(engine, name, &mut len, how)?;
    proc_slice(engine, len, how,
        |slice, _| {
            let value = slice.get_next_slice(len)?;
            Ok(StackItem::Slice(value))
        }
    )
}

pub fn execute_ldsliceq(engine: &mut Engine) -> Status {
    ld_slice(engine, "LDSLICEQ", 256, CMD | QUIET | STAY)
}

/// LDSLICE cc+1 (s - s`` s`), cuts the next cc+1 bits of s into a separate Slice s``.
pub fn execute_ldslice(engine: &mut Engine) -> Status {
    ld_slice(engine, "LDSLICE", 256, CMD | STAY)
}

pub fn execute_pldsliceq(engine: &mut Engine) -> Status {
    ld_slice(engine, "PLDSLICEQ", 256, CMD | QUIET)
}

/// PLDSLICE cc+1 (s - s``), cuts the next cc+1 bits of s into a separate Slice s``.
pub fn execute_pldslice(engine: &mut Engine) -> Status {
    ld_slice(engine, "PLDSLICE", 256, CMD)
}

pub fn execute_ldslicexq(engine: &mut Engine) -> Status {
    ld_slice(engine, "LDSLICEXQ", 1023, STACK | QUIET | STAY)
}

/// LDSLICEX(sl - s`` s`), loads the first 0 =< l =< 1023 bits from Slice s
/// into a separate Slice s``, returning the remainder of s as s`.
pub fn execute_ldslicex(engine: &mut Engine) -> Status {
    ld_slice(engine, "LDSLICEX", 1023, STACK | STAY)
}

pub fn execute_pldslicexq(engine: &mut Engine) -> Status {
    ld_slice(engine, "PLDSLICEXQ", 1023, STACK | QUIET)
}

/// PLDSLICEX(sl - s``)
pub fn execute_pldslicex(engine: &mut Engine) -> Status {
    ld_slice(engine, "PLDSLICEX", 1023, STACK)
}

// (cell - slice)
pub fn execute_ctos(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("CTOS")
    )?;
    fetch_stack(engine, 1)?;
    convert(engine, var!(0), SLICE, CELL)?;
    engine.cc.stack.push(engine.cmd.vars.remove(0));
    Ok(())
}

// (cell - slice ?)
pub fn execute_xctos(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCTOS")
    )?;
    fetch_stack(engine, 1)?;
    let cell = engine.cmd.var(0).as_cell()?.clone();
    let special = cell.cell_type() != CellType::Ordinary;
    let slice = engine.load_hashed_cell(cell, false)?;
    engine.cc.stack.push(StackItem::Slice(slice));
    engine.cc.stack.push(boolean!(special));
    Ok(())
}

// (cell - cell)
pub fn execute_xload(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XLOAD")
    )?;
    fetch_stack(engine, 1)?;
    // now it does nothing as Durov's code
    let cell = engine.cmd.var(0).as_cell()?.clone();
    let slice = engine.load_cell(cell)?;
    let cell = slice.into_cell();
    engine.cc.stack.push(StackItem::Cell(cell));
    Ok(())
}

// (cell - cell -1 or 0)
pub fn execute_xloadq(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XLOADQ")
    )?;
    fetch_stack(engine, 1)?;
    // now it does nothing as Durov's code
    let cell = engine.cmd.var(0).as_cell()?.clone();
    if let Ok(slice) = engine.load_cell(cell.clone()) {
        let cell = slice.into_cell();
        engine.cc.stack.push(StackItem::Cell(cell));
        engine.cc.stack.push(boolean!(true));
    } else {
        engine.cc.stack.push(StackItem::Cell(cell));
        engine.cc.stack.push(boolean!(false));
    }
    Ok(())
}

// (slice - )
pub fn execute_ends(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ENDS")
    )?;
    fetch_stack(engine, 1)?;
    if !engine.cmd.var(0).as_slice()?.is_empty() {
        err!(ExceptionCode::CellUnderflow)
    } else {
        Ok(())
    }
}

// (slice - x slice)
pub fn execute_ldu(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDU", 256, CMD | STAY)
}

// (slice - x slice)
pub fn execute_ldi(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDI", 256, CMD | STAY)
}

pub fn execute_ldiq(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDIQ", 256, CMD | QUIET | STAY)
}

pub fn execute_lduq(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDUQ", 256, CMD | QUIET | STAY)
}

pub fn execute_ldixq(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDIXQ", 257, STACK | QUIET | STAY)
}

pub fn execute_lduxq(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDUXQ", 256, STACK | QUIET | STAY)
}

// (slice length - x slice)
pub fn execute_ldix(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "LDIX", 257, STACK | STAY)
}

// (slice length - x slice) 256
pub fn execute_ldux(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "LDUX", 256, STACK | STAY)
}

pub fn execute_pldixq(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDIXQ", 257, STACK | QUIET)
}

pub fn execute_plduxq(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDUXQ", 256, STACK | QUIET)
}

// (slice length - x)
pub fn execute_pldix(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDIX", 257, STACK)
}

// (slice length - x)
pub fn execute_pldux(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDUX", 256, STACK)
}

// (slice - cell slice)
pub fn execute_ldref(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("LDREF")
    )?;
    fetch_stack(engine, 1)?;
    proc_slice(engine, 0, STAY,
        |slice, _| {
            Ok(StackItem::Cell(slice.checked_drain_reference()?))
        }
    )
}

// (slice - slice' slice'')
pub fn execute_ldrefrtos(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("LDREFRTOS")
    )?;
    fetch_stack(engine, 1)?;
    proc_slice(engine, 0, STAY | INV, |slice, gas_consumer|
        Ok(StackItem::Slice(gas_consumer.load_cell(slice.checked_drain_reference()?)?))
    )
}

// (slice - x -1 or 0)
pub fn execute_pldiq(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDIQ", 256, CMD | QUIET)
}

// (slice - x -1 or 0)
pub fn execute_plduq(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDUQ", 256, CMD | QUIET)
}

// (slice - x)
pub fn execute_pldu(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerBigEndianEncoding>(engine, "PLDU", 256, CMD)
}

// (slice - x)
pub fn execute_pldi(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerBigEndianEncoding>(engine, "PLDI", 256, CMD)
}

// (slice - x s)
pub fn execute_plduz(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PLDUZ").set_opts(InstructionOptions::LengthMinusOne(0..8))
    )?;
    fetch_stack(engine, 1)?;
    let l = 32 * engine.cmd.length();
    let slice = engine.cmd.var(0).as_slice()?.clone();
    let n = slice.remaining_bits();
    let mut data = slice.clone().get_next_slice(std::cmp::min(n, l))?.get_bytestring(0);
    if n < l {
        let r = l - n;
        data.extend_from_slice(&vec![0; r / 8]);
    }
    let encoder = UnsignedIntegerBigEndianEncoding::new(l);
    let value = encoder.deserialize(&data);
    engine.cc.stack.push(StackItem::slice(slice));
    engine.cc.stack.push(StackItem::int(value));
    Ok(())
}

fn sdbegins(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    let mut inst = Instruction::new(name);
    let params = if how.bit(STACK) {
        2
    } else {
        inst = inst.set_opts(InstructionOptions::Bitstring(14, 0, 7, 0));
        1
    };
    engine.load_instruction(inst)?;
    fetch_stack(engine, params)?;
    let prefix = if how.bit(CMD) {
        engine.cmd.slice()
    } else if how.bit(STACK) {
        engine.cmd.var(0).as_slice()?
    } else {
        return err!(ExceptionCode::FatalError)
    };
    let mut tested = engine.cmd.var(params - 1).as_slice()?.clone();
    let len = prefix.remaining_bits();
    if len > tested.remaining_bits() {
        if how.bit(QUIET) {
            engine.cc.stack.push(StackItem::Slice(tested));
            engine.cc.stack.push(boolean!(false));
            return Ok(())
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
    engine.cc.stack.push(StackItem::Slice(tested));
    if how.bit(QUIET) {
        engine.cc.stack.push(boolean!(result));
    }
    Ok(())
}

pub fn execute_sdbegins(engine: &mut Engine) -> Status {
    sdbegins(engine, "SDBEGINS", CMD)
}

pub fn execute_sdbeginsq(engine: &mut Engine) -> Status {
    sdbegins(engine, "SDBEGINSQ", CMD | QUIET)
}

/// SDBEGINSX(s s` - s``), checks whether s begins with
/// (the data bits of) s`, and removes s` from s on success.
/// On failure throws a cell deserialization exception.
pub fn execute_sdbeginsx(engine: &mut Engine) -> Status {
    sdbegins(engine, "SDBEGINSX", STACK)
}

pub fn execute_sdbeginsxq(engine: &mut Engine) -> Status {
    sdbegins(engine, "SDBEGINSXQ", STACK | QUIET)
}

const DROP: u8 = 0x01;   // drop all
const FROM: u8 = 0x02;   // starting position
const LAST: u8 = 0x04;   // last portion
const SIZE: u8 = 0x08;   // portion size
const UPTO: u8 = 0x10;   // ending position

const FROM_SIZE: u8 = FROM | SIZE;
const NOT_LAST:  u8 = INV | LAST;

fn sdcut(engine: &mut Engine, bits: u8, refs: u8) -> Status {
    let mut i = 0;
    let r1 = if (refs & SIZE) == SIZE {
        i += 1;
        engine.cmd.var(i - 1).as_integer()?.into(0..=4)?
    } else {
        0
    };
    let l1 = if (bits & SIZE) == SIZE {
        i += 1;
        engine.cmd.var(i - 1).as_integer()?.into(0..=1023)?
    } else {
        0
    };
    let r0 = if (refs & (FROM | LAST | UPTO)) != 0 {
        i += 1;
        engine.cmd.var(i - 1).as_integer()?.into(0..=4)?
    } else {
        0
    };
    let l0 = engine.cmd.var(i).as_integer()?.into(0..=1023)?;
    let mut slice = engine.cmd.var(i + 1).as_slice()?.clone();
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
    engine.cc.stack.push(StackItem::Slice(slice));
    Ok(())
}

/// SDSKIPFIRST(sl - s`), returns all but the first 0 ≤ l ≤ 1023 bits of s
pub fn execute_sdskipfirst(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SDSKIPFIRST")
    )?;
    fetch_stack(engine, 2)?;
    sdcut(engine, FROM, 0)
}

pub fn execute_sdcutlast(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SDCUTLAST")
    )?;
    fetch_stack(engine, 2)?;
    sdcut(engine, LAST, DROP)
}

/// SDSKIPLAST(sl - s`), returns all but the first 0 ≤ l ≤ 1023 bits of s
pub fn execute_sdskiplast(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SDSKIPLAST")
    )?;
    fetch_stack(engine, 2)?;
    sdcut(engine, INV | LAST, DROP)
}

/// SDSUBSTR(s l` l`` - s`), returns 0 ≤ l′ ≤ 1023 bits of s
/// starting from offset 0 ≤ l ≤ 1023, thus extracting a bit
/// substring out of the data of s.
pub fn execute_sdsubstr(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SDSUBSTR")
    )?;
    fetch_stack(engine, 3)?;
    sdcut(engine, FROM | SIZE, DROP)
}

/// (s l r – s`), returns the first 0 <= l <= 1023 bits and first 0 <= r <= 4 references of s
pub fn execute_scutfirst(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SCUTFIRST")
    )?;
    fetch_stack(engine, 3)?;
    sdcut(engine, UPTO, UPTO)
}

/// (s l r – s`), skips the first 0 <= l <= 1023 bits and first 0 <= r <= 4 references of s
pub fn execute_sskipfirst(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SSKIPFIRST")
    )?;
    fetch_stack(engine, 3)?;
    sdcut(engine, FROM, FROM)
}

/// (s l r – s`), returns the last 0 <= l <= 1023 data bits
///  and last 0 <= r <= 4 references of s.
pub fn execute_scutlast(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SCUTLAST")
    )?;
    fetch_stack(engine, 3)?;
    sdcut(engine, LAST, LAST)
}

/// (s l r – s`)
pub fn execute_sskiplast(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SSKIPLAST")
    )?;
    fetch_stack(engine, 3)?;
    sdcut(engine, INV | LAST, INV | LAST)
}

/// (s l r l` r` – s`), returns 0 <= l`<= 1023 bits and 0 <= r` <= 4
/// references from Slice s, after skipping the first 0 <= l <= 1023
/// bits and first 0 <= r <= 4 references.
pub fn execute_subslice(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SUBSLICE")
    )?;
    fetch_stack(engine, 5)?;
    sdcut(engine, FROM | SIZE, FROM | SIZE)
}

#[derive(PartialEq)]
enum Target {
    Bits,
    Refs,
    BitRefs,
}

fn sbitrefs(engine: &mut Engine, name: &'static str, target: Target) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 1)?;
    let s = engine.cmd.var(0).as_slice()?.clone();
    if (target == Target::Bits) || (target == Target::BitRefs) {
        let l = s.remaining_bits();
        engine.cc.stack.push(int!(l));
    }
    if (target == Target::Refs) || (target == Target::BitRefs) {
        let r = s.remaining_references();
        engine.cc.stack.push(int!(r));
    }
    Ok(())
}

fn schkbits(engine: &mut Engine, name: &'static str, limit: usize, quiet: bool) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 2)?;
    let l = engine.cmd.var(0).as_integer()?.into(0..=limit)?;
    let s = engine.cmd.var(1).as_slice()?;
    if quiet {
        engine.cc.stack.push(boolean!(s.remaining_bits() >= l));
    } else if s.remaining_bits() < l {
        return err!(ExceptionCode::CellUnderflow);
    }
    Ok(())
}

fn schkrefs(engine: &mut Engine, name: &'static str, quiet: bool) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 2)?;
    let r = engine.cmd.var(0).as_integer()?.into(0..=4)?;
    let s = engine.cmd.var(1).as_slice()?;
    let refs_count = s.remaining_references();
    if quiet {
        engine.cc.stack.push(boolean!(refs_count >= r));
    } else if refs_count < r {
        return err!(ExceptionCode::CellUnderflow);
    }
    Ok(())
}

fn schkbitrefs(engine: &mut Engine, name: &'static str, quiet: bool) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 3)?;
    let r = engine.cmd.var(0).as_integer()?.into(0..=4)?;
    let l = engine.cmd.var(1).as_integer()?.into(0..=1023)?;
    let s = engine.cmd.var(2).as_slice()?;
    let data_len = s.remaining_bits();
    let refs_count = s.remaining_references();
    let status = l <= data_len && r <= refs_count;
    if quiet {
        engine.cc.stack.push(boolean!(status));
    } else if !status {
        return err!(ExceptionCode::CellUnderflow);
    }
    Ok(())
}

pub fn execute_schkbitsq(engine: &mut Engine) -> Status {
    schkbits(engine, "SCHKBITSQ", 1023, true)
}

pub fn execute_schkbits(engine: &mut Engine) -> Status {
    schkbits(engine, "SCHKBITS", 1023, false)
}

pub fn execute_schkrefsq(engine: &mut Engine) -> Status {
    schkrefs(engine, "SCHKREFSQ", true)
}

pub fn execute_schkrefs(engine: &mut Engine) -> Status {
    schkrefs(engine, "SCHKREFS", false)
}

pub fn execute_schkbitrefsq(engine: &mut Engine) -> Status {
    schkbitrefs(engine, "SCHKBITREFSQ", true)
}

pub fn execute_schkbitrefs(engine: &mut Engine) -> Status {
    schkbitrefs(engine, "SCHKBITREFS", false)
}

fn pldref(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    let mut inst = Instruction::new(name);
    let mut params = 1;
    if how.bit(STACK) {
        params += 1;
    } else if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Length(0..4));
    }
    engine.load_instruction(inst)?;
    fetch_stack(engine, params)?;
    let n = if how.bit(STACK) {
        engine.cmd.var(0).as_integer()?.into(0..=3)?
    } else if how.bit(CMD) {
        engine.cmd.length()
    } else {
        0
    };
    proc_slice(engine, 0, 0, |slice, _| Ok(StackItem::Cell(slice.reference(n)?)))
}

// (slice - cell)
pub fn execute_pldref(engine: &mut Engine) -> Status {
    pldref(engine, "PLDREF", 0)
}

// (slice - cell)
pub fn execute_pldrefidx(engine: &mut Engine) -> Status {
    pldref(engine, "PLDREFIDX", CMD)
}

// (slice n - cell)
pub fn execute_pldrefvar(engine: &mut Engine) -> Status {
    pldref(engine, "PLDREFVAR", STACK)
}

pub fn execute_sbits(engine: &mut Engine) -> Status {
    sbitrefs(engine, "SBITS", Target::Bits)
}

pub fn execute_srefs(engine: &mut Engine) -> Status {
    sbitrefs(engine, "SREFS", Target::Refs)
}

pub fn execute_sbitrefs(engine: &mut Engine) -> Status {
    sbitrefs(engine, "SBITREFS", Target::BitRefs)
}

// (slice - x slice)
pub fn execute_ldile4(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE4", 32, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_ldule4(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE4", 32, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_ldile8(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE8", 64, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_ldule8(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE8", 64, PARAM | STAY)
}

// (slice - x slice)
pub fn execute_pldile4(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE4", 32, PARAM)
}

// (slice - x slice)
pub fn execute_pldule4(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE4", 32, PARAM)
}

// (slice - x slice)
pub fn execute_pldile8(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE8", 64, PARAM)
}

// (slice - x slice)
pub fn execute_pldule8(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE8", 64, PARAM)
}

// (slice - x slice)
pub fn execute_ldile4q(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE4Q", 32, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_ldule4q(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE4Q", 32, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_ldile8q(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "LDILE8Q", 64, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_ldule8q(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "LDULE8Q", 64, PARAM | QUIET | STAY)
}

// (slice - x slice)
pub fn execute_pldile4q(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE4Q", 32, PARAM | QUIET)
}

// (slice - x slice)
pub fn execute_pldule4q(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE4Q", 32, PARAM | QUIET)
}

// (slice - x slice)
pub fn execute_pldile8q(engine: &mut Engine) -> Status {
    ld_int::<SignedIntegerLittleEndianEncoding>(engine, "PLDILE8Q", 64, PARAM | QUIET)
}

// (slice - x slice)
pub fn execute_pldule8q(engine: &mut Engine) -> Status {
    ld_int::<UnsignedIntegerLittleEndianEncoding>(engine, "PLDULE8Q", 64, PARAM | QUIET)
}

fn trim_leading_bits(slice: &mut SliceData, bit: u8) -> usize {
    let n = slice.remaining_bits();
    let bit = Some(bit == 1);
    let mut skipped = 0;
    while skipped < n && slice.get_bit_opt(skipped) == bit {
        skipped += 1;
    }
    slice.shrink_data(skipped..);
    skipped
}

fn ldbit(engine: &mut Engine, name: &'static str, bit: u8) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 1)?;
    let mut slice = engine.cmd.var(0).as_slice()?.clone();
    let skipped = trim_leading_bits(&mut slice, bit);
    engine.cc.stack.push(int!(skipped));
    engine.cc.stack.push(StackItem::Slice(slice));
    Ok(())
}

pub fn execute_ldzeroes(engine: &mut Engine) -> Status {
    ldbit(engine, "LDZEROES", 0)
}

pub fn execute_ldones(engine: &mut Engine) -> Status {
    ldbit(engine, "LDONES", 1)
}

pub fn execute_ldsame(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("LDSAME")
    )?;
    fetch_stack(engine, 2)?;
    let x = engine.cmd.var(0).as_integer()?.into(0..=1)?;
    let mut slice = engine.cmd.var(1).as_slice()?.clone();
    let skipped = trim_leading_bits(&mut slice, x as u8);
    engine.cc.stack.push(int!(skipped));
    engine.cc.stack.push(StackItem::Slice(slice));
    Ok(())
}

fn split(engine: &mut Engine, name: &'static str, quiet: bool) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 3)?;
    let r = engine.cmd.var(0).as_integer()?.into(0..=4)?;
    let l = engine.cmd.var(1).as_integer()?.into(0..=1023)?;
    let mut slice = engine.cmd.var(2).as_slice()?.clone();
    let data_len = slice.remaining_bits();
    let refs_count = slice.remaining_references();
    if (l > data_len) || (r > refs_count) {
        if quiet {
            engine.cc.stack.push(StackItem::Slice(slice));
            engine.cc.stack.push(boolean!(false));
            return Ok(());
        } else {
            return err!(ExceptionCode::CellUnderflow);
        }
    }
    let mut slice1 = slice.clone();
    slice.shrink_references(0..r);
    slice.shrink_data(0..l);
    slice1.shrink_references(r..);
    slice1.shrink_data(l..);
    engine.cc.stack.push(StackItem::Slice(slice));
    engine.cc.stack.push(StackItem::Slice(slice1));
    if quiet {
        engine.cc.stack.push(boolean!(true));
    }
    Ok(())
}

pub fn execute_split(engine: &mut Engine) -> Status {
    split(engine, "SPLIT", false)
}

pub fn execute_splitq(engine: &mut Engine) -> Status {
    split(engine, "SPLITQ", true)
}

fn datasize(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 2)?;
    // first check types of variables
    engine.cmd.var(0).as_integer()?;
    if !how.bit(CEL) {
        engine.cmd.var(1).as_slice()?;
    } else if !engine.cmd.var(1).is_null() {
        engine.cmd.var(1).as_cell()?;
    }
    let x = engine.cmd.var(0).as_integer()?;
    x.check_neg()?;
    let max = x.into(0..=std::i64::MAX).unwrap_or(std::i64::MAX) as usize;
    const CAPABILITIES: u64 = GlobalCapabilities::CapFastStorageStatBugfix as u64 | GlobalCapabilities::CapFastStorageStat as u64;
    let mut cells = 0;
    let mut bits = 0;
    let mut refs = 0;
    let result = if engine.check_capabilities(CAPABILITIES) {
        if let Ok(slice) = engine.cmd.var(1).as_slice() {
            refs = slice.remaining_references();
            bits = slice.remaining_bits();
            for i in 0..slice.remaining_references() {
                let cell = slice.reference(i)?;
                refs = refs.saturating_add(cell.tree_cell_count() as usize);
                bits = bits.saturating_add(cell.tree_bits_count() as usize);
            }
            cells = refs;
            cells <= max
        } else if let Ok(cell) = engine.cmd.var(1).as_cell() {
            cells = cell.tree_cell_count() as usize;
            bits = cell.tree_bits_count() as usize;
            refs = cells - 1;
            cells <= max
        } else {
            return err!(ExceptionCode::TypeCheckError, "item is neither Cell nor Slice")
        }
    } else {
        let mut visited = HashSet::new();
        let mut cell_stack = Vec::new();
        if let Ok(slice) = engine.cmd.var(1).as_slice() {
            refs = slice.remaining_references();
            bits = slice.remaining_bits();
            for i in 0..slice.remaining_references() {
                if let Ok(cell) = slice.reference(i) {
                    cell_stack.push(cell);
                }
            }
        } else if let Ok(cell) = engine.cmd.var(1).as_cell() {
            cell_stack.push(cell.clone());
        }
        loop {
            let Some(cell) = cell_stack.pop() else { break true };
            if visited.insert(cell.repr_hash()) {
                if max == cells {
                    break false
                }
                cells += 1;
                // Version 34 contains bug with cell loading without gas calculation. Some blocks with the bug were applied in mainnet, so we have to support it.
                let slice = if engine.block_version() == 34 {
                    SliceData::load_cell(cell)?
                } else {
                    engine.load_hashed_cell(cell, false)?
                };
                refs = refs.saturating_add(slice.remaining_references());
                bits = bits.saturating_add(slice.remaining_bits());
                for i in 0..slice.remaining_references() {
                    if let Ok(cell) = slice.reference(i) {
                        cell_stack.push(cell);
                    }
                }
            }
        }
    };
    if result {
        engine.cc.stack.push(int!(cells));
        engine.cc.stack.push(int!(bits));
        engine.cc.stack.push(int!(refs));
    } else if !how.bit(QUIET) {
        return err!(ExceptionCode::CellOverflow)
    }
    if how.bit(QUIET) {
        engine.cc.stack.push(boolean!(result));
    }
    Ok(())
}

/// CDATASIZE (c n - x y z)
pub(crate) fn execute_cdatasize(engine: &mut Engine) -> Status {
    datasize(engine, "CDATASIZE", CEL)
}

/// CDATASIZEQ (c n - x y z -1 or 0)
pub(crate) fn execute_cdatasizeq(engine: &mut Engine) -> Status {
    datasize(engine, "CDATASIZEQ", QUIET | CEL)
}

/// SDATASIZEQ (s n - x y z -1 or 0)
pub(crate) fn execute_sdatasize(engine: &mut Engine) -> Status {
    datasize(engine, "SDATASIZE", 0)
}

/// SDATASIZEQ (s n - x y z)
pub(crate) fn execute_sdatasizeq(engine: &mut Engine) -> Status {
    datasize(engine, "SDATASIZEQ", QUIET)
}

/// LDCONT (slice - cont slice')
pub fn execute_ldcont(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("LDCONT"))?;
    fetch_stack(engine, 1)?;
    let mut slice = engine.cmd.var(0).as_slice()?.clone();
    let cont = if engine.check_capabilities(ton_block::GlobalCapabilities::CapStcontNewFormat as u64) {
        ContinuationData::deserialize(&mut slice, engine)?
    } else {
        let (cont, gas) = ContinuationData::deserialize_old(&mut slice)?;
        engine.use_gas(gas);
        cont
    };
    engine.cc.stack.push_cont(cont);
    engine.cc.stack.push(StackItem::Slice(slice));
    Ok(())
}
