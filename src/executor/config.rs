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
    executor::{engine::{Engine, storage::fetch_stack}, types::{InstructionOptions, Instruction}},
    stack::{StackItem, integer::IntegerData}, types::Failure
};
use std::sync::Arc;
use ton_types::{BuilderData, HashmapE, IBitstring};

fn execute_config_param(engine: &mut Engine, name: &'static str, opt: bool) -> Failure {
    engine.load_instruction(Instruction::new(name))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let index: i32 = ctx.engine.cmd.var(0).as_integer()?.into(std::i32::MIN..=std::i32::MAX)?;
        let params = HashmapE::with_hashmap(32, ctx.engine.config_param(9)?.as_dict()?.cloned());
        let mut key = BuilderData::new();
        key.append_i32(index)?;
        if let Some(value) = params.get_with_gas(key.into_cell()?.into(), ctx.engine)? {
            if let Some(value) = value.reference_opt(0) {
                ctx.engine.cc.stack.push(StackItem::Cell(value.clone()));
                if !opt {
                    ctx.engine.cc.stack.push(boolean!(true));
                }
                return Ok(ctx)
            }
        }
        let _ = match opt {
            true => ctx.engine.cc.stack.push(StackItem::None),
            false => ctx.engine.cc.stack.push(boolean!(false))
        };
        Ok(ctx)
    })
    .err()
}

// - t
pub(super) fn execute_balance(engine: &mut Engine) -> Failure {
    extract_config(engine, "BALANCE")
}

// ( - D 32)
pub(super) fn execute_config_dict(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("CONFIGDICT"))
    .and_then(|ctx| {
        let dict = ctx.engine.config_param(9)?.clone();
        ctx.engine.cc.stack.push(dict.clone());
        ctx.engine.cc.stack.push(int!(32));
        Ok(ctx)
    })
    .err()
}

/// (i - c?)
pub(super) fn execute_config_opt_param(engine: &mut Engine) -> Failure {
    execute_config_param(engine, "CONFIGOPTPARAM", true)
}

/// (i - c -1 or 0)
pub(super) fn execute_config_ref_param(engine: &mut Engine) -> Failure {
    execute_config_param(engine, "CONFIGPARAM", false)
}

fn extract_config(engine: &mut Engine, name: &'static str) -> Failure {
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::Length(0..16))
    )
    .and_then(|ctx| {
        let value = ctx.engine.config_param(ctx.engine.cmd.length())?.clone();
        ctx.engine.cc.stack.push(value);
        Ok(ctx)
    })
    .err()
}

// - D
pub(super) fn execute_config_root(engine: &mut Engine) -> Failure {
    extract_config(engine, "CONFIGROOT")
}

// - x
pub(super) fn execute_getparam(engine: &mut Engine) -> Failure {
    extract_config(engine, "GETPARAM")
}

// - integer
pub(super) fn execute_now(engine: &mut Engine) -> Failure {
    extract_config(engine, "NOW")
}

// - integer
pub(super) fn execute_blocklt(engine: &mut Engine) -> Failure {
     extract_config(engine, "BLOCKLT")
}

// - integer
pub(super) fn execute_ltime(engine: &mut Engine) -> Failure {
    extract_config(engine, "LTIME")
}

// - slice
pub(super) fn execute_my_addr(engine: &mut Engine) -> Failure {
    extract_config(engine, "MYADDR")
}

// - cell
pub(super) fn execute_my_code(engine: &mut Engine) -> Failure {
    extract_config(engine, "MYCODE")
}

// - x
pub(super) fn execute_randseed(engine: &mut Engine) -> Failure {
    extract_config(engine, "RANDSEED")
}

// - integer | none
pub(super) fn execute_init_code_hash(engine: &mut Engine) -> Failure {
    extract_config(engine, "INITCODEHASH")
}
