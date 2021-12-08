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
    executor::{Mask, engine::{Engine, storage::fetch_stack}, types::Instruction},
    stack::{StackItem, integer::IntegerData}, types::Status
};
use std::sync::Arc;

pub(super) fn execute_null(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("NULL")
    )?;
    engine.cc.stack.push(StackItem::None);
    Ok(())
}

pub(super) fn execute_isnull(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ISNULL")
    )?;
    fetch_stack(engine, 1)?;
    let result = engine.cmd.var(0).is_null();
    engine.cc.stack.push(boolean!(result));
    Ok(())
}

const ARG: u8 = 0x03;     // args number
const DBL: u8 = 0x04;     // DouBLe NULL in result
const INV: u8 = 0x08;     // INVert rule to get output value: get it upon unsuccessful call
const ZERO: u8 = 0xA0;    // zeroswapif instead nullswapif

fn nullzeroswapif(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    let args = how.mask(ARG);
    debug_assert!(args == 1 || args == 2);
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, args as usize)?;
    let (attr, new_element) = if how.bit(ZERO) {
        (!engine.cmd.var(0).is_null(), int!(0))
    } else {
        (engine.cmd.var(0).as_bool()?, StackItem::None)
    };
    if attr ^ how.bit(INV) {
        if how.bit(DBL) {
            engine.cc.stack.push(new_element.clone());
        }
        engine.cc.stack.push(new_element);
    }
    if args > 1 {
        engine.cc.stack.push(engine.cmd.vars.remove(1));
    }
    engine.cc.stack.push(engine.cmd.vars.remove(0));
    Ok(())
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapif(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLSWAPIF", 1)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapif2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLSWAPIF2", 1 | DBL)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapifnot(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLSWAPIFNOT", 1 | INV)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapifnot2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLSWAPIFNOT2", 1 | INV | DBL)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrif(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLROTRIF", 2)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrif2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLROTRIF2", 2 | DBL)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrifnot(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLROTRIFNOT", 2 | INV)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrifnot2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "NULLROTRIFNOT2", 2 | INV | DBL)
}

// cell - (cell) | (0 cell)
pub(super) fn execute_zeroswapif(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROSWAPIF", 1 | ZERO)
}

// cell - (cell) | (0 0 cell)
pub(super) fn execute_zeroswapif2(engine: &mut Engine) -> Status { nullzeroswapif(engine, "ZEROSWAPIF2", 1 | DBL | ZERO) }

// cell - (cell) | (0 cell)
pub(super) fn execute_zeroswapifnot(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROSWAPIFNOT", 1 | INV | ZERO)
}

// cell - (cell) | (0 0 cell)
pub(super) fn execute_zeroswapifnot2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROSWAPIFNOT2", 1 | INV | DBL | ZERO)
}

// cell cell - (cell cell) | (0 cell cell)
pub(super) fn execute_zerorotrif(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROROTRIF", 2 | ZERO)
}

// cell cell - (cell cell) | (0 0 cell cell)
pub(super) fn execute_zerorotrif2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROROTRIF2", 2 | DBL | ZERO)
}

// cell cell - (cell cell) | (0 0 cell cell)
pub(super) fn execute_zerorotrifnot(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROROTRIFNOT", 2 | INV | ZERO)
}

// cell cell - (cell ) | (0 0 cell cell)
pub(super) fn execute_zerorotrifnot2(engine: &mut Engine) -> Status {
    nullzeroswapif(engine, "ZEROROTRIFNOT2", 2 | INV | DBL | ZERO)
}
