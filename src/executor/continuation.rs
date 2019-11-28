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

use executor::engine::Engine;
use executor::engine::data::convert;
use executor::engine::storage::{fetch_stack, swap, copy_to_var, pop_range, fetch_reference, apply_savelist, pop_all};
use executor::microcode::{VAR, SAVELIST, CC, CELL, CTRL, SLICE, CONTINUATION};
use executor::types::{
    Ctx,
    Instruction,
    InstructionOptions,
    InstructionParameter,
    Undo,
};
use executor::Mask;
use stack::integer::behavior::{
    Signaling,
};
use stack::{
    ContinuationData,
    ContinuationType,
    IntegerData,
    StackItem,
};
use types::{Exception, ExceptionCode, Result};
use std::mem;
use std::ops::{Range, RangeInclusive};
use std::sync::Arc;

const CALLX: u8 = 0x40;   // CALLX to found value
const SWITCH: u8 = 0x80;  // SWITCH to found value
const PREPARE: u8 = 0xC0; // pass found value to stack

// Utilities ******************************************************************

// (continuation - ), 
// pargs = cmd.pargs if any else cc.stack.depth
// if pargs > cc.stack.depth {
//     StackOverflow
// } 
// move_out = cc.stack[0..pargs]
// move_in  = if continuation.nargs < 0 { 
//     move_out
// } else if continuation.nargs > pargs { 
//     StackOverflow
// } else {
//     move_out[0..pargs + continuation.nargs - continuation.stack.depth]
// }
// cc.stack -= move_out, continuation.stack += move_in, 
// if cmd.nargs exists {
//     if cmd.nargs < 0 {
//         cc.nargs = -1
//     } else {
//         cc.nargs = cc.stack.depth + cmd.nargs
//     }    
// }
// continuation.stack.push(cc), cc = continuation, c[*] = cc.savelist[*]
fn callcc(ctx: Ctx, callee: usize) -> Result<Ctx> {
    let vars = ctx.engine.cmd.var_count();
    if  vars < callee {
        unimplemented!()
    } else if vars == callee {
        fetch_stack(ctx, 1)
    } else {
        Ok(ctx)
    }
    .and_then(|ctx| pop_all(ctx, var!(callee)))
    .and_then(|ctx| swap(ctx, var!(callee), CC))
    .and_then(|ctx| apply_savelist(ctx, 0..0))  
    .and_then(|ctx| {
        let mut old_cc = 
            ctx.engine.cmd.var_mut(callee).as_continuation_mut()?.withdraw();
        if let Some(nargs) = ctx.engine.cmd.ictx.nargs() {
            old_cc.nargs = nargs
        }
        ctx.engine.cc.stack.push_cont(old_cc);
        Ok(ctx)
    })
}

// (continuation - ), 
// pargs = cmd.pargs if any else cc.stack.depth
// if pargs > cc.stack.depth {
//     StackOverflow
// } 
// move_out = cc.stack[0..pargs]
// move_in  = if continuation.nargs < 0 { 
//     move_out
// } else if continuation.nargs > pargs { 
//     StackOverflow
// } else {
//     move_out[0..pargs + continuation.nargs - continuation.stack.depth]
// }
// cc.stack -= move_out, continuation.stack += move_in, 
// if cmd.nargs exists {
//     if cmd.nargs < 0 {
//         cc.nargs = -1
//     } else {
//         cc.nargs = cc.stack.depth + cmd.nargs
//     }    
// } else if cmd.rargs exists {
//     cc.nargs = cc.stack.depth + cmd.rargs
// }
// if continuation.savelist[0].is_none {
//     cc.savelist[0] = c[0]
//     continuation.savelist[0] = cc
// }
// cc = continuation, c[*] = cc.savelist[*]
pub(super) fn callx(ctx: Ctx, callee: usize) -> Result<Ctx> {
    let vars = ctx.engine.cmd.var_count();
    if  vars < callee {
        unimplemented!()
    } else if vars == callee {
        fetch_stack(ctx, 1)
    } else {
        Ok(ctx)
    }
    .and_then(|ctx| pop_all(ctx, var!(callee)))
    .and_then(|ctx| {
        let has_c0 = 
            ctx.engine.cmd.var(callee).as_continuation()?.savelist.get(0).is_some();
        if has_c0 {
            swap(ctx, var!(callee), CC)
        } else {
            swap(ctx, ctrl!(1), savelist!(CC, 1))
            .and_then(|ctx| swap(ctx, ctrl!(0), savelist!(CC, 0)))
            .and_then(|ctx| swap(ctx, var!(callee), CC))
            .and_then(|ctx| swap(ctx, var!(callee), ctrl!(0)))
        }
    })
    .and_then(|ctx| apply_savelist(ctx, 0..0))
    .and_then(|ctx| {
        if let Some(nargs) = ctx.engine.cmd.ictx.nargs() {
            continuation_mut_by_address!(ctx, ctrl!(0))?.nargs = nargs
        } else if let Some(rargs) = ctx.engine.cmd.ictx.rargs() {
            continuation_mut_by_address!(ctx, ctrl!(0))?.nargs = rargs as isize
        } else {
            continuation_mut_by_address!(ctx, ctrl!(0))?.nargs = -1;
        }
        Ok(ctx)
    })
}

