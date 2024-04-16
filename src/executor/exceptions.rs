/*
* Copyright (C) 2019-2024 EverX. All Rights Reserved.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific EVERX DEV software governing permissions and
* limitations under the License.
*/

use crate::{
    error::TvmError,
    executor::{
        continuation::callx, engine::{Engine, storage::{fetch_stack, swap, copy_to_var}},
        microcode::{CTRL, VAR, SAVELIST, CC}, types::{Instruction, InstructionOptions}
    },
    stack::{StackItem, continuation::{ContinuationType, ContinuationData}, integer::IntegerData},
    types::{Exception, Status}
};
use std::ops::Range;
use ever_block::GlobalCapabilities;
use ever_block::{error, fail, types::ExceptionCode};

//Utilities **********************************************************************************
//(c c' -)
//c'.nargs = c'.stack.depth + 2
//c'.savelist[2] = c2, cc.savelist[2] = c2
//c'.savelist[0] = cc, c.savelist[0] = cc
//callx c
fn init_try_catch(engine: &mut Engine, keep: bool) -> Status {
    fetch_stack(engine, 2)?;
    if engine.cc.stack.depth() < engine.cmd.pargs() {
        return err!(ExceptionCode::StackUnderflow)
    }
    let depth: u32 = engine.cc.stack.depth().try_into()?;
    engine.cmd.var(1).as_continuation()?;
    let bugfix = engine.check_capabilities(GlobalCapabilities::CapsTvmBugfixes2022 as u64);
    engine.cmd.var_mut(0).as_continuation_mut().map(|catch_cont| {
        catch_cont.type_of = ContinuationType::TryCatch;
        if !bugfix {
            catch_cont.nargs = catch_cont.stack.depth() as isize + 2
        }
    })?;
    engine.cmd.var_mut(1).as_continuation_mut().map(|try_cont|
        try_cont.remove_from_savelist(0)
    )?;
    if engine.ctrl(2).is_ok() {
        copy_to_var(engine, ctrl!(2))?;
        swap(engine, savelist!(var!(0), 2), var!(2))?;
        copy_to_var(engine, ctrl!(2))?;
        swap(engine, savelist!(CC, 2), var!(3))?;
    }
    // special swapping for callx: it calls a cont from var0, but at this point var0 holds catch cont
    swap(engine, var!(0), var!(1))?;
    swap(engine, ctrl!(2), var!(1))?;
    callx(engine, 0, false)?;
    copy_to_var(engine, ctrl!(0))?;
    let length = engine.cmd.var_count();
    swap(engine, savelist!(ctrl!(2), 0), var!(length - 1))?;
    if keep {
        // envelope catch cont c2 into catch revert cont
        let revert_cont = ContinuationData::with_type(ContinuationType::CatchRevert(depth));
        engine.cmd.push_var(StackItem::continuation(revert_cont));
        let n = engine.cmd.var_count();
        swap(engine, savelist!(var!(n - 1), 0), ctrl!(2))?;
        swap(engine, ctrl!(2), var!(n - 1))?;
    }
    Ok(())
}

fn do_throw(engine: &mut Engine, number_index: isize, value_index: isize) -> Status {
    let number = if number_index < 0 {
        engine.cmd.integer() as usize
    } else {
        engine.cmd.var(number_index as usize).as_integer()?.into(0..=0xFFFF)?
    };
    let value = if value_index < 0 {
        int!(0)
    } else {
        engine.cmd.var(value_index as usize).clone()
    };
    fail!(TvmError::TvmExceptionFull(
        Exception::from_number_and_value(number, value, file!(), line!()), String::new()
    ))
}

//Handlers ***********************************************************************************

fn execute_throw(engine: &mut Engine, range: Range<isize>) -> Status {
    engine.load_instruction(
        Instruction::new("THROW").set_opts(InstructionOptions::Integer(range)),
    )?;
    do_throw(engine, -1, -1)
}

// (=> throw 0 n)
pub(super) fn execute_throw_short(engine: &mut Engine) -> Status {
    execute_throw(engine, 0..64)
}

// (=> throw 0 n)
pub(super) fn execute_throw_long(engine: &mut Engine) -> Status {
    execute_throw(engine, 0..2048)
}

// helper for THROWIF/THROWIFNOT instructions
fn execute_throwif_throwifnot(engine: &mut Engine, reverse_condition: bool, range: Range<isize>) -> Status {
    engine.load_instruction(
        Instruction::new(if reverse_condition {"THROWIFNOT"} else {"THROWIF"})
            .set_opts(InstructionOptions::Integer(range)),
    )?;
    fetch_stack(engine, 1)?;
    if reverse_condition ^ engine.cmd.var(0).as_bool()? {
        do_throw(engine, -1, -1)
    } else {
        Ok(())
    }
}

