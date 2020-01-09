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

use executor::continuation::{callx, switch};
use executor::engine::storage::{fetch_stack};
use executor::engine::Engine;
use executor::Mask;
use executor::microcode::VAR;
use executor::types::{Ctx, Instruction, InstructionOptions};
use stack::{HashmapE, PfxHashmapE, HashmapType};
use stack::integer::serialization::{
    Encoding, IntoSliceExt, SignedIntegerBigEndianEncoding, UnsignedIntegerBigEndianEncoding,
};
use stack::serialization::Deserializer;
use stack::{BuilderData, ContinuationData, IntegerData, SliceData, StackItem};
use ton_types::GasConsumer;
use types::{Exception, ExceptionCode, Failure, Result};
use std::sync::Arc;

fn try_unref_leaf(slice: &SliceData) -> Result<StackItem> {
    match slice.remaining_bits() == 0 && slice.remaining_references() != 0 {
        true => Ok(StackItem::Cell(slice.reference(0)?.clone())),
        false => err!(ExceptionCode::DictionaryError)
    }
}

// Utilities ******************************************************************

const CNV: u8 = 0x01;     // CoNVert input value (from builder to slice)
const DEL: u8 = 0x02;     // DELete key 
const GET: u8 = 0x04;     // GET value from dictionary upon successful call
const INV: u8 = 0x08;     // INVert rule to get output value: get it upon unsuccessful call
const RET: u8 = 0x10;     // RETurn success flag
const SET: u8 = 0x20;     // SET value to dictionary
const SETGET: u8 = GET | SET | RET;

// Extensions
const CALLX: u8 = 0x40;   // CALLX to found value
const SWITCH: u8 = 0x80;  // SWITCH to found value

const CMD: u8 = 0x01;     // Get key from CMD

type KeyReader = fn(&StackItem, usize) -> Result<SliceData>;
type KeyWriter = fn(&mut Ctx, BuilderData) -> StackItem;
type KeyValFinder = fn(&mut Ctx, &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)>;
type KeyValReader = fn(&mut Ctx, &HashmapE, SliceData) -> Result<(Option<BuilderData>, Option<StackItem>)>;
type ValAccessor = fn(&mut Ctx, &mut HashmapE, SliceData) -> Result<Option<StackItem>>;

// Legend: ret = 0 if INV or -1
// ((value if SET) key slice nbits - 
// ((slice if SET or DEL) (value if GET) (ret if RET)) | ((slice if SET or DEL) (!ret if RET)))
fn dict(
    engine: &mut Engine,
    name: &'static str,
    keyreader: KeyReader,
    how: u8,
    handler: ValAccessor
) -> Failure {
    let params = if how.bit(SET) {
        4
    } else {
        if how.any(INV | CNV) {
            unimplemented!()
        }
        3
    };
    let ret = how.bit(INV);
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, params))
    .and_then(|mut ctx| {
        let nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        let mut dict = HashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
        let key = keyreader(ctx.engine.cmd.var(2), nbits)?;
        if key.is_empty() {
            if how.any(SET | DEL) {
                err!(ExceptionCode::RangeCheckError)
            } else {
                if how.bit(RET) {
                    ctx.engine.cc.stack.push(boolean!(false));
                }
                Ok(ctx)
            }
        } else {
            let val = handler(&mut ctx, &mut dict, key)?;
            if how.any(SET | DEL) {
                ctx.engine.cc.stack.push(dict!(dict));
            }
            match val {
                None => if how.bit(RET) {
                    ctx.engine.cc.stack.push(boolean!(ret));
                },
                Some(val) => {
                    if how.bit(GET) {
                        ctx.engine.cc.stack.push(val);
                    }
                    if how.bit(RET) {
                        ctx.engine.cc.stack.push(boolean!(!ret));
                    }
                }
            };
            Ok(ctx)
        }
    })
    .err()
}

// (key slice nbits - )
fn dictcont(
    engine: &mut Engine,
    name: &'static str,
    keyreader: KeyReader,
    how: u8
) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|ctx| {
        let nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        let dict = HashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
        let key = keyreader(ctx.engine.cmd.var(2), nbits)?;
        match dict.get_with_gas(key, &mut ctx.engine.gas)? {
            Some(data) => {
                ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
                    ContinuationData::with_code(data)
                )));
                let n = ctx.engine.cmd.var_count() - 1;
                if how.bit(SWITCH) {
                    switch(ctx, var!(n))
                } else if how.bit(CALLX) {
                    callx(ctx, n)
                } else {
                    unimplemented!()
                }
            }
            None => Ok(ctx)
        }
    })
    .err()
}

// (key slice nbits - (value' key' -1) | (0))
fn dictiter(
    engine: &mut Engine,
    name: &'static str,
    keyreader: KeyReader,
    valreader: KeyValReader,
    keywriter: KeyWriter
) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 3))
    .and_then(|mut ctx| {
        let nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        let dict = HashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
        let key = keyreader(ctx.engine.cmd.var(2), nbits)?;
        if key.is_empty() {
            ctx.engine.cc.stack.push(boolean!(false));
        } else {
            if let (Some(key), Some(value)) = valreader(&mut ctx, &dict, key)? {
                ctx.engine.cc.stack.push(value);
                let key = keywriter(&mut ctx, key);
                ctx.engine.cc.stack.push(key);
                ctx.engine.cc.stack.push(boolean!(true));
            } else {
                ctx.engine.cc.stack.push(boolean!(false));
            }
        }
        Ok(ctx)
    })
    .err()
}