type NRange = RangeInclusive<isize>;
type PRange = RangeInclusive<isize>;

fn fetch_nargs(ctx: Ctx, idx: usize, nrange: NRange) -> Result<Ctx> {
    let nargs = ctx.engine.cmd.var(idx).as_integer()?.into(nrange)?;
    ctx.engine.cmd.ictx.params.push(InstructionParameter::Nargs(nargs));
    Ok(ctx)
}

fn fetch_pargs(ctx: Ctx, idx: usize, prange: PRange) -> Result<Ctx> {
    let pargs = ctx.engine.cmd.var(idx).as_integer()?.into(prange)?;
    if pargs >= 0 {
        ctx.engine.cmd.ictx.params.push(InstructionParameter::Pargs(pargs as usize));
    }
    Ok(ctx)
}

fn fetch_nargs_pargs(ctx: Ctx, nrange: NRange, prange: PRange) -> Result<Ctx> {
    fetch_nargs(ctx, 0, nrange).and_then(|ctx| fetch_pargs(ctx, 1, prange))
}

// (continuation - ), 
// move_out = cc.stack
// move_in  = if continuation.nargs < 0 { 
//     move_out
// } else if continuation.nargs > cc.stack.depth { 
//     StackOverflow
// } else {
//     move_out[0..cc.stack.depth + continuation.nargs - continuation.stack.depth]
// }
// cc.stack -= move_out, continuation.stack += move_in, 
// tmp = cc, cc = continuation, c[*] = cc.savelist[*], cc.stack.push(slice(tmp))
fn jmpxdata(ctx: Ctx) -> Result<Ctx> {
    pop_all(ctx, var!(0))
    .and_then(|ctx| swap(ctx, var!(0), CC))
    .and_then(|ctx| apply_savelist(ctx, 0..0))  
    .and_then(|ctx| {
        let slice = ctx.engine.cmd.var(0).as_continuation()?.code().clone();
        ctx.engine.cc.stack.push(StackItem::Slice(slice));
        Ok(ctx)
    })
}

// checks special case for REPEAT*, UNTIL*, WHILE* 
// works as a continue, not as a break
pub(super) fn ret(ctx: Ctx) -> Result<Ctx> {
    match ctx.engine.cc.type_of {
        ContinuationType::RepeatLoopBody(_, _) => {
            ctx.engine.cc.move_to_end();
            Ok(ctx)
        },
        _ => switch(ctx, ctrl!(0))
    }
}

fn retalt(ctx: Ctx) -> Result<Ctx> {
    switch(ctx, ctrl!(1))
}

// ( - ), if c[?].savelist[i].is_none() { c[?].savelist[i] = c[i] }
fn save(ctx: Ctx, index: usize) -> Result<Ctx> {
    let creg = ctx.engine.cmd.creg();
    let skip = match ctx.engine.ctrls.get(index) {
        Some(c) => c.as_continuation()?.savelist.get(creg).is_some(),
        None => return err!(ExceptionCode::TypeCheckError)
    };
    if !skip {
        let v = ctx.engine.cmd.var_count() as u16;
        copy_to_var(ctx, ctrl!(creg))
        .and_then(|ctx| swap(ctx, var!(v), savelist!(ctrl!(index), creg)))
    } else {
        Ok(ctx)
    }
}

// (x1 ... xR y {R N} - continuation), y->continuation, continuation.stack.push(x1 ... xR)
fn setcont(ctx: Ctx, v: usize, need_to_convert: bool) -> Result<Ctx> {
    fetch_stack(ctx, v + 1) // fetch slice or continuation from stack and nargs/parags
    .and_then(|ctx| match v {
        0 => Ok(ctx),
        1 => fetch_nargs(ctx, 0, -1..=255),
        2 => fetch_nargs_pargs(ctx, -1..=255, 0..=255),
        _ => return err!(ExceptionCode::FatalError)
    })
    .and_then(|ctx| {
        if need_to_convert {
            ctx.engine.cmd.var(v).as_slice()?;
        } else {
            ctx.engine.cmd.var(v).as_continuation()?;
        }
        if ctx.engine.cc.stack.depth() < ctx.engine.cmd.pargs() {
            err!(ExceptionCode::StackUnderflow)
        } else if need_to_convert {
            convert(ctx, var!(v as u16), CONTINUATION, SLICE)
        } else {
            Ok(ctx)
        }
    })
    .and_then(|ctx| {
        let pargs = ctx.engine.cmd.pargs();
        if pargs == 0 {
            Ok(ctx)
        } else {
            pop_range(ctx, 0..pargs, pargs, var!(v as u16))
        }
    })
    .and_then(|ctx| { // update nargs
        let nargs = ctx.engine.cmd.nargs();
        if nargs >= 0 {
            let old_nargs = ctx.engine.cmd.var(v).as_continuation()?.nargs;
            ctx.engine.cmd.undo.push(Undo::WithAddressAndNargs(undo_set_nargs, var!(v), old_nargs));
            ctx.engine.cmd.var_mut(v).as_continuation_mut()
            .map(|cdata| cdata.nargs = nargs)?;
        }
        Ok(ctx)
    })
    .and_then(|ctx| { // return continuation to stack
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
        Ok(ctx)
    })
}

