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
    executor::{
        Mask, engine::{Engine, storage::fetch_stack}, gas::gas_state::Gas, 
        types::{InstructionOptions, Instruction, WhereToGetParams}
    },
    stack::{StackItem, integer::IntegerData},
    types::{Exception, Failure}
};
use std::sync::Arc;
use ton_types::{error, types::ExceptionCode};

fn tuple(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    let mut inst = Instruction::new(name);
    let mut params = 0;
    if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Length(0..16));
    }
    engine.load_instruction(inst)
    .and_then(|ctx| if how.bit(CMD) {
        params = ctx.engine.cmd.length();
        fetch_stack(ctx, params)
    } else {
        fetch_stack(ctx, 1)
        .and_then(|ctx| {
            params = ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?;
            fetch_stack(ctx, params)
        })
    })
    .and_then(|ctx| {
        let vars = ctx.engine.cmd.var_count();
        let mut tuple = ctx.engine.cmd.vars.split_off(vars - params);
        tuple.reverse();
        ctx.engine.use_gas(Gas::tuple_gas_price(tuple.len()));
        ctx.engine.cc.stack.push_tuple(tuple);
        Ok(ctx)
    })
    .err()
}

// TUPLE n (x1 . . . xn – t)
pub(super) fn execute_tuple_create(engine: &mut Engine) -> Failure {
    tuple(engine, "TUPLE", CMD)
}

// TUPLEVAR (x1 . . . xn n – t)
pub(super) fn execute_tuple_createvar(engine: &mut Engine) -> Failure {
    tuple(engine, "TUPLEVAR", STACK)
}

fn tuple_index(engine: &mut Engine, how: u8) -> Failure {
    let index = how.mask(INDEX);
    let params = if index == 0 {2} else {1};
    engine.load_instruction(match index & 3 {
        0 => Instruction::new("INDEXVAR"),
        1 => Instruction::new("INDEX" ).set_opts(InstructionOptions::Length(0..16)),
        2 => Instruction::new("INDEX2").set_opts(InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromLastByte2Bits)),
        3 => Instruction::new("INDEX3").set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromLastByte2Bits)),
        _ => return err_opt!(ExceptionCode::FatalError)
    })
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        let n = if index == 0 {
            ctx.engine.cmd.var(0).as_integer()?.into(0..=254)?
        } else {
            0
        };
        if ctx.engine.cmd.var(params - 1).is_null() && how.bit(QUIET) {
            ctx.engine.cc.stack.push(StackItem::None);
            return Ok(ctx)
        }
        let len = ctx.engine.cmd.var(params - 1).as_tuple()?.len();
        match index & 3 {
            0 => {
                if n < len {
                    let value = ctx.engine.cmd.var(1).as_tuple()?[n].clone();
                    ctx.engine.cc.stack.push(value);
                    return Ok(ctx)
                } else if how.bit(QUIET) {
                    ctx.engine.cc.stack.push(StackItem::None);
                    return Ok(ctx)
                }
            }
            1 => {
                let n = ctx.engine.cmd.length();
                if n < len {
                    let value = ctx.engine.cmd.var_mut(0).as_tuple()?[n].clone();
                    ctx.engine.cc.stack.push(value);
                    return Ok(ctx)
                } else if how.bit(QUIET) {
                    ctx.engine.cc.stack.push(StackItem::None);
                    return Ok(ctx)
                }
            }
            2 => {
                let n = ctx.engine.cmd.sregs().ra;
                if n < len {
                    let value = ctx.engine.cmd.var(0).as_tuple()?[n].clone();
                    let n = ctx.engine.cmd.sregs().rb;
                    let len = value.as_tuple()?.len();
                    if n < len {
                        let value = value.as_tuple()?[n].clone();
                        ctx.engine.cc.stack.push(value);
                        return Ok(ctx)
                    }
                }
            }
            3 => {
                let n = ctx.engine.cmd.sregs3().ra;
                if n < len {
                    let value = ctx.engine.cmd.var(0).as_tuple()?[n].clone();
                    let n = ctx.engine.cmd.sregs3().rb;
                    let len = value.as_tuple()?.len();
                    if n < len {
                        let value = value.as_tuple()?[n].clone();
                        let n = ctx.engine.cmd.sregs3().rc;
                        let len = value.as_tuple()?.len();
                        if n < len {
                            let value = value.as_tuple()?[n].clone();
                            ctx.engine.cc.stack.push(value);
                            return Ok(ctx)
                        }
                    }
                }
            }
            _ => return err!(ExceptionCode::FatalError)
        } 
        err!(ExceptionCode::RangeCheckError)
    })
    .err()
}

