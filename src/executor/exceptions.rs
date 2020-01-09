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

use executor::continuation::{callx};
use executor::engine::Engine;
use executor::engine::storage::{fetch_stack, swap, copy_to_var};
use executor::microcode::{CTRL, VAR, SAVELIST, CC};
use executor::types::{Ctx, Instruction, InstructionOptions};
use stack::{ContinuationType, IntegerData, StackItem};
use std::ops::Range;
use std::sync::Arc;
use types::{Exception, ExceptionCode, Result};

//Utilities **********************************************************************************
//(c c' -)
//c'.nargs = c'.stack.depth + 2
//c'.savelist[2] = c2, cc.savelist[2] = c2
//c'.savelist[0] = cc, c.savelist[0] = cc
//callx c
fn init_try_catch(ctx: Ctx) -> Result<Ctx> {
    fetch_stack(ctx, 2)
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < ctx.engine.cmd.pargs() {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cmd.var(1).as_continuation()?;
        ctx.engine.cmd.var_mut(0).as_continuation_mut().map(|catch_cont| {
            catch_cont.type_of = ContinuationType::TryCatch;
            catch_cont.nargs = catch_cont.stack.depth() as isize + 2
        })?;
        ctx.engine.cmd.var_mut(1).as_continuation_mut().map(|try_cont|
            try_cont.remove_from_savelist(0)
        )?;
        if ctx.engine.ctrl(2).is_ok() {
            copy_to_var(ctx, ctrl!(2))
            .and_then(|ctx| swap(ctx, savelist!(var!(0), 2), var!(2)))
            .and_then(|ctx| copy_to_var(ctx, ctrl!(2)))
            .and_then(|ctx| swap(ctx, savelist!(CC, 2), var!(3)))
        } else {
            Ok(ctx)
        }
    })
     // special swaping for callx - it calls cont from var0, but now in var0 - catch cont
    .and_then(|ctx| swap(ctx, var!(0), var!(1)))
    .and_then(|ctx| swap(ctx, ctrl!(2), var!(1)))
    .and_then(|ctx| callx(ctx, 0))
    .and_then(|ctx| copy_to_var(ctx, ctrl!(0)))
    .and_then(|ctx| {
        let length = ctx.engine.cmd.var_count();
        swap(ctx, savelist!(ctrl!(2), 0), var!(length - 1))
    })
}

fn do_throw(ctx: Ctx, number_index: isize, value_index: isize) -> Result<Ctx> {
    ctx.engine.cmd.undo.clear();
    let number = if number_index < 0 {
        ctx.engine.cmd.integer() as usize
    } else {
        ctx.engine.cmd.var(number_index as usize).as_integer()?.into(0..=0xFFFF)?
    };
    let value = if value_index < 0 {
        int!(0)
    } else {
        ctx.engine.cmd.var(value_index as usize).clone()
    };
    Err(Exception::from_number_and_value(number, value, file!(), line!()))
}

//Handlers ***********************************************************************************

fn execute_throw(engine: &mut Engine, range: Range<isize>) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("THROW").set_opts(InstructionOptions::Integer(range)),
    )
    .and_then(|ctx| do_throw(ctx, -1, -1))
    .err()
}

// (=> throw 0 n)
pub(super) fn execute_throw_short(engine: &mut Engine) -> Option<Exception> {
    execute_throw(engine, 0..64)
}

// (=> throw 0 n)
pub(super) fn execute_throw_long(engine: &mut Engine) -> Option<Exception> {
    execute_throw(engine, 0..2048)
}

// helper for THROWIF/THROWIFNOT instructions
fn execute_throwif_throwifnot(engine: &mut Engine, reverse_condition: bool, range: Range<isize>) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(if reverse_condition {"THROWIFNOT"} else {"THROWIF"})
            .set_opts(InstructionOptions::Integer(range)),
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        if reverse_condition ^ ctx.engine.cmd.var(0).as_bool()? {
            do_throw(ctx, -1, -1)
        } else {
            Ok(ctx)
        }
    })
    .err()
}

pub(super) fn execute_throwif_short(engine: &mut Engine) -> Option<Exception> {
    execute_throwif_throwifnot(engine, false, 0..64)
}