// (slice nbits - (value' key -1) | (0))
fn find(
    engine: &mut Engine,
    name: &'static str,
    remove: bool,
    valfinder: KeyValFinder,
    keywriter: KeyWriter
) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|mut ctx| {
        let nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        let mut dict = HashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
        if let (Some(key), Some(value)) = valfinder(&mut ctx, &dict)? {
            if remove {
                dict.remove_with_gas(SliceData::from(&key), &mut ctx.engine.gas)?;
                ctx.engine.cc.stack.push(dict!(dict));
            }
            ctx.engine.cc.stack.push(value);
            let key = keywriter(&mut ctx, key);
            ctx.engine.cc.stack.push(key);
            ctx.engine.cc.stack.push(boolean!(true));
        } else {
            if remove {
                ctx.engine.cc.stack.push(dict!(dict));
            }
            ctx.engine.cc.stack.push(boolean!(false));
        }
        Ok(ctx)
    })
    .err()
}

// (value key slice nbits - slice -1|0)
fn pfxdictset(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| {
        let params = if how.bit(DEL) {
            3
        } else { 
            4
        };
        fetch_stack(ctx, params)
    })
    .and_then(|ctx| {
        let nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        let mut dict = PfxHashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
        let key = ctx.engine.cmd.var(2).as_slice()?.clone();
        let key_valid = if how.bit(DEL) { // remove
            dict.remove_with_gas(key, &mut ctx.engine.gas)?.is_some()
        } else {
            let value = ctx.engine.cmd.var(3).as_slice()?;
            if how.bit(INV) { // add
                if !dict.is_prefix(key.clone()) && dict.get(key.clone())?.is_none() {
                    dict.set_with_gas(key, value, &mut ctx.engine.gas)?;
                    true
                } else {
                    dict.get_with_gas(key, &mut ctx.engine.gas)?;
                    false
                }
            } else if how.bit(GET) { // replace
                dict.replace_with_gas(key.clone(), value, &mut ctx.engine.gas)?.is_some()
            } else { // set
                if !dict.is_prefix(key.clone()) {
                    dict.set_with_gas(key, value, &mut ctx.engine.gas)?;
                    true
                } else {
                    dict.get_prefix_leaf_with_gas(key.clone(), &mut ctx.engine.gas)?;
                    false
                }
            }
        };
        ctx.engine.cc.stack.push(dict!(dict));
        ctx.engine.cc.stack.push(boolean!(key_valid));
        Ok(ctx)
    })
    .err()
}

// (prefixed slice nbits - {prefix value suffix -1} | {prefixed | 0}
fn pfxdictget(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    let get_cont = how.bit(CALLX) || how.bit(SWITCH);
    let mut inst = Instruction::new(name);
    if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Dictionary(13, 10))
    }
    engine.load_instruction(inst)
    .and_then(|ctx| fetch_stack(ctx, if how.bit(CMD) {1} else {3}))
    .and_then(|ctx| {
        let (nbits, dict, mut key);
        if how.bit(CMD) {
            nbits = ctx.engine.cmd.length();
            dict  = PfxHashmapE::with_data(nbits, ctx.engine.cmd.slice().clone());
            key   = ctx.engine.cmd.var(0).as_slice()?.clone();
        } else {
            nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
            dict = PfxHashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
            key   = ctx.engine.cmd.var(2).as_slice()?.clone();
        }
        if let (prefix, Some(value), suffix) = dict.get_prefix_leaf_with_gas(key.clone(), &mut ctx.engine.gas)? {
            ctx.engine.cc.stack.push(StackItem::Slice(key.shrink_data(prefix.remaining_bits()..)));
            if get_cont {
                ctx.engine.cmd.vars.push(StackItem::Continuation(Arc::new(
                    ContinuationData::with_code(value)
                )));
            } else {
                ctx.engine.cc.stack.push(StackItem::Slice(value));
            }
            ctx.engine.cc.stack.push(StackItem::Slice(suffix));
            if how.bit(RET) {
                ctx.engine.cc.stack.push(boolean!(true)); 
            }
            if get_cont {
                let n = ctx.engine.cmd.var_count();
                if how.bit(SWITCH) {
                    Ok(switch(ctx, var!(n - 1)).unwrap())
                } else if how.bit(CALLX) {
                    Ok(callx(ctx, n - 1).unwrap())
                } else { 
                    unimplemented!()
                }
            } else { 
                Ok(ctx)
            }
        } else if how.bit(RET) || get_cont {
            ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
            if how.bit(RET) {
                ctx.engine.cc.stack.push(boolean!(false)); 
            }
            Ok(ctx)
        } else {
            err!(ExceptionCode::CellUnderflow)
        }
    })
    .err()
}

fn keyreader_from_slice(key: &StackItem, nbits: usize) -> Result<SliceData> {
    let mut key = key.as_slice()?.clone();
    if key.remaining_bits() < nbits {
        err!(ExceptionCode::CellUnderflow)
    } else {
        key.shrink_data(..nbits);
        key.shrink_references(..0);
        Ok(key)
    }
}

