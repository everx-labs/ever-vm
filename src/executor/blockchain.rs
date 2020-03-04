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
use executor::engine::storage::fetch_stack;
use executor::serialize_currency_collection;
use executor::gas::gas_state::Gas;
use executor::types::{Instruction, Ctx};
use stack::{Cell, IBitstring, IntegerData, BuilderData, SliceData, StackItem};
use stack::integer::behavior::OperationBehavior;
use stack::integer::serialization::{IntoSliceExt, UnsignedIntegerBigEndianEncoding};
use std::sync::Arc;
use types::{Exception, ExceptionCode, Failure, Result};
use types::{ACTION_RESERVE, ACTION_SEND_MSG, ACTION_SET_CODE, ACTION_CHANGE_LIB};

// Blockchain related instructions ********************************************

fn add_action(ctx: Ctx, action_id: u32, cell: Option<Cell>, suffix: BuilderData) -> Result<Ctx> {
    let mut new_action = BuilderData::new();
    new_action.append_u32(action_id)?.append_builder(&suffix)?;
    let c5 = ctx.engine.ctrls.get(5).ok_or(exception!(ExceptionCode::TypeCheckError))?;
    new_action.append_reference_cell(c5.as_cell()?.clone());
    if let Some(cell) = cell {
        new_action.append_reference_cell(cell);
    }
    ctx.engine.ctrls.put(5, &mut StackItem::Cell(new_action.finalize(&mut ctx.engine.gas)))?;
    Ok(ctx)
}

/// CHANGELIB (h x - )
pub(super) fn execute_changelib(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("CHANGELIB"))
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x = ctx.engine.cmd.var(0).as_integer()?.into(0..=2)? as u8;
        let hash = ctx.engine.cmd.var(1).as_integer()?.into_builder::<UnsignedIntegerBigEndianEncoding>(256)?;
        let mut suffix = BuilderData::with_raw(vec![x * 2], 8)?;
        suffix.append_builder(&hash)?;
        add_action(ctx, ACTION_CHANGE_LIB, None, suffix)
    })
    .err()
}

/// SENDRAWMSG (c x – ): pop mode and message cell from stack and put it at the
/// end of output actions list.
pub(super) fn execute_sendrawmsg(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("SENDRAWMSG"))
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x = ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?;
        let cell = ctx.engine.cmd.var(1).as_cell()?.clone();
        let suffix = BuilderData::with_raw(vec![x], 8)?;
        add_action(ctx, ACTION_SEND_MSG, Some(cell), suffix)
    })
    .err()
}

/// SETCODE (c - )
pub(super) fn execute_setcode(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("SETCODE"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let cell = ctx.engine.cmd.var(0).as_cell()?.clone();
        add_action(ctx, ACTION_SET_CODE, Some(cell), BuilderData::new())
    })
    .err()
}

/// SETLIBCODE (c x - )
pub(super) fn execute_setlibcode(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("SETLIBCODE"))
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let x = ctx.engine.cmd.var(0).as_integer()?.into(0..=2)? as u8;
        let cell = ctx.engine.cmd.var(1).as_cell()?.clone();
        add_action(ctx, ACTION_CHANGE_LIB, Some(cell), BuilderData::with_raw(vec![x * 2 + 1], 8)?)
    })
    .err()
}

/// RAWRESERVE (x y - )
pub(super) fn execute_rawreserve(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("RAWRESERVE"))
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let y = ctx.engine.cmd.var(0).as_integer()?.into(0..=15)?;
        let mut suffix = BuilderData::with_raw(vec![y], 8)?;
        let x = ctx.engine.cmd.var(1).as_grams()?;
        suffix.append_builder(&serialize_currency_collection(x, None)?)?;
        add_action(ctx, ACTION_RESERVE, None, suffix)
    })
    .err()
}

/// RAWRESERVEX (s y - )
pub(super) fn execute_rawreservex(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("RAWRESERVEX"))
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let y = ctx.engine.cmd.var(0).as_integer()?.into(0..=15)?;
        let mut suffix = BuilderData::with_raw(vec![y], 8)?;
        let other = ctx.engine.cmd.var(1).as_dict()?;
        let x = ctx.engine.cmd.var(2).as_grams()?;
        suffix.append_builder(&serialize_currency_collection(x, other.cloned())?)?;
        add_action(ctx, ACTION_RESERVE, None, suffix)
    })
    .err()
}