// switch to continuation from var!(0)
fn jmpx(ctx: Ctx) -> Result<Ctx> {
    pop_all(ctx, var!(0))
    .and_then(|ctx| swap(ctx, var!(0), CC))
    .and_then(|ctx| apply_savelist(ctx, 0..2))
}

// (continuation - ), 
// pargs = cmd.pargs if any else cc.stack.depth
// if pargs > cc.stack.depth {
//     StackOverflow
// } 
// move_out = cc.stack[0..pargs]
// move_in  = if continuation.nargs < 0 { 
//     move_out
// } else if continuation.nargs > pargs { 
//     StackOverflow
// } else {
//     move_out[0..pargs + continuation.nargs - continuation.stack.depth]
// }
// cc.stack -= move_out, continuation.stack += move_in, 
// cc = continuation, c[*] = cc.savelist[*]
pub(super) fn switch(ctx: Ctx, continuation: u16) -> Result<Ctx> {
    pop_all(ctx, continuation)
    .and_then(|ctx| swap(ctx, continuation, CC))
    .and_then(|ctx| {
        let drop_c0 = (continuation == ctrl!(0)) && ctx.engine.cc.savelist.get(0).is_none();
        let drop_c1 = (continuation == ctrl!(1)) && ctx.engine.cc.savelist.get(1).is_none();
        apply_savelist(ctx, 0..0)
        .and_then(|ctx| {
            if drop_c0 {
                ctx.engine.ctrls.remove(0);
            }
            if drop_c1 {
                ctx.engine.ctrls.remove(1);
            }
            Ok(ctx)
        })
    })
}

pub(super) fn undo_set_nargs(ctx: &mut Ctx, address: u16, nargs: isize) {
    let cdata = match address_tag!(address) {
        VAR => ctx.engine.cmd.var_mut(storage_index!(address)).as_continuation_mut(),
        CTRL => match ctx.engine.ctrls.get_mut(storage_index!(address)) {
            Some(ctrl) => ctrl.as_continuation_mut(),
            None => return
        },
        _ => return
    };
    if let Ok(cdata) = cdata {
        cdata.nargs = nargs;
    }
}

// Continuation related instructions ******************************************
// (c - ), execute C infinitely
pub(super) fn execute_again(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("AGAIN")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let body = ctx.engine.cmd.var(0).as_continuation()?.code().clone();
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::AgainLoopBody(body))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, savelist!(CC, 0), ctrl!(0)) ) // cc.savelist[0] = c[0]
    .and_then(|ctx| copy_to_var(ctx, CC) )
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2)) ) // again.savelist[0] = cc
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)) ) // continuation.savelist[0] = again
    .and_then(|ctx| switch(ctx, var!(0)))
    .err()
}

// ( - ), execute CC infinitely
pub(super) fn execute_againend(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("AGAINEND")
    )
    .and_then(|ctx| {
        let body = ctx.engine.cc.code_mut().withdraw();
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_code(body.clone())
        )));
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::AgainLoopBody(body))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, savelist!(CC, 0), ctrl!(0)) ) // cc.savelist[0] = c[0]
    .and_then(|ctx| copy_to_var(ctx, CC) )
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2)) ) // again.savelist[0] = cc
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)) ) // continuation.savelist[0] = again
    .and_then(|ctx| switch(ctx, var!(0)))
    .err()
}

// (continuation - ), continuation.savelist[0] = c[0], c[0] = continuation
pub(super) fn execute_atexit(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("ATEXIT")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| swap(ctx, var!(0), ctrl!(0)))
    .and_then(|ctx| swap(ctx, var!(0), savelist!(ctrl!(0), 0)))
    .err()
}

// (continuation - ), continuation.savelist[1] = c[1], c[1] = continuation
pub(super) fn execute_atexitalt(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("ATEXITALT")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| swap(ctx, var!(0), ctrl!(1)))
    .and_then(|ctx| swap(ctx, var!(0), savelist!(ctrl!(1), 1)))
    .err()
}

// (slice - continuation)
pub(super) fn execute_bless(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("BLESS")
    )
    .and_then(|ctx| setcont(ctx, 0, true))
    .err()
}

// (x1 ... xR slice - continuation), continuation.stack.push(x1 ... xR)
pub(super) fn execute_blessargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("BLESSARGS")
            .set_opts(InstructionOptions::ArgumentConstraints)
    )
    .and_then(|ctx| setcont(ctx, 0, true))
    .err()
}

// (x1 ... xR slice R N - continuation), continuation.stack.push(x1 ... xR)
pub(super) fn execute_blessva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("BLESSVARARGS")
    )
    .and_then(|ctx| setcont(ctx, 2, true))
    .err()
}