fn keyreader_from_int(key: &StackItem, nbits: usize) -> Result<SliceData> {
    let key = key.as_integer()?;
    if key.is_nan() {
        return err!(ExceptionCode::IntegerOverflow);
    }
    key.into_builder::<SignedIntegerBigEndianEncoding>(nbits).map(|builder| builder.into())
}

fn keyreader_from_uint(key: &StackItem, nbits: usize) -> Result<SliceData> {
    let key = key.as_integer()?;
    if key.is_nan() {
        return err!(ExceptionCode::IntegerOverflow);
    }
    key.into_builder::<UnsignedIntegerBigEndianEncoding>(nbits).map(|builder| builder.into())
}

fn valreader_from_slice(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    Ok(dict.get_with_gas(key, &mut ctx.engine.gas)?.map(|val| StackItem::Slice(val)))
}

fn valreader_from_ref(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    dict.get_with_gas(key, &mut ctx.engine.gas)?.map(|val| try_unref_leaf(&val)).transpose()
}

fn valreader_from_refopt(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    dict.get_with_gas(key, &mut ctx.engine.gas)?.map(|val| try_unref_leaf(&val)).or(Some(Ok(StackItem::None))).transpose()
}

fn valwriter_add_slice(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = ctx.engine.cmd.var(3).as_slice()?;
    match dict.add_with_gas(key.clone(), new_val, &mut ctx.engine.gas)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None),
    }
}

fn valwriter_add_builder(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = ctx.engine.cmd.var(3).as_builder()?.into();
    match dict.add_with_gas(key.clone(), &new_val, &mut ctx.engine.gas)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None),
    }
}

fn valwriter_add_ref(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = ctx.engine.cmd.var(3).as_cell()?;
    match dict.addref_with_gas(key.clone(), new_val, &mut ctx.engine.gas)? {
        Some(val) => Ok(Some(try_unref_leaf(&val)?)),
        None => Ok(None),
    }
}

fn valwriter_add_ref_without_unref(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = ctx.engine.cmd.var(3).as_cell()?;
    match dict.get_with_gas(key.clone(), &mut ctx.engine.gas)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => {
            dict.setref_with_gas(key, new_val, &mut ctx.engine.gas)?;
            Ok(None)
        }
    }
}

fn valwriter_add_or_remove_refopt(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let old_value = match ctx.engine.cmd.var(3).as_dict()? {
        Some(new_val) => dict.setref_with_gas(key, new_val, &mut ctx.engine.gas)?,
        None => dict.remove_with_gas(key, &mut ctx.engine.gas)?
    };
    old_value.map(|val| try_unref_leaf(&val)).or(Some(Ok(StackItem::None))).transpose()
}

fn valwriter_remove_slice(ctx: &mut Ctx, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    Ok(dict.remove_with_gas(key, &mut ctx.engine.gas)?.map(|val| StackItem::Slice(val)))
}

fn valwriter_remove_ref(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    dict.remove_with_gas(key, &mut ctx.engine.gas)?.map(|val| try_unref_leaf(&val)).transpose()
}

fn valwriter_replace_slice(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = ctx.engine.cmd.var(3).as_slice()?;
    match dict.replace_with_gas(key, val, &mut ctx.engine.gas)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None)
    }
}

fn valwriter_replace_builder(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = ctx.engine.cmd.var(3).as_builder()?.into();
    match dict.replace_with_gas(key, &val, &mut ctx.engine.gas)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None)
    }
}

fn valwriter_replace_ref(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = ctx.engine.cmd.var(3).as_cell()?;
    match dict.replaceref_with_gas(key, val, &mut ctx.engine.gas)? {
        Some(val) => Some(try_unref_leaf(&val)).transpose(),
        None => Ok(None)
    }
}

fn valwriter_to_slice(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = ctx.engine.cmd.var(3).as_slice()?;
    Ok(dict.set_with_gas(key, val, &mut ctx.engine.gas)?.map(|val| StackItem::Slice(val)))
}

fn valwriter_to_builder(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = ctx.engine.cmd.var(3).as_builder()?.into();
    Ok(dict.set_with_gas(key, &val, &mut ctx.engine.gas)?.map(|val| StackItem::Slice(val)))
}

fn valwriter_to_ref(
    ctx: &mut Ctx,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = ctx.engine.cmd.var(3).as_cell()?;
    dict.setref_with_gas(key, val, &mut ctx.engine.gas)?.map(|val| try_unref_leaf(&val)).transpose()
}

const NEXT: u8 = 0x01;
const SAME: u8 = 0x02;
const SIGNED : u8 = 0x04;

fn iter_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    how: u8, 
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    let (key, val) = dict.find_leaf(key, how.bit(NEXT), how.bit(SAME), how.bit(SIGNED), &mut ctx.engine.gas)?;
    let val = val.map(|val| StackItem::Slice(val));
    Ok((key, val))
}

fn eq_next_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, NEXT | SAME, key)
}

fn eq_prev_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, SAME, key)
}

fn next_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, NEXT, key)
}

fn prev_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, 0, key)
}

fn signed_next_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, NEXT | SIGNED, key)
}

fn signed_eq_next_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, NEXT | SAME | SIGNED, key)
}

fn signed_prev_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, SIGNED, key)
}