pub(super) fn execute_ldmsgaddr<T: OperationBehavior>(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(if T::quiet() {"LDMSGADDRQ"} else {"LDMSGADDR"})
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut slice = ctx.engine.cmd.var(0).as_slice()?.clone();
        let mut remainder = slice.clone();
        if parse_address(&mut remainder).is_ok() {
            slice.shrink_by_remainder(&remainder);
            ctx.engine.cc.stack.push(StackItem::Slice(slice));
            ctx.engine.cc.stack.push(StackItem::Slice(remainder));
            if T::quiet() {
                ctx.engine.cc.stack.push(boolean!(true));
            }
            Ok(ctx)
        } else if T::quiet() {
            let var = ctx.engine.cmd.vars.pop().unwrap();
            ctx.engine.cc.stack.push(var);
            ctx.engine.cc.stack.push(boolean!(false));
            Ok(ctx)
        } else {
            err!(ExceptionCode::CellUnderflow)
        }
    })
    .err()
}

fn load_address<F, T>(engine: &mut Engine, name: &'static str, op: F) -> Failure
where F: FnOnce(Vec<StackItem>, &mut Gas) -> Result<Vec<StackItem>>, T: OperationBehavior {
    engine.load_instruction(Instruction::new(name))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut slice = ctx.engine.cmd.var(0).as_slice()?.clone();
        let mut result = false;
        if let Ok(addr) = parse_address(&mut slice) {
            if let Ok(mut stack) = op(addr, &mut ctx.engine.gas) {
                stack.drain(..).for_each(|var| {ctx.engine.cc.stack.push(var);});
                result = true;
            }
        }
        if T::quiet() {
            ctx.engine.cc.stack.push(boolean!(result));
            Ok(ctx)
        } else if result {
            Ok(ctx)
        } else {
            err!(ExceptionCode::CellUnderflow)
        }
    })
    .err()
}

pub(super) fn execute_parsemsgaddr<T: OperationBehavior>(engine: &mut Engine) -> Option<Exception> {
    load_address::<_, T>(engine, if T::quiet() {"PARSEMSGADDRQ"} else {"PARSEMSGADDR"},
        |tuple, _| Ok(vec![StackItem::Tuple(tuple)])
    )
}

// (s - x y) compose rewrite_pfx and address to a 256 bit integer
pub(super) fn execute_rewrite_std_addr<T: OperationBehavior>(engine: &mut Engine) -> Option<Exception> {
    load_address::<_, T>(engine, if T::quiet() {"REWRITESTDADDRQ"} else {"REWRITESTDADDR"}, |tuple, _| {
        if tuple.len() == 4 {
            let addr = tuple[3].as_slice()?;
            let mut y = match addr.remaining_bits() {
                256 => IntegerData::from(addr.get_bigint(256))?,
                _ => err!(ExceptionCode::CellUnderflow)?
            };
            if let Ok(rewrite_pfx) = tuple[1].as_slice() {
                let bits = rewrite_pfx.remaining_bits();
                if bits > 256 {
                    return err!(ExceptionCode::CellUnderflow)
                } else if bits > 0 {
                    let prefix = IntegerData::from(rewrite_pfx.get_bigint(256))?;
                    y = y.and::<T>(
                        &IntegerData::one().shl::<T>(256 - bits)?.sub::<T>(&IntegerData::one())?
                    )?.or::<T>(&prefix)?;
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
pub(super) fn execute_rewrite_var_addr<T: OperationBehavior>(engine: &mut Engine) -> Option<Exception> {
    load_address::<_, T>(engine, if T::quiet() {"REWRITEVARADDRQ"} else {"REWRITEVARADDR"}, |tuple, gas| {
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
                    addr = b.finalize_and_load(gas);
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
                .map(|rewrite_pfx| StackItem::Slice(rewrite_pfx))
                .unwrap_or(StackItem::None));
            tuple.push(int!(cell.get_next_byte()? as i8));
            tuple.push(StackItem::Slice(cell.get_next_slice(256)?));
        }
        0b11 => {
            tuple.push(read_rewrite_pfx(cell)?
                .map(|rewrite_pfx| StackItem::Slice(rewrite_pfx))
                .unwrap_or(StackItem::None));
            let len = cell.get_next_int(9)?;
            tuple.push(int!(cell.get_next_i32()?));
            tuple.push(StackItem::Slice(cell.get_next_slice(len as usize)?));
        }
        _ => ()
    }
    Ok(tuple)
}