//(c - )
// c'= continuation {PUSHINT -1}, c'[0] = cc
// c''= continuation {PUSHINT 0}, c''[0] = cc
//c[0] = c', c[1] = c''
//execute c
pub(super) fn execute_booleval(engine: &mut Engine) -> Option<Exception> {
    let mut old_cc_idx = ctrl!(0);
    engine.load_instruction(
        Instruction::new("BOOLEVAL")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_continuation()?;
        ctx.engine.cmd.push_var(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::PushInt(-1))
        )));
        ctx.engine.cmd.push_var(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::PushInt(0))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| callx(ctx, 0))
    .and_then(|ctx| {
        let has_save_c0 = !ctx.engine.cc.can_put_to_savelist_once(0);
        if has_save_c0 {
            old_cc_idx = var!(0)
        };
        copy_to_var(ctx, old_cc_idx)
    })
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), old_cc_idx))
    .and_then(|ctx| swap(ctx, savelist!(var!(2), 0), var!(3)))
    .and_then(|ctx| swap(ctx, ctrl!(0), var!(1)))
    .and_then(|ctx| swap(ctx, ctrl!(1), var!(2)))
    .err()
}

// n ( - n), calls the continuation in c3
// approximately equivalent to PUSHINT n; PUSH c3; EXECUTE
fn execute_call(engine: &mut Engine, name: &'static str, range: Range<isize>, how: u8) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::Integer(range)),
    ).and_then(|ctx| {
        let n = ctx.engine.cmd.integer();
        if how == PREPARE {
            copy_to_var(ctx, ctrl!(3))
            .and_then(|ctx| {
                ctx.engine.cc.stack.push(int!(n));
                ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
                Ok(ctx)
            })
        } else {
            let depth = ctx.engine.cc.stack.depth();
            ctx.engine.cc.stack.push(int!(n));
            ctx.engine.cmd.undo.push(Undo::WithSize(undo_execute_call, depth));
            copy_to_var(ctx, ctrl!(3))
            .and_then(|ctx| {
                match how {
                    SWITCH => switch(ctx, var!(0)),
                    CALLX => callx(ctx, 0),
                    _ => unimplemented!("how: 0x{:X}", how)
                }
            })
        }
    })
    .err()
}
fn undo_execute_call(ctx: &mut Ctx, index: usize) {
    ctx.engine.cc.stack.drop(index).unwrap();
}

// 0 =< n =< 255
pub(super) fn execute_call_short(engine: &mut Engine) -> Option<Exception> {
    execute_call(engine, "CALL", 0..256, CALLX)
}
// 0 =< n < (2 ^ 14)
pub(super) fn execute_call_long(engine: &mut Engine) -> Option<Exception> {
    execute_call(engine, "CALL", 0..16384, CALLX)
}
// 0 =< n < (2 ^ 14)
pub(super) fn execute_jmp(engine: &mut Engine) -> Option<Exception> {
    execute_call(engine, "JMP", 0..16384, SWITCH)
}
// 0 =< n < (2 ^ 14)
pub(super) fn execute_prepare(engine: &mut Engine) -> Option<Exception> {
    execute_call(engine, "PREPARE", 0..16384, PREPARE)
}

// (continuation - ), callcc pattern
pub(super) fn execute_callcc(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("CALLCC"))
    .and_then(|ctx| callcc(ctx, 0))
    .err()
}

// (continuation - ), callcc pattern
pub(super) fn execute_callccargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CALLCCARGS") 
            .set_opts(InstructionOptions::ArgumentConstraints)
    )
    .and_then(|ctx| callcc(ctx, 0))
    .err()
}

// (continuation pargs rargs - ), callcc pattern
pub(super) fn execute_callccva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CALLCCVARARGS")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| fetch_nargs_pargs(ctx, -1..=255, -1..=255))
    .and_then(|ctx| callcc(ctx, 2))
    .err()
}

// equivalent to PUSHREFCONT; CALLX
// e.g. remove first reference from CC and then call it
pub(super) fn execute_callref(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CALLREF")
    )
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| convert(ctx, var!(0), CONTINUATION, CELL))
    .and_then(|ctx| callx(ctx, 0))
    .err()
}

// (continuation - ), callx pattern
pub(super) fn execute_callx(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CALLX")
    )
    .and_then(|ctx| callx(ctx, 0))
    .err()
}

// (continuation - ), callx pattern
pub(super) fn execute_callxargs(engine: &mut Engine) -> Option<Exception> {
    let cmd = engine.cc.last_cmd();
    engine.load_instruction(
        Instruction::new("CALLXARGS").set_opts(
            if cmd == 0xDA {
                InstructionOptions::ArgumentAndReturnConstraints
            } else {
                InstructionOptions::Pargs(0..16)
            }
        )
    )
    .and_then(|ctx| callx(ctx, 0))
    .err()
}

// (continuation pargs rargs - ), callx pattern
pub(super) fn execute_callxva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CALLXVARARGS")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| fetch_nargs_pargs(ctx, -1..=254, -1..=254))
    .and_then(|ctx| callx(ctx, 2))
    .err()
}

// (continuation1 continuation2 - continuation1), continuation1.savelist[0] = continuation2
pub(super) fn execute_compos(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("COMPOS")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_continuation()?;
        ctx.engine.cmd.var(1).as_continuation()?;
        swap(ctx, var!(0), savelist!(var!(1), 0))
    })
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
        Ok(ctx)
    })
    .err()
}