fn signed_eq_prev_reader(
    ctx: &mut Ctx,
    dict: &HashmapE,
    key: SliceData
) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    iter_reader(ctx, dict, SAME | SIGNED, key)
}

const MIN: u8 = 0x01;
const REF: u8 = 0x02;
const SIG: u8 = 0x04;

fn finder(ctx: &mut Ctx, dict: &HashmapE, how: u8) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    let (key, val) = if how.bit(MIN) {
        dict.get_min(how.bit(SIG), &mut ctx.engine.gas)?
    } else {
        dict.get_max(how.bit(SIG), &mut ctx.engine.gas)?
    };
    val.map(|val| if how.bit(REF) {
        try_unref_leaf(&val)
    } else {
        Ok(StackItem::Slice(val))
    }).transpose().map(|val| (key, val))
}

fn finder_max(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, 0)
}

fn finder_max_ref(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, REF)
}

fn finder_min(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, MIN)
}

fn finder_min_ref(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, MIN | REF)
}

fn finder_imax(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, SIG)
}

fn finder_imax_ref(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, REF | SIG)
}

fn finder_imin(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, MIN | SIG)
}

fn finder_imin_ref(ctx: &mut Ctx, dict: &HashmapE) -> Result<(Option<BuilderData>, Option<StackItem>)> {
    finder(ctx, dict, MIN | REF | SIG)
}

fn keywriter_to_int(_ctx: &mut Ctx, key: BuilderData) -> StackItem {
    let encoding = SignedIntegerBigEndianEncoding::new(key.length_in_bits());
    let ret = encoding.deserialize(key.data());
    StackItem::Integer(Arc::new(ret))
}

fn keywriter_to_uint(_ctx: &mut Ctx, key: BuilderData) -> StackItem {
    let encoding = UnsignedIntegerBigEndianEncoding::new(key.length_in_bits());
    let ret = encoding.deserialize(key.data());
    StackItem::Integer(Arc::new(ret))
}

fn keywriter_to_slice(ctx: &mut Ctx, key: BuilderData) -> StackItem {
    StackItem::Slice(key.finalize(&mut ctx.engine.gas).into())
}

// Dictionary manipulation primitives *****************************************

// (value key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictadd(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTADD", keyreader_from_slice, INV | RET | SET, valwriter_add_slice)
}

// (builder key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictaddb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTADDB", keyreader_from_slice, CNV | INV | RET | SET, valwriter_add_builder)
}

// (value key slice nbits - (slice -1) | (slice y 0))
pub(super) fn execute_dictaddget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTADDGET", keyreader_from_slice, INV | SETGET, valwriter_add_slice)
}

// (builder key slice nbits - (slice -1) | (slice value 0))
pub(super) fn execute_dictaddgetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTADDGETB", keyreader_from_slice, CNV | INV | SETGET, valwriter_add_builder)
}

// (cell key slice nbits - (slice' -1) | (slice cell 0))
pub(super) fn execute_dictaddgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTADDGETREF", keyreader_from_slice, INV | SETGET, valwriter_add_ref)
}

// (cell key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictaddref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTADDREF", keyreader_from_slice, INV | RET | SET, valwriter_add_ref_without_unref)
}

// (key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictdel(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTDEL", keyreader_from_slice, DEL | RET, valwriter_remove_slice)
}

// (key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictdelget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTDELGET", keyreader_from_slice,  DEL | GET | RET, valwriter_remove_slice)
}

// (key slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictdelgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTDELGETREF", keyreader_from_slice, DEL | GET | RET, valwriter_remove_ref)
}

// (key slice nbits - (value -1) | (0))
pub(super) fn execute_dictget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTGET", keyreader_from_slice, GET | RET, valreader_from_slice)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetnext(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTGETNEXT", keyreader_from_slice, next_reader, keywriter_to_slice)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetnexteq(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTGETNEXTEQ", keyreader_from_slice, eq_next_reader, keywriter_to_slice)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetprev(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTGETPREV", keyreader_from_slice, prev_reader, keywriter_to_slice)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetpreveq(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTGETPREVEQ", keyreader_from_slice, eq_prev_reader, keywriter_to_slice)
}

// (key slice nbits - (cell -1) | (0))
pub(super) fn execute_dictgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTGETREF", keyreader_from_slice, GET | RET, valreader_from_ref)
}

// (value int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictiadd(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIADD", keyreader_from_int, INV | RET | SET, valwriter_add_slice)
}

// (builder int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictiaddb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIADDB", keyreader_from_int, CNV | INV | RET | SET, valwriter_add_builder)
}

// (value int slice nbits - (slice -1) | (slice value 0))
pub(super) fn execute_dictiaddget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIADDGET", keyreader_from_int, INV | SETGET, valwriter_add_slice)
}

// (builder int slice nbits - (slice' -1) | (slice y 0))
pub(super) fn execute_dictiaddgetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIADDGETB", keyreader_from_int, CNV | INV | SETGET, valwriter_add_builder)
}

// (cell int slice nbits - (slice -1) | (slice cell 0))
pub(super) fn execute_dictiaddgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIADDGETREF", keyreader_from_int, INV | SETGET, valwriter_add_ref)
}

// (cell int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictiaddref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIADDREF", keyreader_from_int, INV | RET | SET, valwriter_add_ref_without_unref)
}

