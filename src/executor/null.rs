/*
* Copyright 2018-2019 TON DEV SOLUTIONS LTD.
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

use types::Exception;
use executor::engine::Engine;
use executor::types::Instruction;
use stack::{StackItem, IntegerData};
use executor::engine::storage::fetch_stack;
use std::sync::Arc;

pub(super) fn execute_null(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("NULL")
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(StackItem::None);
        Ok(ctx)
    })
    .err()
}

pub(super) fn execute_isnull(engine: &mut Engine) -> Option<Exception> {
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

fn nullswapif(engine: &mut Engine, name: &'static str, invert: bool, args: usize) -> Option<Exception> {
    debug_assert!(args == 1 || args == 2);
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, args))
    .and_then(|ctx| {
        if ctx.engine.cmd.var(0).as_bool()? ^ invert {
            ctx.engine.cc.stack.push(StackItem::None);
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
pub(super) fn execute_nullswapif(engine: &mut Engine) -> Option<Exception> {
    nullswapif(engine, "NULLSWAPIF", false, 1)
}

// integer - (integer) | (null integer)
pub(super) fn execute_nullswapifnot(engine: &mut Engine) -> Option<Exception> {
    nullswapif(engine, "NULLSWAPIFNOT", true, 1)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrif(engine: &mut Engine) -> Option<Exception> {
    nullswapif(engine, "NULLROTRIF", false, 2)
}

// x integer - (x integer) | (null x integer)
pub(super) fn execute_nullrotrifnot(engine: &mut Engine) -> Option<Exception> {
    nullswapif(engine, "NULLROTRIFNOT", true, 2)
}