// (continuation1 continuation2 - continuation1), continuation1.savelist[1] = continuation2
pub(super) fn execute_composalt(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("COMPOSALT")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_continuation()?;
        ctx.engine.cmd.var(1).as_continuation()?;
        swap(ctx, var!(0), savelist!(var!(1), 1))
    })
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
        Ok(ctx)
    })
    .err()
}

// (continuation1 continuation2 - continuation1),
// continuation1.savelist[0] = continuation2, continuation1.savelist[1] = continuation2
pub(super) fn execute_composboth(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("COMPOSBOTH")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_continuation()?;
        ctx.engine.cmd.var(1).as_continuation()?;
        Ok(ctx)
    })
    .and_then(|ctx| copy_to_var(ctx, var!(0)))
    .and_then(|ctx| swap(ctx, var!(0), savelist!(var!(1), 0)))
    .and_then(|ctx| swap(ctx, var!(2), savelist!(var!(1), 1)))
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(1));
        Ok(ctx)
    })
    .err()
}

// (f x y - ), x f != 0 else y
pub(super) fn execute_condsel(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CONDSEL")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        if !ctx.engine.cmd.var(2).as_bool()? {
            ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        } else {
            ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(1));
        }
        Ok(ctx)
    })
    .err()
}

// (f x y - ), x f != 0 else y, throws exception, if types mismatch
pub(super) fn execute_condselchk(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("CONDSELCHK")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        if mem::discriminant(ctx.engine.cmd.var(0)) != mem::discriminant(ctx.engine.cmd.var(1)) {
            return err!(ExceptionCode::TypeCheckError)
        }
        if !ctx.engine.cmd.var(2).as_bool()? {
            ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        } else {
            ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(1));
        }
        Ok(ctx)
    })
    .err()
}

const JMP:   u8 = 0x01; // jump to cont
const CALL:  u8 = 0x02; // call cont
const RET:   u8 = 0x04; // ret to c0
const ALT:   u8 = 0x08; // ret to c1
const REF:   u8 = 0x10; // use refslice as cont
const INV:   u8 = 0x20; // condition not
const BOTH:  u8 = 0x40; // IFELSE
const THROW: u8 = 0x80; // checks if condition is NaN then throw IntegerOverflow

fn execute_if_mask(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    let mut params = 2;
    if how.bit(REF) {
        params -= 1;
    }
    if how.bit(RET) {
        params -= 1;
    }
    if how.bit(BOTH) {
        params += 1;
    }

    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| match how.bit(REF) {
        true => fetch_reference(ctx, CC).and_then(|ctx| convert(ctx, var!(0), CONTINUATION, CELL)),
        false => Ok(ctx)
    })
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| if how.bit(THROW) && ctx.engine.cmd.vars.last().unwrap().as_integer()?.is_nan() {
        err!(ExceptionCode::IntegerOverflow)
    } else {
        Ok(ctx)
    })
    .and_then(|ctx| match ctx.engine.cmd.vars.last().unwrap().as_bool()? ^ how.bit(INV) {
        false if how.bit(BOTH) => callx(ctx, 1),
        false => Ok(ctx),
        true if how.bit(CALL) => callx(ctx, 0),
        true if how.bit(JMP ) => jmpx(ctx),
        true if how.bit(ALT ) => retalt(ctx),
        true if how.bit(RET ) => ret(ctx),
        _ => unimplemented!("how = 0x{:X}", how)
    })
    .err()
}

// (condition continuation - ): callx if condition != 0
pub(super) fn execute_if(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IF", CALL)
}

// (condition continuation1 continuation2 - ): if condition != 0 callx continuation1 else callx continuation2
pub(super) fn execute_ifelse(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFELSE", CALL | BOTH | INV)
}

// (condition continuation - ): switch if condition != 0
pub(super) fn execute_ifjmp(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFJMP", JMP)
}

// (condition continuation - ): callx if condition == 0
pub(super) fn execute_ifnot(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFNOT", CALL | INV)
}
      
// (condition continuation - ): switch if condition == 0
pub(super) fn execute_ifnotjmp(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFNOTJMP", JMP | INV)
}

// (condition - Continuation): pushrefcont if condition == 0
pub(super) fn execute_ifnotref(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFNOTREF", CALL | INV | REF)
}

// (condition - ): switch to continuation from references[0] if condition != 0
pub(super) fn execute_ifjmpref(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFJMPREF", JMP | REF)
}

// (condition - ): switch to continuation from references[0] if condition == 0
pub(super) fn execute_ifnotjmpref(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFNOTJMPREF", JMP | INV | REF)
}

// (condition - ): switch if condition == 0
pub(super) fn execute_ifnotret(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFNOTRET", RET | INV)
}

// (condition - Continuation): pushrefcont if condition != 0
pub(super) fn execute_ifref(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFREF", CALL | REF)
}

// (condition - ): switch if condition != 0
pub(super) fn execute_ifret(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFRET", RET | THROW)
}

// (f - ), RETALT f != 0
pub(super) fn execute_ifretalt(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFRETALT", RET | ALT)
}