// (int slice nbits - (slice' -1) | (slice 0))
pub(super) fn execute_dictidel(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIDEL", keyreader_from_int, DEL | RET, valwriter_remove_slice)
}

// (int slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictidelget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIDELGET", keyreader_from_int, DEL | GET | RET, valwriter_remove_slice)
}

// (int slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictidelgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIDELGETREF", keyreader_from_int, DEL | GET | RET, valwriter_remove_ref)
}

// (int slice nbits - (value -1) | (0))
pub(super) fn execute_dictiget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIGET", keyreader_from_int, GET | RET, valreader_from_slice)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetnext(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTIGETNEXT", keyreader_from_int, signed_next_reader, keywriter_to_int)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetnexteq(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTIGETNEXTEQ", keyreader_from_int, signed_eq_next_reader, keywriter_to_int)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetprev(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTIGETPREV", keyreader_from_int, signed_prev_reader, keywriter_to_int)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetpreveq(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTIGETPREVEQ", keyreader_from_int, signed_eq_prev_reader, keywriter_to_int)
}

// (int slice nbits - (cell -1) | (0))
pub(super) fn execute_dictigetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIGETREF", keyreader_from_int, GET | RET, valreader_from_ref)
}

// (slice nbits - (value int -1) | (0))
pub(super) fn execute_dictimax(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIMAX", false, finder_imax, keywriter_to_int)
}

// (slice nbits - (cell int -1) | (0))
pub(super) fn execute_dictimaxref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIMAXREF", false, finder_imax_ref, keywriter_to_int)
}

// (slice nbits - (value int -1) | (0))
pub(super) fn execute_dictimin(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIMIN", false, finder_imin, keywriter_to_int)
}

// (slice nbits - (cell int -1) | (0))
pub(super) fn execute_dictiminref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIMINREF", false, finder_imin_ref, keywriter_to_int)
}

// (slice nbits - (slice value int -1) | (0))
pub(super) fn execute_dictiremmax(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIREMMAX", true, finder_imax, keywriter_to_int)
}

// (slice nbits - (slice cell int -1) | (0))
pub(super) fn execute_dictiremmaxref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIREMMAXREF", true, finder_imax_ref, keywriter_to_int)
}

// (slice nbits - (slice value int -1) | (0))
pub(super) fn execute_dictiremmin(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIREMMIN", true, finder_imin, keywriter_to_int)
}

// (slice nbits - (slice cell int -1) | (0))
pub(super) fn execute_dictiremminref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTIREMMINREF", true, finder_imin_ref, keywriter_to_int)
}

// (value int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictireplace(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIREPLACE", keyreader_from_int, RET | SET, valwriter_replace_slice)
}

// (builder int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictireplaceb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIREPLACEB", keyreader_from_int, CNV | RET | SET, valwriter_replace_builder)
}

// (value int slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictireplaceget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIREPLACEGET", keyreader_from_int,  SETGET, valwriter_replace_slice)
}

// (builder int slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictireplacegetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIREPLACEGETB", keyreader_from_int, CNV | SETGET, valwriter_replace_builder)
}

// (cell int slice nbits - (slice' cell -1) | (slice 0))
pub(super) fn execute_dictireplacegetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIREPLACEGETREF", keyreader_from_int, SETGET, valwriter_replace_ref)
}

// (cell int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictireplaceref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIREPLACEREF", keyreader_from_int, RET | SET, valwriter_replace_ref)
}

// (value int slice nbits - slice)
pub(super) fn execute_dictiset(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISET", keyreader_from_int, SET, valwriter_to_slice)
}

// (builder int slice nbits - slice)
pub(super) fn execute_dictisetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISETB", keyreader_from_int, CNV | SET, valwriter_to_builder)
}

// (value int slice nbits - (slice y -1) | (slice 0))
pub(super) fn execute_dictisetget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISETGET", keyreader_from_int, SETGET, valwriter_to_slice)
}

// (builder int slice nbits - (slice' y -1) | (slice' 0))
pub(super) fn execute_dictisetgetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISETGETB", keyreader_from_int, CNV | SETGET, valwriter_to_builder)
}

// (cell int slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictisetgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISETGETREF", keyreader_from_int, SETGET, valwriter_to_ref)
}

// (cell int slice nbits - slice)
pub(super) fn execute_dictisetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISETREF", keyreader_from_int, SET, valwriter_to_ref)
}

// (slice nbits - (value key -1) | (0))
pub(super) fn execute_dictmax(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTMAX", false, finder_max, keywriter_to_slice)
}

// (slice nbits - (cell key -1) | (0))
pub(super) fn execute_dictmaxref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTMAXREF", false, finder_max_ref, keywriter_to_slice)
}

// (slice nbits - (value key -1) | (0))
pub(super) fn execute_dictmin(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTMIN", false, finder_min, keywriter_to_slice)
}

// (slice nbits - (cell key -1) | (0))
pub(super) fn execute_dictminref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTMINREF", false, finder_min_ref, keywriter_to_slice)
}

// (slice nbits - (slice value key -1) | (0))
pub(super) fn execute_dictremmax(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTREMMAX", true, finder_max, keywriter_to_slice)
}

// (slice nbits - (slice cell key -1) | (0))
pub(super) fn execute_dictremmaxref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTREMMAXREF", true, finder_max_ref, keywriter_to_slice)
}

