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
    executor::{engine::{Engine, storage::fetch_stack}, types::Instruction},
    stack::{StackItem, integer::{IntegerData, conversion::FromInt}},
    types::{Exception, Status}
};
use std::sync::Arc;
use ton_types::{error, types::ExceptionCode};

pub mod gas_state;
use gas_state::SPEC_LIMIT;

// Application-specific primitives - A.10; Gas-related primitives - A.10.2
// ACCEPT - F800
pub fn execute_accept(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("ACCEPT"))?;
    engine.new_gas_limit(SPEC_LIMIT);
    Ok(())
}
// Application-specific primitives - A.11; Gas-related primitives - A.11.2
// SETGASLIMIT - F801
pub fn execute_setgaslimit(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("SETGASLIMIT"))?;
    fetch_stack(engine, 1)?;
    let gas_limit = engine.cmd.var(0).as_integer()?
        .take_value_of(|x| i64::from_int(x).ok())?;
    if gas_limit < engine.gas_used() {
        return err!(ExceptionCode::OutOfGas);
    }
    engine.new_gas_limit(gas_limit);
    Ok(())
}
// Application-specific primitives - A.11; Gas-related primitives - A.11.2
// BUYGAS - F802
pub fn execute_buygas(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("BUYGAS"))?;
    fetch_stack(engine, 1)?;
    let nanograms = engine.cmd.var(0).as_integer()?
        .take_value_of(|x| i64::from_int(x).ok())?;
    let gas_price = engine.get_gas().get_gas_price();
    engine.new_gas_limit(gas_price*nanograms);
    Ok(())
}
// Application-specific primitives - A.11; Gas-related primitives - A.11.2
// GRAMTOGAS - F804
pub fn execute_gramtogas(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("GRAMTOGAS"))?;
    fetch_stack(engine, 1)?;
    let nanograms_input = engine.cmd.var(0);
    let gas = if nanograms_input.as_integer()?.is_neg() {
        0
    } else {
        let nanograms = nanograms_input.as_integer()?.take_value_of(|x| i64::from_int(x).ok())?;
        let gas_price = engine.get_gas().get_gas_price();
        std::cmp::min(SPEC_LIMIT, gas_price * nanograms)
    };
    engine.cc.stack.push(int!(gas));
    Ok(())
}
// Application-specific primitives - A.10; Gas-related primitives - A.10.2
// GASTOGRAM - F805
pub fn execute_gastogram(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("GASTOGRAM"))?;
    fetch_stack(engine, 1)?;
    let gas = engine.cmd.var(0).as_integer()?.take_value_of(|x| i64::from_int(x).ok())?;
    let gas_price = engine.get_gas().get_gas_price();
    let nanogram_output = gas * gas_price;
    engine.cc.stack.push(int!(nanogram_output));
    Ok(())
}

// Application-specific primitives - A.11; Gas-related primitives - A.11.2
// COMMIT - F80F
pub fn execute_commit(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("COMMIT"))?;
    engine.commit();
    Ok(())
}
