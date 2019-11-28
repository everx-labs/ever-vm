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

use executor::microcode::{VAR, STACK, CC, CC_SAVELIST, CTRL, CTRL_SAVELIST, VAR_SAVELIST};
use executor::types::{Ctx, Undo};
use stack::{ContinuationData, SaveList, StackItem};
use executor::continuation::undo_set_nargs;
use std::mem;
use std::ops::Range;
use std::sync::Arc;
use types::{
//    Exception,
    ExceptionCode,
    Result,
    ResultMut,
    ResultRef,
    ResultVec,
    Status,
};

// Utilities ******************************************************************

fn continuation_by_address<'a>(ctx: &'a Ctx, address: u16) -> ResultRef<'a, ContinuationData> {
    match address_tag!(address) {
        VAR => ctx.engine.cmd.var(storage_index!(address)).as_continuation(),
        CTRL => match ctx.engine.ctrls.get(storage_index!(address)) {
            Some(ctrl) => ctrl.as_continuation(),
            None => return err!(ExceptionCode::TypeCheckError)
        },
        _ => unimplemented!()
    }
}

#[macro_export]
macro_rules! continuation_mut_by_address {
    ($ctx:ident, $address:expr) => {
        match address_tag!($address) {
            VAR => $ctx.engine.cmd.var_mut(storage_index!($address)).as_continuation_mut(),
            CTRL => match $ctx.engine.ctrls.get_mut(storage_index!($address)) {
                Some(ctrl) => ctrl.as_continuation_mut(),
                None => return err!(ExceptionCode::TypeCheckError)
            },
            _ => unimplemented!()
        }
    };
}

fn move_stack(
    ctx: &mut Ctx, 
    dst: u16, 
    src: u16, 
    drop: Range<usize>, 
    save: usize
) -> ResultVec<StackItem> {
    if save > (drop.end - drop.start) {
        unimplemented!()
    }
    let from_cc = address_tag!(src) == CC;
    let address = if from_cc { dst } else { src };
    let peer = continuation_mut_by_address!(ctx, address)?;
    let mut popped = if from_cc {
        if peer.nargs >= 0 {
            if save > peer.nargs as usize {
                return err!(ExceptionCode::StackOverflow)
            } else {
                peer.nargs -= save as isize
            }
        }
        ctx.engine.cc.stack.drop_range(drop)?
    } else {
        peer.stack.drop_range(drop)?
    };
    let mut ret = Vec::new();
    while popped.len() > save {
        ret.push(popped.pop().unwrap())
    }
    while let Some(x) = popped.pop() {
        if from_cc {
            peer.stack.push(x);
        } else {
            ctx.engine.cc.stack.push(x);
        }
    }
    Ok(ret)
}

// Swapping *******************************************************************

struct Info {
    flags: u16,
    index: usize
}

impl Info {
    fn item<'a>(&self, ctx: &'a mut Ctx) -> ResultMut<'a, StackItem> {
        match address_tag!(self.flags) {
            VAR => Ok(ctx.engine.cmd.var_mut(self.index)),
            _ => unimplemented!("Info.item {:x}\n", self.flags)
        }
    }
    #[cfg_attr(rustfmt, rustfmt_skip)]
    fn list<'a>(&mut self, ctx: &'a mut Ctx) -> ResultMut<'a, SaveList> {
        match address_tag!(self.flags) {
            CC_SAVELIST => {
                self.index = savelist_index!(self.flags);
                Ok(&mut ctx.engine.cc.savelist)
            },
            CTRL => Ok(&mut ctx.engine.ctrls),
            CTRL_SAVELIST => {
                let continuation = ctx.engine.ctrls.get_mut(storage_index!(self.flags)).unwrap()
                    .as_continuation_mut()?;
                self.index = savelist_index!(self.flags);
                Ok(&mut continuation.savelist)
            },
            VAR_SAVELIST => {
                let continuation = ctx.engine.cmd.var_mut(storage_index!(self.flags))
                    .as_continuation_mut()?;
                self.index = savelist_index!(self.flags);
                Ok(&mut continuation.savelist)
            }
            _ => unimplemented!("Info.list {:x}\n", self.flags)
        }
    }
}

fn put_to_list(ctx: &mut Ctx, x: &mut Info, y: &mut StackItem) -> Result<Option<StackItem>> {
    x.list(ctx)?.put(x.index, y)
}

fn put_to_list_from_item(ctx: &mut Ctx, x: &mut Info, y: &Info) -> Result<Option<StackItem>> {
    if !SaveList::can_put(x.index, y.item(ctx)?) {
        let value = x.list(ctx)?.get(x.index).map(|value| value.clone()).unwrap_or_default();
        error!(target: "tvm", "Cannot set: {} to list with index: {} and value: {}",
            y.item(ctx)?.clone(), x.index, value);
        err!(ExceptionCode::TypeCheckError)
    } else {
        let mut y = y.item(ctx)?.withdraw();
        x.list(ctx)?.put(x.index, &mut y)
    }
}