// (slice nbits - (slice value key -1) | (0))
pub(super) fn execute_dictremmin(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTREMMIN", true, finder_min, keywriter_to_slice)
}

// (slice nbits - (slice cell key -1) | (0))
pub(super) fn execute_dictremminref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTREMMINREF", true, finder_min_ref, keywriter_to_slice)
}

// (value key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictreplace(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTREPLACE", keyreader_from_slice, RET | SET, valwriter_replace_slice)
}

// (builder key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictreplaceb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTREPLACEB", keyreader_from_slice, CNV | RET | SET, valwriter_replace_builder)
}

// (value key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictreplaceget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTREPLACEGET", keyreader_from_slice, SETGET, valwriter_replace_slice)
}

// (builder key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictreplacegetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTREPLACEGETB", keyreader_from_slice, CNV | SETGET, valwriter_replace_builder)
}

// (cell key slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictreplacegetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTREPLACEGETREF", keyreader_from_slice, SETGET, valwriter_replace_ref)
}

// (cell key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictreplaceref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTREPLACEREF", keyreader_from_slice, RET | SET, valwriter_replace_ref)
}

// (value key slice nbits - slice)
pub(super) fn execute_dictset(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSET", keyreader_from_slice, SET, valwriter_to_slice)
}

// (builder key slice nbits - slice)
pub(super) fn execute_dictsetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSETB", keyreader_from_slice, CNV | SET, valwriter_to_builder)
}

// (value key slice nbits - (slice y -1) | (slice 0))
pub(super) fn execute_dictsetget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSETGET", keyreader_from_slice, SETGET, valwriter_to_slice)
}

// (builder key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictsetgetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSETGETB", keyreader_from_slice, CNV | SETGET, valwriter_to_builder)
}

// (cell key slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictsetgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSETGETREF", keyreader_from_slice, SETGET, valwriter_to_ref)
}

// (cell key slice nbits - slice)
pub(super) fn execute_dictsetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSETREF", keyreader_from_slice, SET, valwriter_to_ref)
}

// (value uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictuadd(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUADD", keyreader_from_uint, INV | RET | SET, valwriter_add_slice)
}

// (builder uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictuaddb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUADDB", keyreader_from_uint, CNV | INV | RET | SET, valwriter_add_builder)
}

// (value uint slice nbits - (slice -1) | (slice value 0))
pub(super) fn execute_dictuaddget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUADDGET", keyreader_from_uint, INV | SETGET, valwriter_add_slice)
}

// (builder uint slice nbits - (slice' -1) | (slice y 0))
pub(super) fn execute_dictuaddgetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUADDGETB", keyreader_from_uint, CNV | INV | SETGET, valwriter_add_builder)
}

// (cell uint slice nbits - (slice -1) | (slice cell 0))
pub(super) fn execute_dictuaddgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUADDGETREF", keyreader_from_uint, INV | SETGET, valwriter_add_ref)
}

// (cell uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictuaddref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUADDREF", keyreader_from_uint, INV | RET | SET, valwriter_add_ref_without_unref)
}

// (uint slice nbits - (slice' -1) | (slice 0))
pub(super) fn execute_dictudel(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUDEL", keyreader_from_uint, DEL | RET, valwriter_remove_slice)
}

// (uint slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictudelget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUDELGET", keyreader_from_uint, DEL | GET | RET, valwriter_remove_slice)
}

// (uint slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictudelgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUDELGETREF", keyreader_from_uint, DEL | GET | RET, valwriter_remove_ref)
}

// (uint slice nbits - (value -1) | (0))
pub(super) fn execute_dictuget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUGET", keyreader_from_uint, GET | RET, valreader_from_slice)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetnext(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTUGETNEXT", keyreader_from_uint, next_reader, keywriter_to_uint)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetnexteq(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTUGETNEXTEQ", keyreader_from_uint, eq_next_reader, keywriter_to_uint)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetprev(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTUGETPREV", keyreader_from_uint, prev_reader, keywriter_to_uint)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetpreveq(engine: &mut Engine) -> Option<Exception> {
    dictiter(engine, "DICTUGETPREVEQ", keyreader_from_uint, eq_prev_reader, keywriter_to_uint)
}

// (uint slice nbits - (cell -1) | (0))
pub(super) fn execute_dictugetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUGETREF", keyreader_from_uint, GET | RET, valreader_from_ref)
}

// (slice nbits - (value uint -1) | (0))
pub(super) fn execute_dictumax(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUMAX", false, finder_max, keywriter_to_uint)
}

// (slice nbits - (cell uint -1) | (0))
pub(super) fn execute_dictumaxref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUMAXREF", false, finder_max_ref, keywriter_to_uint)
}

// (slice nbits - (value uint -1) | (0))
pub(super) fn execute_dictumin(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUMIN", false, finder_min, keywriter_to_uint)
}

// (slice nbits - (cell uint -1) | (0))
pub(super) fn execute_dictuminref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUMINREF", false, finder_min_ref, keywriter_to_uint)
}

// (slice nbits - (slice value uint -1) | (0))
pub(super) fn execute_dicturemmax(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUREMMAX", true, finder_max, keywriter_to_uint)
}

// (slice nbits - (slice cell uint -1) | (0))
pub(super) fn execute_dicturemmaxref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUREMMAXREF", true, finder_max_ref, keywriter_to_uint)
}

