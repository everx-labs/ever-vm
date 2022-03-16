/*
* Copyright (C) 2019-2021 TON Labs. All Rights Reserved.
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
        serialize_currency_collection, engine::{Engine, storage::fetch_stack},
        types::{Instruction}
    },
    stack::{
        StackItem,
        integer::{
            IntegerData, behavior::OperationBehavior,
            serialization::UnsignedIntegerBigEndianEncoding
        }
    },
    types::{
        Exception, Status
    }
};
use num::{BigInt, bigint::Sign};
use std::sync::Arc;
use ton_block::{
    ACTION_CHANGE_LIB, ACTION_RESERVE, ACTION_SEND_MSG, ACTION_SET_CODE,
    GlobalCapabilities
};
use ton_types::{
    BuilderData, Cell, error, GasConsumer, IBitstring, Result, SliceData,
    types::ExceptionCode,
};

fn get_bigint(slice: &SliceData) -> BigInt {
    let bits = slice.remaining_bits();
    if bits == 0 {
        BigInt::from(0)
    } else if bits < 256 {
        BigInt::from_bytes_be(Sign::Plus, &slice.get_bytestring(0)) << (256 - bits)
    } else {
        BigInt::from_bytes_be(Sign::Plus, &slice.get_bytestring(0)[..32])
    }
}


// Blockchain related instructions ********************************************

fn add_action(engine: &mut Engine, action_id: u32, cell: Option<Cell>, suffix: BuilderData) -> Status {
    let mut new_action = BuilderData::new();
    let c5 = engine.ctrls.get(5).ok_or(ExceptionCode::TypeCheckError)?;
    new_action.append_reference_cell(c5.as_cell()?.clone());
    new_action.append_u32(action_id)?.append_builder(&suffix)?;
    if let Some(cell) = cell {
        new_action.append_reference_cell(cell);
    }
    let cell = engine.finalize_cell(new_action)?;
    engine.ctrls.put(5, &mut StackItem::Cell(cell))?;
    Ok(())
}

/// CHANGELIB (h x - )
pub(super) fn execute_changelib(engine: &mut Engine) -> Status {
    if !engine.check_capabilities(GlobalCapabilities::CapSetLibCode as u64) {
        return Status::Err(ExceptionCode::InvalidOpcode.into());
    }
    engine.load_instruction(Instruction::new("CHANGELIB"))?;
    fetch_stack(engine, 2)?;
    let x = engine.cmd.var(0).as_integer()?.into(0..=2)? as u8;
    let hash = engine.cmd.var(1).as_integer()?.as_builder::<UnsignedIntegerBigEndianEncoding>(256)?;
    let mut suffix = BuilderData::with_raw(vec![x * 2], 8)?;
    suffix.append_builder(&hash)?;
    add_action(engine, ACTION_CHANGE_LIB, None, suffix)
}

/// SENDRAWMSG (c x – ): pop mode and message cell from stack and put it at the
/// end of output actions list.
pub(super) fn execute_sendrawmsg(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("SENDRAWMSG"))?;
    fetch_stack(engine, 2)?;
    let x = engine.cmd.var(0).as_integer()?.into(0..=255)?;
    let cell = engine.cmd.var(1).as_cell()?.clone();
    let suffix = BuilderData::with_raw(vec![x], 8)?;
    add_action(engine, ACTION_SEND_MSG, Some(cell), suffix)
}

/// SETCODE (c - )
pub(super) fn execute_setcode(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("SETCODE"))?;
    fetch_stack(engine, 1)?;
    let cell = engine.cmd.var(0).as_cell()?.clone();
    add_action(engine, ACTION_SET_CODE, Some(cell), BuilderData::new())
}

/// SETLIBCODE (c x - )
pub(super) fn execute_setlibcode(engine: &mut Engine) -> Status {
    if !engine.check_capabilities(GlobalCapabilities::CapSetLibCode as u64) {
        return Status::Err(ExceptionCode::InvalidOpcode.into());
    }
    engine.load_instruction(Instruction::new("SETLIBCODE"))?;
    fetch_stack(engine, 2)?;
    let x = engine.cmd.var(0).as_integer()?.into(0..=2)? as u8;
    let cell = engine.cmd.var(1).as_cell()?.clone();
    add_action(engine, ACTION_CHANGE_LIB, Some(cell), BuilderData::with_raw(vec![x * 2 + 1], 8)?)
}

/// RAWRESERVE (x y - )
pub(super) fn execute_rawreserve(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("RAWRESERVE"))?;
    fetch_stack(engine, 2)?;
    let y = engine.cmd.var(0).as_integer()?.into(0..=15)?;
    let mut suffix = BuilderData::with_raw(vec![y], 8)?;
    let x = engine.cmd.var(1).as_grams()?;
    suffix.append_builder(&serialize_currency_collection(x, None)?)?;
    add_action(engine, ACTION_RESERVE, None, suffix)
}

/// RAWRESERVEX (s y - )
pub(super) fn execute_rawreservex(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("RAWRESERVEX"))?;
    fetch_stack(engine, 3)?;
    let y = engine.cmd.var(0).as_integer()?.into(0..=15)?;
    let mut suffix = BuilderData::with_raw(vec![y], 8)?;
    let other = engine.cmd.var(1).as_dict()?;
    let x = engine.cmd.var(2).as_grams()?;
    suffix.append_builder(&serialize_currency_collection(x, other.cloned())?)?;
    add_action(engine, ACTION_RESERVE, None, suffix)
}

pub(super) fn execute_ldmsgaddr<T: OperationBehavior>(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new(if T::quiet() {"LDMSGADDRQ"} else {"LDMSGADDR"})
    )?;
    fetch_stack(engine, 1)?;
    let mut slice = engine.cmd.var(0).as_slice()?.clone();
    let mut remainder = slice.clone();
    if parse_address(&mut remainder).is_ok() {
        slice.shrink_by_remainder(&remainder);
        engine.cc.stack.push(StackItem::Slice(slice));
        engine.cc.stack.push(StackItem::Slice(remainder));
        if T::quiet() {
            engine.cc.stack.push(boolean!(true));
        }
        Ok(())
    } else if T::quiet() {
        let var = engine.cmd.pop_var()?;
        engine.cc.stack.push(var);
        engine.cc.stack.push(boolean!(false));
        Ok(())
    } else {
        err!(ExceptionCode::CellUnderflow)
    }
}

fn load_address<F, T>(engine: &mut Engine, name: &'static str, op: F) -> Status
where F: FnOnce(Vec<StackItem>, &mut dyn GasConsumer) -> Result<Vec<StackItem>>, T: OperationBehavior {
    engine.load_instruction(Instruction::new(name))?;
    fetch_stack(engine, 1)?;
    let mut slice = engine.cmd.var(0).as_slice()?.clone();
    let mut result = false;
    if let Ok(addr) = parse_address(&mut slice) {
        if let Ok(mut stack) = op(addr, engine) {
            stack.drain(..).for_each(|var| {engine.cc.stack.push(var);});
            result = true;
        }
    }
    if T::quiet() {
        engine.cc.stack.push(boolean!(result));
        Ok(())
    } else if result {
        Ok(())
    } else {
        err!(ExceptionCode::CellUnderflow)
    }
}

pub(super) fn execute_parsemsgaddr<T: OperationBehavior>(engine: &mut Engine) -> Status {
    load_address::<_, T>(engine, if T::quiet() {"PARSEMSGADDRQ"} else {"PARSEMSGADDR"},
        |tuple, _| Ok(vec![StackItem::tuple(tuple)])
    )
}

// (s - x y) compose rewrite_pfx and address to a 256 bit integer
pub(super) fn execute_rewrite_std_addr<T: OperationBehavior>(engine: &mut Engine) -> Status {
    load_address::<_, T>(engine, if T::quiet() {"REWRITESTDADDRQ"} else {"REWRITESTDADDR"}, |tuple, _| {
        if tuple.len() == 4 {
            let addr = tuple[3].as_slice()?;
            let mut y = match addr.remaining_bits() {
                256 => IntegerData::from(get_bigint(addr))?,
                _ => return err!(ExceptionCode::CellUnderflow)
            };
            if tuple[1].is_slice() {
                let rewrite_pfx = tuple[1].as_slice()?;
                let bits = rewrite_pfx.remaining_bits();
                if bits > 256 {
                    return err!(ExceptionCode::CellUnderflow)
                } else if bits > 0 {
                    let prefix = IntegerData::from(get_bigint(rewrite_pfx))?;
                    let mask = IntegerData::mask(256 - bits);
                    y = y.and::<T>(&mask)?.or::<T>(&prefix)?;
                }
            };
            let x = tuple[2].clone();
            Ok(vec![x, StackItem::Integer(Arc::new(y))])
        } else {
            return err!(ExceptionCode::CellUnderflow)
        }
    })
}

// (s - x s') compose rewrite_pfx and address to a slice
pub(super) fn execute_rewrite_var_addr<T: OperationBehavior>(engine: &mut Engine) -> Status {
    load_address::<_, T>(engine, if T::quiet() {"REWRITEVARADDRQ"} else {"REWRITEVARADDR"}, |tuple, gas_consumer| {
        if tuple.len() == 4 {
            let mut addr = tuple[3].as_slice()?.clone();
            if let Ok(rewrite_pfx) = tuple[1].as_slice() {
                let bits = rewrite_pfx.remaining_bits();
                if bits > addr.remaining_bits() {
                    return err!(ExceptionCode::CellUnderflow)
                } else if bits > 0 {
                    let mut b = BuilderData::from_slice(rewrite_pfx);
                    addr.shrink_data(bits..);
                    b.append_bytestring(&addr)?;
                    addr = gas_consumer.finalize_cell_and_load(b)?;
                }
            };
            let x = tuple[2].clone();
            Ok(vec![x, StackItem::Slice(addr)])
        } else {
            return err!(ExceptionCode::CellUnderflow)
        }
    })
}

fn read_rewrite_pfx(cell: &mut SliceData) -> Result<Option<SliceData>> {
    match cell.get_next_bit()? {
        true => {
            let len = cell.get_next_int(5)?;
            Ok(Some(cell.get_next_slice(len as usize)?))
        }
        false => Ok(None)
    }
}

fn parse_address(cell: &mut SliceData) -> Result<Vec<StackItem>> {
    let addr_type = cell.get_next_int(2)? as u8;
    let mut tuple = vec!(int!(addr_type));
    match addr_type & 0b11 {
        0b00 => (),
        0b01 => {
            let len = cell.get_next_int(9)?;
            tuple.push(StackItem::Slice(cell.get_next_slice(len as usize)?));
        }
        0b10 => {
            tuple.push(read_rewrite_pfx(cell)?
                .map(StackItem::Slice)
                .unwrap_or(StackItem::None));
            tuple.push(int!(cell.get_next_byte()? as i8));
            tuple.push(StackItem::Slice(cell.get_next_slice(256)?));
        }
        0b11 => {
            tuple.push(read_rewrite_pfx(cell)?
                .map(StackItem::Slice)
                .unwrap_or(StackItem::None));
            let len = cell.get_next_int(9)?;
            tuple.push(int!(cell.get_next_i32()?));
            tuple.push(StackItem::Slice(cell.get_next_slice(len as usize)?));
        }
        _ => ()
    }
    Ok(tuple)
}