fn put_to_list_from_list(ctx: &mut Ctx, x: &mut Info, y: &mut Info) -> Result<Option<StackItem>> {
    if !SaveList::can_put(x.index, y.list(ctx)?.get(y.index).unwrap()) {
        let value = x.list(ctx)?.get(x.index).map(|value| value.clone()).unwrap_or_default();
        error!(target: "tvm", "Cannot set: {} to list with index: {} and value: {}",
            y.list(ctx)?.get(y.index).unwrap().clone(), x.index, value);
        err!(ExceptionCode::TypeCheckError)
    } else {
        let mut y = y.list(ctx)?.remove(y.index).unwrap();
        x.list(ctx)?.put(x.index, &mut y)
    }
}

fn swap_with_list(ctx: &mut Ctx, mut x: Info, y: Info) -> Status {
    if !x.list(ctx)?.get(x.index).is_none() || !y.item(ctx)?.is_null() {
        *y.item(ctx)? = match put_to_list_from_item(ctx, &mut x, &y)? {
            Some(x) => x,
            None => StackItem::None
        };
    }
    Ok(())
}

fn swap_between_lists(ctx: &mut Ctx, mut x: Info, mut y: Info) -> Status {
    if x.list(ctx)?.get(x.index).is_none() {
        if y.list(ctx)?.get(y.index).is_none() {
            Ok(())
        } else {
            put_to_list_from_list(ctx, &mut x, &mut y)?;
            Ok(())
        }
    } else if y.list(ctx)?.get(y.index).is_none() {
        put_to_list_from_list(ctx, &mut y, &mut x)?;
        Ok(())
    } else if !SaveList::can_put(y.index, x.list(ctx)?.get(x.index).unwrap()) {
        err!(ExceptionCode::TypeCheckError)
    } else {
        let mut x = match put_to_list_from_list(ctx, &mut x, &mut y)? {
            Some(x) => x,
            None => StackItem::None
        };
        put_to_list(ctx, &mut y, &mut x).unwrap();
        Ok(())
    }
}

fn swap_any(ctx: &mut Ctx, mut x: u16, mut y: u16) -> Status {
    if address_tag!(x) > address_tag!(y) {
        mem::swap(&mut x, &mut y);
    }
    let x = Info {
        flags: x,
        index: storage_index!(x),
    };
    let y = Info {
        flags: y,
        index: storage_index!(y),
    };
    match address_tag!(x.flags) {
        CC_SAVELIST | CTRL | CTRL_SAVELIST | VAR_SAVELIST => match address_tag!(y.flags) {
            CC_SAVELIST | CTRL | CTRL_SAVELIST | VAR_SAVELIST => swap_between_lists(ctx, x, y),
            VAR => swap_with_list(ctx, x, y),
            _ => unimplemented!()
        },
        CC => match address_tag!(y.flags) {
            CTRL => match ctx.engine.ctrls.get_mut(y.index) {
                Some(c) => {
                    mem::swap(c.as_continuation_mut()?, &mut ctx.engine.cc);
                    Ok(())
                },
                None => err!(ExceptionCode::TypeCheckError)
            },
            VAR => {
                mem::swap(
                    ctx.engine.cmd.var_mut(y.index).as_continuation_mut()?,
                    &mut ctx.engine.cc
                );
                Ok(())
            },
            _ => unimplemented!("swap CC-{:x}", y.flags)
        },
        VAR => match address_tag!(y.flags) {
            CC_SAVELIST | CTRL_SAVELIST | VAR_SAVELIST => swap_with_list(ctx, y, x),
            VAR => {
                ctx.engine.cmd.vars.swap(x.index, y.index);
                Ok(())
            },
            _ => unimplemented!()
        }
        _ => {
            unimplemented!("swap_any {:x}-{:x}\nswap_any {:x}-{:x}\n", x.flags, y.flags, address_tag!(x.flags), CTRL)
        }
    }
}

// Microfunctions *************************************************************

// c[*] = CC.savelist[*], excluding given indexes
pub(in executor) fn apply_savelist(ctx: Ctx, exclude: Range<usize>) -> Result<Ctx> {
    let mut prev = SaveList::new();
    let mut undo = false;
    for (k, v) in ctx.engine.cc.savelist.iter_mut() {
        if (*k >= exclude.start) && (*k < exclude.end) {
            continue
        }
        match ctx.engine.ctrls.put(*k, v) {
            Err(e) => {
                if undo {
                    ctx.engine.cmd.undo.push(Undo::WithSaveList(undo_apply_savelist, prev));
                }
                return Err(e)
            },
            Ok(Some(mut x)) => {
                undo = true;
                prev.put(*k, &mut x).unwrap();
            },
            _ => ()
        }
    }
    ctx.engine.cc.savelist.clear();
    ctx.engine.cmd.undo.push(Undo::WithSaveList(undo_apply_savelist, prev));
    Ok(ctx)
}

fn undo_apply_savelist(ctx: &mut Ctx, mut savelist: SaveList) {
    ctx.engine.apply_savelist(&mut savelist).unwrap()
}