// (slice nbits - (slice value uint -1) | (0))
pub(super) fn execute_dicturemmin(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUREMMIN", true, finder_min, keywriter_to_uint)
}

// (slice nbits - (slice cell uint -1) | (0))
pub(super) fn execute_dicturemminref(engine: &mut Engine) -> Option<Exception> {
    find(engine, "DICTUREMMINREF", true, finder_min_ref, keywriter_to_uint)
}

// (value uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictureplace(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUREPLACE", keyreader_from_uint, RET | SET, valwriter_replace_slice)
}

// (builder uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictureplaceb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUREPLACEB", keyreader_from_uint, CNV | RET | SET, valwriter_replace_builder)
}

// (value uint slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictureplaceget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUREPLACEGET", keyreader_from_uint,  SETGET, valwriter_replace_slice)
}

// (builder uint slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictureplacegetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUREPLACEGETB", keyreader_from_uint, CNV | SETGET, valwriter_replace_builder)
}

// (cell uint slice nbits - (slice' cell -1) | (slice 0))
pub(super) fn execute_dictureplacegetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUREPLACEGETREF", keyreader_from_uint, SETGET, valwriter_replace_ref)
}

// (cell uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictureplaceref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUREPLACEREF", keyreader_from_uint, RET | SET, valwriter_replace_ref)
}

// (value uint slice nbits - slice)
pub(super) fn execute_dictuset(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSET", keyreader_from_uint, SET, valwriter_to_slice)
}

// (builder uint slice nbits - slice)
pub(super) fn execute_dictusetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSETB", keyreader_from_uint, CNV | SET, valwriter_to_builder)
}

// (value uint slice nbits - (slice y -1) | (slice 0))
pub(super) fn execute_dictusetget(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSETGET", keyreader_from_uint, SETGET, valwriter_to_slice)
}

// (builder uint slice nbits - (slice' y -1) | (slice' 0))
pub(super) fn execute_dictusetgetb(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSETGETB", keyreader_from_uint, CNV | SETGET, valwriter_to_builder)
}

// (cell uint slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictusetgetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSETGETREF", keyreader_from_uint, SETGET, valwriter_to_ref)
}

// (cell uint slice nbits - slice)
pub(super) fn execute_dictusetref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSETREF", keyreader_from_uint, SET, valwriter_to_ref)
}

pub(super) fn execute_dictpushconst(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new("DICTPUSHCONST").set_opts(InstructionOptions::Dictionary(13, 10))
    )
    .and_then(|ctx| {
        let slice = ctx.engine.cmd.slice();
        if slice.remaining_references() == 0 {
            return err!(ExceptionCode::InvalidOpcode);
        } else {
            ctx.engine.cc.stack.push(StackItem::Cell(slice.reference(0)?.clone()));
        }
        let key = ctx.engine.cmd.length();
        ctx.engine.cc.stack.push(int!(key));
        Ok(ctx)
    })
    .err()
}

// (int slice nbits - )
pub(super) fn execute_dictigetjmp(engine: &mut Engine) -> Option<Exception> {
    dictcont(engine, "DICTIGETJMP", keyreader_from_int, SWITCH)
}

// (uint slice nbits - )
pub(super) fn execute_dictugetjmp(engine: &mut Engine) -> Option<Exception> {
    dictcont(engine, "DICTUGETJMP", keyreader_from_uint, SWITCH)
}

// (int slice nbits - )
pub(super) fn execute_dictigetexec(engine: &mut Engine) -> Option<Exception> {
    dictcont(engine, "DICTIGETEXEC", keyreader_from_int, CALLX)
}

// (uint slice nbits - )
pub(super) fn execute_dictugetexec(engine: &mut Engine) -> Option<Exception> {
    dictcont(engine, "DICTUGETEXEC", keyreader_from_uint, CALLX)
}

// (value key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictset(engine: &mut Engine) -> Option<Exception> {
    pfxdictset(engine, "PFXDICTSET", 0)
}

// (value key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictreplace(engine: &mut Engine) -> Option<Exception> {
    pfxdictset(engine, "PFXDICTREPLACE", GET)
}

// (value key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictadd(engine: &mut Engine) -> Option<Exception> {
    pfxdictset(engine, "PFXDICTADD", INV | GET)
}

// (key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictdel(engine: &mut Engine) -> Option<Exception> {
    pfxdictset(engine, "PFXDICTDEL", DEL)
}

// (prefixed slice nbits - {prefix value suffix -1} | {prefixed | 0}
pub(super) fn execute_pfxdictgetq(engine: &mut Engine) -> Option<Exception> {
    pfxdictget(engine, "PFXDICTGETQ", RET)
}

// (prefixed slice nbits - prefix value suffix -1}
pub(super) fn execute_pfxdictget(engine: &mut Engine) -> Option<Exception> {
    pfxdictget(engine, "PFXDICTGET", 0)
}

// (s' s n - (s'' s''') | (s')))
pub(super) fn execute_pfxdictgetjmp(engine: &mut Engine) -> Option<Exception> {
    pfxdictget(engine, "PFXDICTGETJMP", SWITCH)
}

