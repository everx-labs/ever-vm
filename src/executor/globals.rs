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

use executor::gas::gas_state::Gas;
use executor::Mask;
use executor::types::{Instruction, InstructionOptions};
use executor::engine::{Engine, storage::fetch_stack};
use stack::StackItem;
use types::Failure;

const STACK: u8 = 0x02;
const CMD:   u8 = 0x04;
const SET:   u8 = 0x10;

fn execute_setget_globalvar(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    let mut inst = Instruction::new(name);
    let mut params = 0;
    if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Length(1..32))
    }
    if how.bit(STACK) {
        params += 1;
    }
    if how.bit(SET) {
        params += 1;
    }
    engine.load_instruction(inst)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        let k = if how.bit(STACK) {
            ctx.engine.cmd.var(0).as_integer()?.into(0..=254)?
        } else {
            ctx.engine.cmd.length()
        };
        if how.bit(SET) {
            let mut c7 = ctx.engine.ctrl_mut(7)?.as_tuple_mut()?;
            let x = ctx.engine.cmd.var_mut(params - 1).withdraw();
            if k < c7.len() {
                c7[k] = x;
            } else {
                c7.append(&mut vec![StackItem::None; k - c7.len()]);
                c7.push(x);
            }
            ctx.engine.gas.use_gas(Gas::tuple_gas_price(c7.len()));
            ctx.engine.ctrls.put(7, &mut StackItem::Tuple(c7))?;
        } else {
            let c7 = ctx.engine.ctrl(7)?.as_tuple()?;
            let x = c7.get(k).map(|value| value.clone()).unwrap_or_default();
            ctx.engine.cc.stack.push(x);
        }
        Ok(ctx)
    })
    .err()
}

// GETGLOBVAR (k–x), returns the k-th global variable for 0 ≤ k < 255. 
// Equivalent to PUSH c7; SWAP; INDEXVARQ
pub(super) fn execute_getglobvar(engine: &mut Engine) -> Failure {
    execute_setget_globalvar(engine, "GETGLOBVAR", STACK)
}

// GETGLOB k( –x), returns the k-th global variable for 1 ≤ k ≤ 31
// Equivalent to PUSH c7; INDEXQ k.
pub(super) fn execute_getglob(engine: &mut Engine) -> Failure {
    execute_setget_globalvar(engine, "GETGLOB", CMD)
}

// SETGLOBVAR (x k– ), assigns x to the k-th global variable for 0 ≤ k <255.
// Equivalent to PUSH c7; ROTREV; SETINDEXVARQ; POP c7.
pub(super) fn execute_setglobvar(engine: &mut Engine) -> Failure {
    execute_setget_globalvar(engine, "SETGLOBVAR", SET | STACK)
}

// SETGLOB k (x– ), assigns x to the k-th global variable for 1 ≤ k ≤ 31.
// Equivalent to PUSH c7; SWAP; SETINDEXQ k; POP c7
pub(super) fn execute_setglob(engine: &mut Engine) -> Failure {
    execute_setget_globalvar(engine, "SETGLOB", SET | CMD)
}