// (f - ), RETALT f == 0
pub(super) fn execute_ifnotretalt(engine: &mut Engine) -> Option<Exception> {
    execute_if_mask(engine, "IFNOTRETALT", RET | ALT | INV)
}

// c[0] <-> c[1]
pub(super) fn execute_invert(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("INVERT")
    )
    .and_then(|ctx| swap(ctx, ctrl!(0), ctrl!(1)))
    .err()
}

pub(super) fn execute_jmpref(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("JMPREF")
    )
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| convert(ctx, var!(0), CONTINUATION, CELL))
    .and_then(|ctx| jmpx(ctx))
    .err()
}

pub(super) fn execute_jmprefdata(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("JMPREFDATA")
    )
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| convert(ctx, var!(0), CONTINUATION, CELL))
    .and_then(|ctx| jmpxdata(ctx))
    .err()
}

// (continuation - ), switch pattern
pub(super) fn execute_jmpx(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("JMPX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| jmpx(ctx))
    .err()
}

fn execute_ifbit_mask(engine: &mut Engine, how: u8) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(
            if how.bit(INV) {
                "IFNBITJMPREF"
            } else {
                "IFBITJMPREF"
            })
            .set_opts(InstructionOptions::Integer(0..32))
    )
    .and_then(|ctx| if how.bit(REF) {
        fetch_reference(ctx, CC)
    } else {
        fetch_stack(ctx, 1)
        .and_then(|ctx| {
            ctx.engine.cmd.var(0).as_continuation()?;
            Ok(ctx)
        })
    })
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 1 {
            return err!(ExceptionCode::StackUnderflow);
        }
        let is_zero = {
            let x = ctx.engine.cc.stack.get(0).as_integer()?;
            let nbit = ctx.engine.cmd.integer() as u32;
            let test_bit_mask = IntegerData::from_u32(1 << nbit);
            x.and::<Signaling>(&test_bit_mask)?.is_zero()
        };
        if is_zero ^ how.bit(INV) {
            Ok(ctx)
        } else if how.bit(REF) {
            convert(ctx, var!(0), CONTINUATION, CELL)
            .and_then(|ctx| jmpx(ctx))
        } else {
            jmpx(ctx)
        }
    })
    .err()
}

// (x continuation - x), switch if n's bit of x is set
pub(super) fn execute_ifbitjmp(engine: &mut Engine) -> Option<Exception> {
    execute_ifbit_mask(engine, 0)
}

// (x continuation - x), switch if n's bit of x is not set
pub(super) fn execute_ifnbitjmp(engine: &mut Engine) -> Option<Exception> {
    execute_ifbit_mask(engine, INV)
}

// (x - x), switch pattern if n'th bit is set
pub(super) fn execute_ifbitjmpref(engine: &mut Engine) -> Option<Exception> {
    execute_ifbit_mask(engine, REF)
}

// (x - x), switch pattern if n'th bit is not set
pub(super) fn execute_ifnbitjmpref(engine: &mut Engine) -> Option<Exception> {
    execute_ifbit_mask(engine, REF | INV)
}

// (continuation - ), continuation.nargs = cmd.pargs, then switch pattern
pub(super) fn execute_jmpxargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("JMPXARGS").set_opts(InstructionOptions::Pargs(0..16))
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| switch(ctx, var!(0)))
    .err()
}

// (continuation p - ), continuation.nargs = cmd.pargs, then switch pattern
pub(super) fn execute_jmpxva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("JMPXVARARGS")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| fetch_pargs(ctx, 0, -1..=254).and_then(|ctx| switch(ctx, var!(1))))
    .err()
}

// (integer_repeat_count body_continuation - )
// body.savelist[0] = cc
// cc.savelist[0] = c[0]
pub(super) fn execute_repeat(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("REPEAT")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let body = ctx.engine.cmd.var(0).as_continuation()?.code().clone();
        let counter = ctx.engine.cmd.var(1).as_integer()?.into(-0x80000000..=0x7FFFFFFF)?;
        if counter <= 0 {
            Ok(ctx)
        } else {
            ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
                ContinuationData::with_type(ContinuationType::RepeatLoopBody(body, counter))
            )));
            swap(ctx, savelist!(CC, 0), ctrl!(0)) // cc.savelist[0] = c[0]
            .and_then(|ctx| copy_to_var(ctx, CC))
            .and_then(|ctx| swap(ctx, savelist!(var!(2), 0), var!(3))) // ec_repeat.savelist[0] = cc
            .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(2))) // body.savelist[0] = ec_repeat
            .and_then(|ctx| switch(ctx, var!(0)))
        }
    })
    .err()
}

// (integer_repeat_count - )
// body.savelist[0] = cc
// cc.savelist[0] = c[0]
pub(super) fn execute_repeatend(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("REPEATEND")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let body = ctx.engine.cc.code_mut().withdraw();
        let counter = ctx.engine.cmd.var(0).as_integer()?.into(-0x80000000..=0x7FFFFFFF)?;
        if counter <= 0 {
            return Ok(ctx);
        } else {
            ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
                ContinuationData::with_code(body.clone())
            )));
            ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
                ContinuationData::with_type(ContinuationType::RepeatLoopBody(body, counter))
            )));
            swap(ctx, savelist!(CC, 0), ctrl!(0)) // cc.savelist[0] = c[0]
            .and_then(|ctx| copy_to_var(ctx, CC))
            .and_then(|ctx| swap(ctx, savelist!(var!(2), 0), var!(3))) // ec_repeat.savelist[0] = cc
            .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2))) // body.savelist[0] = ec_repeat
            .and_then(|ctx| switch(ctx, var!(1)))
        }
    })
    .err()
}