// INDEX k (t – x)
pub(super) fn execute_tuple_index(engine: &mut Engine) -> Failure {
    tuple_index(engine, 1 | CMD)
}

// INDEXQ k (t – x)
pub(super) fn execute_tuple_index_quiet(engine: &mut Engine) -> Failure {
    tuple_index(engine, 1 | CMD | QUIET)
}

// INDEX2 i,j (t – x)
pub(super) fn execute_tuple_index2(engine: &mut Engine) -> Failure {
    tuple_index(engine, 2 | CMD)
}

// INDEX3 i,j,k (t – x)
pub(super) fn execute_tuple_index3(engine: &mut Engine) -> Failure {
    tuple_index(engine, 3 | CMD)
}

// INDEXVAR (t n – x)
pub(super) fn execute_tuple_indexvar(engine: &mut Engine) -> Failure {
    tuple_index(engine, STACK)
}

// INDEXVARQ (t n – x)
pub(super) fn execute_tuple_indexvar_quiet(engine: &mut Engine) -> Failure {
    tuple_index(engine, STACK | QUIET)
}

const INDEX: u8 = 0x03; // mask for INDEX index
const COUNT: u8 = 0x01;
const CMD:   u8 = 0x04;
const QUIET: u8 = 0x10;
const STACK: u8 = 0x08;

const CMP:   u8 = 0xC0; // mask for comparsion
const EXACT: u8 = 0x40;
const LESS:  u8 = 0x80;
const MORE:  u8 = 0xC0;

fn untuple(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    let mut params = 1;
    let mut inst = Instruction::new(name);

    if how.bit(STACK) {
        params += 1;
    }
    if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Length(0..16));
    }
    engine.load_instruction(inst)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        let mut n = if how.bit(CMD) {
            ctx.engine.cmd.length()
        } else if how.bit(STACK) {
            ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?
        } else {
            0
        };
        let tuple = ctx.engine.cmd.var(params - 1).as_tuple()?;
        let len = tuple.len();
        let mask = how.mask(CMP);
        if ((mask == EXACT) && (len != n))
            || ((mask == LESS) && (len < n))
            || ((mask == MORE) && (len > n)) {
            return err!(ExceptionCode::TypeCheckError)
        }
        if how.mask(CMP) == MORE {
            n = len;
        }
        ctx.engine.use_gas(Gas::tuple_gas_price(n));
        let mut vars: Vec<StackItem> = ctx.engine.cmd.var(params - 1).as_tuple()?.iter().take(n)
            .map(|v| v.clone()).collect();
        vars.drain(..).for_each(|v| {ctx.engine.cc.stack.push(v);});
        if how.bit(COUNT) {
            ctx.engine.cc.stack.push(int!(len));
        }
        Ok(ctx)
    })
    .err()
}

// ISTUPLE (t – ?)
pub(super) fn execute_istuple(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("ISTUPLE"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let tuple = ctx.engine.cmd.var(0).as_tuple().is_ok();
        ctx.engine.cc.stack.push(boolean!(tuple));
        Ok(ctx)
    })
    .err()
}

// UNPACKFIRST k (t – x1 . . . xk)
pub(super) fn execute_tuple_unpackfirst(engine: &mut Engine) -> Failure {
    untuple(engine, "UNPACKFIRST", CMD | LESS)
}

// UNPACKFIRSTVAR (t n – x1 . . . xn)
pub(super) fn execute_tuple_unpackfirstvar(engine: &mut Engine) -> Failure {
    untuple(engine, "UNPACKFIRSTVAR", STACK | LESS)
}

// UNTUPLE n (t – x1 . . . xn)
pub(super) fn execute_tuple_un(engine: &mut Engine) -> Failure {
    untuple(engine, "UNTUPLE", CMD | EXACT)
}

// UNTUPLEVAR (t n – x1 . . . xn)
pub(super) fn execute_tuple_untuplevar(engine: &mut Engine) -> Failure {
    untuple(engine, "UNTUPLEVAR", STACK | EXACT)
}

// EXPLODE n (t – x1 . . . xm m)
pub(super) fn execute_tuple_explode(engine: &mut Engine) -> Failure {
    untuple(engine, "EXPLODE", CMD | MORE | COUNT)
}

// EXPLODEVAR (t n – x1 . . . xm m)
pub(super) fn execute_tuple_explodevar(engine: &mut Engine) -> Failure {
    untuple(engine, "EXPLODEVAR", STACK | MORE | COUNT)
}