pub(super) fn execute_throwif_long(engine: &mut Engine) -> Option<Exception> {
    execute_throwif_throwifnot(engine, false, 0..2048)
}

pub(super) fn execute_throwifnot_short(engine: &mut Engine) -> Option<Exception> {
    execute_throwif_throwifnot(engine, true, 0..64)
}

pub(super) fn execute_throwifnot_long(engine: &mut Engine) -> Option<Exception> {
    execute_throwif_throwifnot(engine, true, 0..2048)
}

// helper for THROWANYIF/THROWANYIFNOT instructions
fn execute_throwanyif_throwanyifnot(
    engine: &mut Engine, 
    reverse_condition: bool
) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(if reverse_condition {"THROWANYIFNOT"} else {"THROWANYIF"})
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        if reverse_condition ^ ctx.engine.cmd.var(0).as_bool()? {
            do_throw(ctx, 1, -1)
        } else {
            Ok(ctx)
        }
    })
    .err()
}

// (n f, f!=0 => throw 0 n)
pub(super) fn execute_throwanyif(engine: &mut Engine) -> Option<Exception> {
    execute_throwanyif_throwanyifnot(engine, false)
}

// (n f, f==0 => throw 0 n)
pub(super) fn execute_throwanyifnot(engine: &mut Engine) -> Option<Exception> {
    execute_throwanyif_throwanyifnot(engine, true)
}

// (n => throw 0 n)
pub(super) fn execute_throwany(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("THROWANY")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| do_throw(ctx, 0, -1))
    .err()
}

// (x => throw x n)
pub(super) fn execute_throwarg(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("THROWARG").set_opts(InstructionOptions::Integer(0..2048)),
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| do_throw(ctx, -1, 0))
    .err()
}

// (x n => throw x n)
pub(super) fn execute_throwargany(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("THROWARGANY")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| do_throw(ctx, 0, 1))
    .err()
}

// helper for THROWARGANYIF[NOT] instructions
fn execute_throwarganyif_throwarganyifnot(
    engine: &mut Engine, 
    reverse_condition: bool
) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(if reverse_condition {"THROWARGANYIFNOT"} else {"THROWARGANYIF"})
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        if reverse_condition ^ ctx.engine.cmd.var(0).as_bool()? {
            do_throw(ctx, 1, 2)
        } else {
            Ok(ctx)
        }
    })
    .err()
}

// (x n f, f!=0 => throw x n)
pub(super) fn execute_throwarganyif(engine: &mut Engine) -> Option<Exception> {
    execute_throwarganyif_throwarganyifnot(engine, false)
}

// (x n f, f==0 => throw x n)
pub(super) fn execute_throwarganyifnot(engine: &mut Engine) -> Option<Exception> {
    execute_throwarganyif_throwarganyifnot(engine, true)
}

// helper for THROWARGIF[NOT] instructions
fn execute_throwargif_throwargifnot(
    engine: &mut Engine, 
    reverse_condition: bool
) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(
            if reverse_condition {"THROWARGIFNOT"} else {"THROWARGIF"}
        ).set_opts(InstructionOptions::Integer(0..2048))
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        if reverse_condition ^ ctx.engine.cmd.var(0).as_bool()? {
            do_throw(ctx, -1, 1)
        } else {
            Ok(ctx)
        }
    })
    .err()
}

// (x f, f!=0 => throw x n)
pub(super) fn execute_throwargif(engine: &mut Engine) -> Option<Exception> {
    execute_throwargif_throwargifnot(engine, false)
}

// (x f, f==0 => throw x n)
pub(super) fn execute_throwargifnot(engine: &mut Engine) -> Option<Exception> {
    execute_throwargif_throwargifnot(engine, true)
}

// (c c' - ) 
pub(super) fn execute_try(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("TRY")
    )
    .and_then(|ctx| init_try_catch(ctx))
    .err()
}

// (c c' - ) 
//move 0<=p<=15 stack elements from cc to c, return 0<=r<=15 stack values of resulting stack of c or c'.
pub(super) fn execute_tryargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("TRYARGS").set_opts(InstructionOptions::ArgumentAndReturnConstraints)
    )
    .and_then(|ctx| init_try_catch(ctx))
    .err()
}
