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
    executor::{engine::{Engine, storage::fetch_stack}, types::{InstructionOptions, Instruction}},
    stack::{StackItem, integer::IntegerData}, types::Status
};
use ton_block::GlobalCapabilities;

fn execute_config_param(engine: &mut Engine, name: &'static str, opt: bool) -> Status {
    engine.load_instruction(Instruction::new(name))?;
    fetch_stack(engine, 1)?;
    let index: i32 = engine.cmd.var(0).as_integer()?.into(std::i32::MIN..=std::i32::MAX)?;
    if let Some(value) = engine.get_config_param(index)? {
        engine.cc.stack.push(StackItem::Cell(value));
        if !opt {
            engine.cc.stack.push(boolean!(true));
        }
    } else {
        let value = match opt {
            true => StackItem::None,
            false => boolean!(false)
        };
        engine.cc.stack.push(value);
    }
    Ok(())
}

// - t
pub(super) fn execute_balance(engine: &mut Engine) -> Status {
    extract_config(engine, "BALANCE")
}

// ( - D 32)
pub(super) fn execute_config_dict(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("CONFIGDICT"))?;
    let dict = engine.smci_param(9)?.clone();
    engine.cc.stack.push(dict);
    engine.cc.stack.push(int!(32));
    Ok(())
}

/// (i - c?)
pub(super) fn execute_config_opt_param(engine: &mut Engine) -> Status {
    execute_config_param(engine, "CONFIGOPTPARAM", true)
}

/// (i - c -1 or 0)
pub(super) fn execute_config_ref_param(engine: &mut Engine) -> Status {
    execute_config_param(engine, "CONFIGPARAM", false)
}

fn extract_config(engine: &mut Engine, name: &'static str) -> Status {
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::Length(0..16))
    )?;
    let value = engine.smci_param(engine.cmd.length())?.clone();
    engine.cc.stack.push(value);
    Ok(())
}

// - D
pub(super) fn execute_config_root(engine: &mut Engine) -> Status {
    extract_config(engine, "CONFIGROOT")
}

// - x
pub(super) fn execute_getparam(engine: &mut Engine) -> Status {
    extract_config(engine, "GETPARAM")
}

// - integer
pub(super) fn execute_now(engine: &mut Engine) -> Status {
    extract_config(engine, "NOW")
}

// - integer
pub(super) fn execute_blocklt(engine: &mut Engine) -> Status {
     extract_config(engine, "BLOCKLT")
}

// - integer
pub(super) fn execute_ltime(engine: &mut Engine) -> Status {
    extract_config(engine, "LTIME")
}

// - slice
pub(super) fn execute_my_addr(engine: &mut Engine) -> Status {
    extract_config(engine, "MYADDR")
}

// - cell
pub(super) fn execute_my_code(engine: &mut Engine) -> Status {
    engine.check_capability(GlobalCapabilities::CapMycode)?;
    extract_config(engine, "MYCODE")
}

// - x
pub(super) fn execute_randseed(engine: &mut Engine) -> Status {
    extract_config(engine, "RANDSEED")
}

// - integer | none
pub(super) fn execute_init_code_hash(engine: &mut Engine) -> Status {
    engine.check_capability(GlobalCapabilities::CapInitCodeHash)?;
    extract_config(engine, "INITCODEHASH")
}

// - integer
pub(super) fn execute_seq_no(engine: &mut Engine) -> Status {
    engine.check_capability(GlobalCapabilities::CapDelections)?;
    extract_config(engine, "SEQNO")
}

// - integer
pub(super) fn execute_storage_fees_collected(engine: &mut Engine) -> Status {
    engine.check_capability(GlobalCapabilities::CapStorageFeeToTvm)?;
    extract_config(engine, "STORAGEFEE")
}