// c[0].stack = cc.stack, cc.stack = ()
// cc = continuation, c[2..] = cc.savelist[2..]
// (continuation - ), var[0] = cc.stack.pop(), then jmpxdata pattern
pub(super) fn execute_jmpxdata(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("JMPXDATA")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| jmpxdata(ctx))
    .err()
}

// switch to c[0]
pub(super) fn execute_ret(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("RET"))
    .and_then(|ctx| ret(ctx))
    .err()
}

// switch to c[1]
pub(super) fn execute_retalt(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETALT")
    )
    .and_then(|ctx| retalt(ctx))
    .err()
}

// switch to c[0] with pargs
pub(super) fn execute_retargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETARGS").set_opts(InstructionOptions::Pargs(0..16))
    )
    .and_then(|ctx| switch(ctx, ctrl!(0)))
    .err()
}

// (p - ) switch to c[0] with p params
pub(super) fn execute_retva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETVARARGS")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| fetch_pargs(ctx, 0, -1..=254).and_then(|ctx| switch(ctx, ctrl!(0))))
    .err()
}


// (condition - ), if condition != 0 then RET else RETALT
pub(super) fn execute_retbool(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETBOOL")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        match ctx.engine.cmd.var(0).as_bool()? {
            false => retalt(ctx),
            _ => ret(ctx)
        }
    })
    .err()
}

// var[0] = c[0], then jmpxdata pattern
pub(super) fn execute_retdata(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETDATA")
    )
    .and_then(|ctx| copy_to_var(ctx, ctrl!(0)))
    .and_then(|ctx| jmpxdata(ctx))
    .err()
}

// (xN ... xN-p xN-p-1 ... x0 - xN-p-1 ... x0), c0.stack.push(xN ... xN-p)
pub(super) fn execute_returnargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETURNARGS")
           .set_opts(InstructionOptions::Rargs(0..16))
    )
    .and_then(|ctx| if ctx.engine.cc.stack.depth() < ctx.engine.cmd.rargs() {
        err!(ExceptionCode::StackUnderflow)
    } else {
        let drop = ctx.engine.cmd.rargs()..ctx.engine.cc.stack.depth();
        let save = drop.end - drop.start;
        pop_range(ctx, drop, save, ctrl!(0))
    })
    .err()
}

// (xN ... xN-p xN-p-1 ... x0 p - xN-p-1 ... x0), c0.stack.push(xN ... xN-p)
pub(super) fn execute_returnva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("RETURNVARARGS")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let rargs = ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?;
        if ctx.engine.cc.stack.depth() < rargs {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let drop = rargs..ctx.engine.cc.stack.depth();
            let save = drop.end - drop.start;
            pop_range(ctx, drop, save, ctrl!(0))
        }
    })
    .err()
}

// ( - ), if c[0].savelist[i].is_none() { c[0].savelist[i] = c[i] }
pub(super) fn execute_save(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SAVE").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| save(ctx, 0))
    .err()
}

// ( - ), if c[1].savelist[i].is_none() { c[1].savelist[i] = c[i] }
pub(super) fn execute_savealt(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SAVEALT").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| save(ctx, 1))
    .err()
}

// ( - ), if c[0].savelist[i].is_none() { c[0].savelist[i] = c[i] }
// if c[1].savelist[i].is_none() { c[1].savelist[i] = c[i] }
pub(super) fn execute_saveboth(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SAVEBOTH").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| if ctx.engine.ctrl(0).is_ok() || ctx.engine.ctrl(1).is_ok() {
        err!(ExceptionCode::TypeCheckError)
    } else {
        save(ctx, 0)
    })
    .and_then(|ctx| save(ctx, 1))
    .err()
}

// (x - ), c1.savelist[i] = x
pub(super) fn execute_setaltctr(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETALTCTR").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let creg = ctx.engine.cmd.creg();
        swap(ctx, var!(0), savelist!(ctrl!(1), creg))
    })
    .err()
}

// (x1 ... xR continuation - continuation), continuation.stack.push(x1 ... xR)
pub(super) fn execute_setcontargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETCONTARGS").set_opts(InstructionOptions::ArgumentConstraints)
    )
    .and_then(|ctx| setcont(ctx, 0, false))
    .err()
}

// (x1 ... xR continuation R N - continuation), continuation.stack.push(x1 ... xR)
pub(super) fn execute_setcontva(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETCONTVARARGS")
    )
    .and_then(|ctx| setcont(ctx, 2, false))
    .err()
}

// (continuation n - continuation)
pub(super) fn execute_setnumvarargs(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETNUMVARARGS")
    )
    .and_then(|ctx| setcont(ctx, 1, false))
    .err()
}