// (s' s n - (s'' s'''))
pub(super) fn execute_pfxdictgetexec(engine: &mut Engine) -> Option<Exception> {
    pfxdictget(engine, "PFXDICTGETEXEC", CALLX)
}

// (s' - (s'' s''') | (s')))
pub(super) fn execute_pfxdictswitch(engine: &mut Engine) -> Option<Exception> {
    pfxdictget(engine, "PFXDICTSWITCH", CMD | SWITCH)
}

const QUIET: u8 = 0x01; // quiet variant
const DICT: u8 = 0x02; // dictionary
const SLC: u8 = 0x04; // slice
const REST: u8 = 0x08; // remainder

fn load_dict(engine: &mut Engine, name: &'static str, how: u8) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut slice = ctx.engine.cmd.var(0).as_slice()?.clone();
        let empty = if let Ok(dict) = slice.get_dictionary() {
            if how.bit(SLC) {
                ctx.engine.cc.stack.push(StackItem::Slice(dict.clone()));
            } else if how.bit(DICT) {
                ctx.engine.cc.stack.push(if dict.is_empty_root() {
                    StackItem::None
                } else {
                    StackItem::Cell(dict.reference(0)?.clone())
                });
            }
            false
        } else {
            slice = ctx.engine.cmd.var(0).as_slice()?.clone();
            true
        };
        if how.bit(REST) {
            ctx.engine.cc.stack.push(StackItem::Slice(slice));
        }
        if how.bit(QUIET) {
            ctx.engine.cc.stack.push(boolean!(!empty));
        } else if empty {
            return err!(ExceptionCode::CellUnderflow)
        }
        Ok(ctx)
    })
    .err()
}

// (slice - slice)
pub(super) fn execute_skipdict(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "SKIPDICT", REST)
}

// (slice - D slice)
pub(super) fn execute_lddict(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "LDDICT", REST | DICT)
}

// (slice - D)
pub(super) fn execute_plddict(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "PLDDICT", DICT)
}

// (slice - slice slice)
pub(super) fn execute_lddicts(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "LDDICTS", REST | SLC)
}

// (slice - slice)
pub(super) fn execute_plddicts(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "PLDDICTS", SLC)
}

// (slice - (D slice -1) | (slice 0))
pub(super) fn execute_lddictq(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "LDDICTQ", REST | DICT | QUIET)
}

// (slice - (D -1) | (0))
pub(super) fn execute_plddictq(engine: &mut Engine) -> Option<Exception> {
    load_dict(engine, "PLDDICTQ", DICT | QUIET)
}

type IntoSubtree = fn(&mut HashmapE, prefix: SliceData, &mut dyn GasConsumer);
fn subdict(engine: &mut Engine, name: &'static str, keyreader: KeyReader, into: IntoSubtree) -> Option<Exception> {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 4))
    .and_then(|ctx| {
        let nbits = ctx.engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        let mut dict = HashmapE::with_hashmap(nbits, ctx.engine.cmd.var(1).as_dict()?.cloned());
        let lbits = ctx.engine.cmd.var(2).as_integer()?.into(0..=nbits)?;
        let key = keyreader(ctx.engine.cmd.var(3), lbits)?;
        into(&mut dict, key, &mut ctx.engine.gas);
        ctx.engine.cc.stack.push(dict!(dict));
        Ok(ctx)
    })
    .err()
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictget(engine: &mut Engine) -> Option<Exception> {
    subdict(engine, "SUBDICTGET", keyreader_from_slice, HashmapE::into_subtree_with_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictiget(engine: &mut Engine) -> Option<Exception> {
    subdict(engine, "SUBDICTIGET", keyreader_from_int, HashmapE::into_subtree_with_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictuget(engine: &mut Engine) -> Option<Exception> {
    subdict(engine, "SUBDICTUGET", keyreader_from_uint, HashmapE::into_subtree_with_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictrpget(engine: &mut Engine) -> Option<Exception> {
    subdict(engine, "SUBDICTRPGET", keyreader_from_slice, HashmapE::into_subtree_without_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictirpget(engine: &mut Engine) -> Option<Exception> {
    subdict(engine, "SUBDICTIRPGET", keyreader_from_int, HashmapE::into_subtree_without_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdicturpget(engine: &mut Engine) -> Option<Exception> {
    subdict(engine, "SUBDICTURPGET", keyreader_from_uint, HashmapE::into_subtree_without_prefix)
}
pub(super) fn execute_dictgetoptref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTGETOPTREF", keyreader_from_slice, GET, valreader_from_refopt)
}

pub(super) fn execute_dictigetoptref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTIGETOPTREF", keyreader_from_int, GET, valreader_from_refopt)
}

pub(super) fn execute_dictugetoptref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUGETOPTREF", keyreader_from_uint, GET, valreader_from_refopt)
}

pub(super) fn execute_dictsetgetoptref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTSETGETOPTREF", keyreader_from_slice, SET | GET, valwriter_add_or_remove_refopt)
}

pub(super) fn execute_dictisetgetoptref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTISETGETOPTREF", keyreader_from_int, SET | GET, valwriter_add_or_remove_refopt)
}

pub(super) fn execute_dictusetgetoptref(engine: &mut Engine) -> Option<Exception> {
    dict(engine, "DICTUSETGETOPTREF", keyreader_from_uint, SET | GET, valwriter_add_or_remove_refopt)
}