fn set_index(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    let mut params = 2;
    let mut inst = Instruction::new(name);

    if how.bit(STACK) {
        params += 1;
    }
    if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Length(0..16));
    }
    engine.load_instruction(inst)
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|ctx| {
        let n = if how.bit(CMD) {
            ctx.engine.cmd.length()
        } else if how.bit(STACK) {
            ctx.engine.cmd.var(0).as_integer()?.into(0..=254)?
        } else {
            0
        };
        let mut tuple = if how.bit(QUIET) && ctx.engine.cmd.var(params - 1).is_null() {
            vec![]
        } else {
            ctx.engine.cmd.var_mut(params - 1).as_tuple_mut()?
        };
        let var = ctx.engine.cmd.var_mut(params - 2).withdraw();
        let len = tuple.len();
        if n < len {
            tuple[n] = var;
        } else if how.bit(QUIET) {
            tuple.append(&mut vec![StackItem::None; n - len]);
            tuple.push(var);
        } else {
            return err!(ExceptionCode::RangeCheckError)
        }
        ctx.engine.use_gas(Gas::tuple_gas_price(tuple.len()));
        ctx.engine.cc.stack.push_tuple(tuple);
        Ok(ctx)
    })
    .err()
}

// SETINDEX k (t x – t0)
pub(super) fn execute_tuple_setindex(engine: &mut Engine) -> Failure {
    set_index(engine, "SETINDEX", CMD)
}

// SETINDEXQ k (t x – t0)
pub(super) fn execute_tuple_setindex_quiet(engine: &mut Engine) -> Failure {
    set_index(engine, "SETINDEXQ", CMD | QUIET)
}

// SETINDEXVAR (t x k – t0)
pub(super) fn execute_tuple_setindexvar(engine: &mut Engine) -> Failure {
    set_index(engine, "SETINDEXVAR", STACK)
}

// SETINDEXVARQ (t x k – t0)
pub(super) fn execute_tuple_setindexvar_quiet(engine: &mut Engine) -> Failure {
    set_index(engine, "SETINDEXVARQ", STACK | QUIET)
}

fn tuple_length(engine: &mut Engine, name: &'static str, how: u8) -> Failure {
    engine.load_instruction(Instruction::new(name))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let _ = match ctx.engine.cmd.var(0).as_tuple() {
            Ok(tuple) => ctx.engine.cc.stack.push(int!(tuple.len())),
            Err(_) if how.bit(QUIET) => ctx.engine.cc.stack.push(int!(-1)),
            Err(err) => return Err(err)
        };
        Ok(ctx)
    })
    .err()
}

// TLEN (t – n)
pub(super) fn execute_tuple_len(engine: &mut Engine) -> Failure {
    tuple_length(engine, "TLEN", 0)
}

// QTLEN (t – n or −1)
pub(super) fn execute_tuple_len_quiet(engine: &mut Engine) -> Failure {
    tuple_length(engine, "QTLEN", 0 | QUIET)
}

// LAST (t – x)
pub(super) fn execute_tuple_last(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("LAST"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let var = ctx.engine.cmd.var(0).as_tuple()?.last()
            .ok_or(ExceptionCode::TypeCheckError)?.clone();
        ctx.engine.cc.stack.push(var);
        Ok(ctx)
    })
    .err()
}

// TPUSH (t x – t0)
pub(super) fn execute_tuple_push(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("TPUSH"))
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let len = ctx.engine.cmd.var(1).as_tuple()?.len();
        if len >= 255 {
            return err!(ExceptionCode::TypeCheckError);
        }
        let mut tuple = ctx.engine.cmd.var_mut(1).as_tuple_mut()?;
        let value = ctx.engine.cmd.var(0).clone();
        tuple.push(value);
        ctx.engine.use_gas(Gas::tuple_gas_price(tuple.len()));
        ctx.engine.cc.stack.push_tuple(tuple);
        Ok(ctx)
    })
    .err()
}

// TPOP (t – t0 x)
pub(super) fn execute_tuple_pop(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("TPOP"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut tuple = ctx.engine.cmd.var_mut(0).as_tuple_mut()?;
        let value = tuple.pop().ok_or(ExceptionCode::TypeCheckError)?.clone();
        ctx.engine.use_gas(Gas::tuple_gas_price(tuple.len()));
        ctx.engine.cc.stack.push_tuple(tuple);
        ctx.engine.cc.stack.push(value);
        Ok(ctx)
    })
    .err()
}
