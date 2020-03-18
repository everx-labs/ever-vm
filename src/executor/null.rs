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

use types::Failure;
use executor::engine::Engine;
use executor::types::Instruction;
use stack::{StackItem, IntegerData};
use executor::engine::storage::fetch_stack;
use std::sync::Arc;
use super::Mask;

pub(super) fn execute_null(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("NULL")
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(StackItem::None);
        Ok(ctx)
    })
    .err()
}

pub(super) fn execute_isnull(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ISNULL")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let result = ctx.engine.cmd.var(0).is_null();
        ctx.engine.cc.stack.push(boolean!(result));
        Ok(ctx)
    })
    .err()
}

const ARG: u8 = 0x03;     // args number
const DBL: u8 = 0x04;     // DouBLe NULL in result
const INV: u8 = 0x08;     // INVert rule to get output value: get it upon unsuccessful call

fn nullswapif(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    let args = how.mask(ARG);
    debug_assert!(args == 1 || args == 2);
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, args as usize))
    .and_then(|ctx| {
        if ctx.engine.cmd.var(0).as_bool()? ^ how.bit(INV) {
            ctx.engine.cc.stack.push(StackItem::None);
            if how.bit(DBL) {
                ctx.engine.cc.stack.push(StackItem::None);
            }
        }
        if args > 1 {
            ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(1));
        }
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapif(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLSWAPIF", 1)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapif2(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLSWAPIF2", 1 | DBL)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapifnot(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLSWAPIFNOT", 1 | INV)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapifnot2(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLSWAPIFNOT2", 1 | INV | DBL)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrif(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLROTRIF", 2)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrif2(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLROTRIF2", 2 | DBL)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrifnot(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLROTRIFNOT", 2 | INV)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrifnot2(engine: &mut Engine) -> Failure {
    nullswapif(engine, "NULLROTRIFNOT2", 2 | INV | DBL)
}