// ctx.cmd.push_var(copy-of-src)
// src addressing is described in executor/microcode.rs
pub(in executor) fn copy_to_var(ctx: Ctx, src: u16) -> Result<Ctx> {
    let copy = match address_tag!(src) {
        CC => {
            let copy = ctx.engine.cc.copy_without_stack();
            StackItem::Continuation(Arc::new(copy))
        }
        CTRL => match ctx.engine.ctrls.get(storage_index!(src)) {
            Some(ctrl) => ctrl.clone(),
            None => return err!(ExceptionCode::TypeCheckError)
        },
        STACK => ctx.engine.cc.stack.get(stack_index!(src)).clone(),
        VAR => ctx.engine.cmd.var(storage_index!(src)).clone(),
        _ => unimplemented!()
    };
    ctx.engine.cmd.push_var(copy);
    ctx.engine.cmd.undo.push(Undo::WithCode(undo_copy_to_var, src));
    Ok(ctx)
}

fn undo_copy_to_var(ctx: &mut Ctx, _src: u16) {
    ctx.engine.cmd.vars.pop().unwrap();
}

// ctx.cmd.push_var(src.references[0])
pub(in executor) fn fetch_reference(ctx: Ctx, src: u16) -> Result<Ctx> {
    let cell = match address_tag!(src) {
        CC => ctx.engine.cc.drain_reference()?.clone(),
        _ => unimplemented!()
    };
    ctx.engine.cmd.push_var(StackItem::Cell(cell));
    ctx.engine.cmd.undo.push(Undo::WithCode(undo_fetch_reference, src));
    Ok(ctx)
}

fn undo_fetch_reference(ctx: &mut Ctx, src: u16) {
    match ctx.engine.cmd.vars.pop().unwrap() {
        StackItem::Slice(_) => match address_tag!(src) {
            CC => ctx.engine.cc.undrain_reference(),
            _ => unimplemented!()
        },
        _ => (),
    }
}

// ctx.cmd.push_var(CC.stack[0..depth])
pub(in executor) fn fetch_stack(ctx: Ctx, depth: usize) -> Result<Ctx> {
    if ctx.engine.cc.stack.depth() < depth {
        err!(ExceptionCode::StackUnderflow)
    } else {
        ctx.engine.cmd.vars.append(&mut ctx.engine.cc.stack.drop_range(0..depth)?);
        ctx.engine.cmd.undo.push(Undo::WithSize(undo_fetch_stack, depth));
        Ok(ctx)
    }
}

fn undo_fetch_stack(ctx: &mut Ctx, depth: usize) {
    for _ in 0..depth {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
    }
}

// dst.stack.push(CC.stack)
// dst addressing is described in executor/microcode.rs
pub(in executor) fn pop_all(ctx: Ctx, dst: u16) -> Result<Ctx> {
    let nargs = continuation_by_address(&ctx, dst)?.nargs;
    let depth = ctx.engine.cc.stack.depth();
    let pargs = ctx.engine.cmd.ictx.pargs();
    let drop = if nargs < 0 {
        pargs.unwrap_or(depth)
    } else if let Some(pargs) = pargs {
        if pargs < nargs as usize {
            return err!(ExceptionCode::StackUnderflow)
        }
        pargs
    } else {
        nargs as usize
    };
    pop_range(ctx, 0..drop, drop, dst)
}

// dst.stack.push(CC.stack[range])
// dst addressing is described in executor/microcode.rs
pub(in executor) fn pop_range(
    mut ctx: Ctx, 
    drop: Range<usize>, 
    save: usize, 
    dst: u16
) -> Result<Ctx> {
    let undo_save = save;
    let undo_drop = move_stack(&mut ctx, dst, CC, drop, save)?;
    let nargs = continuation_by_address(&ctx, dst)?.nargs;
    ctx.engine.cmd.undo.push(Undo::WithAddressAndNargs(undo_set_nargs, dst, nargs));
    ctx.engine.cmd.undo.push(
        Undo::WithSizeDataAndCode(undo_pop_range, undo_save, undo_drop, dst)
    );
    Ok(ctx)
}

fn undo_pop_range(ctx: &mut Ctx, save: usize, mut drop: Vec<StackItem>, src: u16) {
    loop {
        match drop.pop() {
            Some(x) => ctx.engine.cc.stack.push(x),
            None => break
        };
    }
    move_stack(ctx, CC, src, 0..save, save).unwrap();
}

// x <-> y
// x and y addressing is described in executor/microcode.rs
pub(in executor) fn swap(mut ctx: Ctx, x: u16, y: u16) -> Result<Ctx> {  
    swap_any(&mut ctx, x, y)?;                                                                             
    ctx.engine.cmd.undo.push(Undo::WithCodePair(undo_swap, x, y));
    Ok(ctx)
}

fn undo_swap(ctx: &mut Ctx, x: u16, y: u16) {
    swap_any(ctx, x, y).unwrap()
}