pub(super) fn execute_throwif_short(engine: &mut Engine) -> Status {
    execute_throwif_throwifnot(engine, false, 0..64)
}

pub(super) fn execute_throwif_long(engine: &mut Engine) -> Status {
    execute_throwif_throwifnot(engine, false, 0..2048)
}

pub(super) fn execute_throwifnot_short(engine: &mut Engine) -> Status {
    execute_throwif_throwifnot(engine, true, 0..64)
}

pub(super) fn execute_throwifnot_long(engine: &mut Engine) -> Status {
    execute_throwif_throwifnot(engine, true, 0..2048)
}

// helper for THROWANYIF/THROWANYIFNOT instructions
fn execute_throwanyif_throwanyifnot(
    engine: &mut Engine,
    reverse_condition: bool
) -> Status {
    engine.load_instruction(
        Instruction::new(if reverse_condition {"THROWANYIFNOT"} else {"THROWANYIF"})
    )?;
    fetch_stack(engine, 2)?;
    if reverse_condition ^ engine.cmd.var(0).as_bool()? {
        do_throw(engine, 1, -1)
    } else {
        Ok(())
    }
}

// (n f, f!=0 => throw 0 n)
pub(super) fn execute_throwanyif(engine: &mut Engine) -> Status {
    execute_throwanyif_throwanyifnot(engine, false)
}

// (n f, f==0 => throw 0 n)
pub(super) fn execute_throwanyifnot(engine: &mut Engine) -> Status {
    execute_throwanyif_throwanyifnot(engine, true)
}

// (n => throw 0 n)
pub(super) fn execute_throwany(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("THROWANY")
    )?;
    fetch_stack(engine, 1)?;
    do_throw(engine, 0, -1)
}

// (x => throw x n)
pub(super) fn execute_throwarg(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("THROWARG").set_opts(InstructionOptions::Integer(0..2048)),
    )?;
    fetch_stack(engine, 1)?;
    do_throw(engine, -1, 0)
}

// (x n => throw x n)
pub(super) fn execute_throwargany(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("THROWARGANY")
    )?;
    fetch_stack(engine, 2)?;
    do_throw(engine, 0, 1)
}

// helper for THROWARGANYIF[NOT] instructions
fn execute_throwarganyif_throwarganyifnot(
    engine: &mut Engine,
    reverse_condition: bool
) -> Status {
    engine.load_instruction(
        Instruction::new(if reverse_condition {"THROWARGANYIFNOT"} else {"THROWARGANYIF"})
    )?;
    fetch_stack(engine, 3)?;
    if reverse_condition ^ engine.cmd.var(0).as_bool()? {
        do_throw(engine, 1, 2)
    } else {
        Ok(())
    }
}

// (x n f, f!=0 => throw x n)
pub(super) fn execute_throwarganyif(engine: &mut Engine) -> Status {
    execute_throwarganyif_throwarganyifnot(engine, false)
}

// (x n f, f==0 => throw x n)
pub(super) fn execute_throwarganyifnot(engine: &mut Engine) -> Status {
    execute_throwarganyif_throwarganyifnot(engine, true)
}

// helper for THROWARGIF[NOT] instructions
fn execute_throwargif_throwargifnot(
    engine: &mut Engine,
    reverse_condition: bool
) -> Status {
    engine.load_instruction(
        Instruction::new(
            if reverse_condition {"THROWARGIFNOT"} else {"THROWARGIF"}
        ).set_opts(InstructionOptions::Integer(0..2048))
    )?;
    fetch_stack(engine, 2)?;
    if reverse_condition ^ engine.cmd.var(0).as_bool()? {
        do_throw(engine, -1, 1)
    } else {
        Ok(())
    }
}

// (x f, f!=0 => throw x n)
pub(super) fn execute_throwargif(engine: &mut Engine) -> Status {
    execute_throwargif_throwargifnot(engine, false)
}

// (x f, f==0 => throw x n)
pub(super) fn execute_throwargifnot(engine: &mut Engine) -> Status {
    execute_throwargif_throwargifnot(engine, true)
}

// (c c' - )
pub(super) fn execute_try(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("TRY")
    )?;
    init_try_catch(engine, false)
}

// (c c' - )
//move 0<=p<=15 stack elements from cc to c, return 0<=r<=15 stack values of resulting stack of c or c'.
pub(super) fn execute_tryargs(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("TRYARGS").set_opts(InstructionOptions::ArgumentAndReturnConstraints)
    )?;
    init_try_catch(engine, false)
}

pub(super) fn execute_trykeep(engine: &mut Engine) -> Status {
    if !engine.check_capabilities(GlobalCapabilities::CapsTvmBugfixes2022 as u64) {
        return Status::Err(ExceptionCode::InvalidOpcode.into());
    }
    engine.load_instruction(
        Instruction::new("TRYKEEP")
    )?;
    init_try_catch(engine, true)
}