// (x continuation - continuation), continuation.savelist[i] = x
pub(super) fn execute_setcontctr(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETCONTCTR").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        ctx.engine.cmd.var(0).as_continuation()?;
        let creg = ctx.engine.cmd.creg();
        swap(ctx, var!(1), savelist!(var!(0), creg))
    })
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// (x continuation i - continuation), continuation.savelist[i] = x
pub(super) fn execute_setcontctrx(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETCONTCTRX")
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let creg = ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?;
        ctx.engine.cmd.var(1).as_continuation()?;
        swap(ctx, var!(2), savelist!(var!(1), creg))
    })
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(1));
        Ok(ctx)
    })
    .err()
}

// (continuation - ), continuation.savelist[0] = c[0], continuation.savelist[1] = c[1],
// c[1] = continuation
pub(super) fn execute_setexitalt(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETEXITALT")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| copy_to_var(ctx, ctrl!(0)))
    .and_then(|ctx| swap(ctx, var!(1), savelist!(var!(0), 0)))
    .and_then(|ctx| if ctx.engine.cc.savelist.get(1).is_some() {
        copy_to_var(ctx, ctrl!(1))
        .and_then(|ctx| swap(ctx, var!(2), savelist!(var!(0), 1)))
    } else {
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, var!(0), ctrl!(1)))
    .err()
}

// (x - ), c0.savelist[i] = x
pub(super) fn execute_setretctr(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("SETRETCTR").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let creg = ctx.engine.cmd.creg();
        swap(ctx, var!(0), savelist!(ctrl!(0), creg))
    })
    .err()
}

// (continuation - continuation), continuation.savelist[0] = c[0]
pub(super) fn execute_thenret(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("THENRET")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| copy_to_var(ctx, ctrl!(0)))
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)))
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// (continuation - continuation), continuation.savelist[0] = c[1]
pub(super) fn execute_thenretalt(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("THENRETALT")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| copy_to_var(ctx, ctrl!(1)))
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)))
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// (body - )
// cc.savelist[0] = c[0]
// condition.savelist[0] = cc
// body.savelist[0] = condition
// switch to body
pub(super) fn execute_until(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("UNTIL")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let body = ctx.engine.cmd.var(0).as_continuation()?.code().clone();
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::UntilLoopCondition(body))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, savelist!(CC, 0), ctrl!(0)) )     // cc.savelist[0] = c[0]
    .and_then(|ctx| copy_to_var(ctx, CC) )
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2)) ) // ec_until.savelist[0] = cc
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)) ) // body.savelist[0] = ec_until
    .and_then(|ctx| switch(ctx, var!(0)))
    .err()
}

// cc is body
// condition.savelist[0] = c[0]
// body.savelist[0] = condition
// switch to body
pub(super) fn execute_untilend(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("UNTILEND")
    )
    .and_then(|ctx| {
        let body = ctx.engine.cc.code_mut().withdraw();
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_code(body.clone())
        )));
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::UntilLoopCondition(body))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, savelist!(CC, 0), ctrl!(0)) )     // cc.savelist[0] = c[0]
    .and_then(|ctx| copy_to_var(ctx, CC))
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2)) ) // ec_until.savelist[0] = cc
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)) ) // body.savelist[0] = ec_until
    .and_then(|ctx| switch(ctx, var!(0)))
    .err()
}

// (condition body - )
// cc.savelist[0] = c[0]
// ec_while.savelist[0] = cc
// condition.savelist[0] = ec_while
// switch to condition
pub(super) fn execute_while(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("WHILE")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let body = ctx.engine.cmd.var(0).as_continuation()?.code().clone();
        let cond = ctx.engine.cmd.var(1).as_continuation()?.code().clone();
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::WhileLoopCondition(body, cond))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, savelist!(CC, 0), ctrl!(0)) )     // cc.savelist[0] = c[0]
    .and_then(|ctx| copy_to_var(ctx, CC))
    .and_then(|ctx| swap(ctx, savelist!(var!(2), 0), var!(3)) ) // ec_while.savelist[0] = cc
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2)) ) // condition.savelist[0] = ec_while
    .and_then(|ctx| switch(ctx, var!(1)))
    .err()
}

// cc is body
// (condition - )
// condition.savelist[0] = c[0]
// ec_while.savelist[0] = condition
// switch to condition
pub(super) fn execute_whileend(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("WHILEEND")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let body = ctx.engine.cc.code_mut().withdraw();
        let cond = ctx.engine.cmd.var(0).as_continuation()?.code().clone();
        ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
            ContinuationData::with_type(ContinuationType::WhileLoopCondition(body, cond))
        )));
        Ok(ctx)
    })
    .and_then(|ctx| swap(ctx, savelist!(CC, 0), ctrl!(0)) )     // cc.savelist[0] = c[0]
    .and_then(|ctx| copy_to_var(ctx, CC))
    .and_then(|ctx| swap(ctx, savelist!(var!(1), 0), var!(2)) ) // ec_while.savelist[0] = cc
    .and_then(|ctx| swap(ctx, savelist!(var!(0), 0), var!(1)) ) // condition.savelist[0] = ec_while
    .and_then(|ctx| switch(ctx, var!(0)))
    .err()
}
