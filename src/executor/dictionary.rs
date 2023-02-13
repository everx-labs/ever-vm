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
    error::TvmError,
    executor::{
        Mask, continuation::{callx, switch}, engine::{Engine, storage::{fetch_stack}},
        microcode::VAR, types::{Instruction, InstructionOptions}
    },
    stack::{
        StackItem, continuation::ContinuationData,
        integer::{
            IntegerData,
            serialization::{
                Encoding, SignedIntegerBigEndianEncoding,
                UnsignedIntegerBigEndianEncoding
            }
        },
        serialization::Deserializer
    },
    types::{Exception, Status}
};
use ton_types::{
    BuilderData, error, fail, GasConsumer, HashmapE, HashmapSubtree, PfxHashmapE, Result, SliceData,
    types::ExceptionCode
};

fn try_unref_leaf(slice: SliceData) -> Result<StackItem> {
    match slice.remaining_bits() == 0 && slice.remaining_references() != 0 {
        true => Ok(StackItem::Cell(slice.reference(0)?)),
        false => err!(ExceptionCode::DictionaryError)
    }
}

// Utilities ******************************************************************

const CNV: u8 = 0x01;     // CoNVert input value (from builder to slice)
const SET: u8 = 0x02;     // SET value to dictionary
const GET: u8 = 0x04;     // GET value from dictionary upon successful call
const INV: u8 = 0x08;     // INVert rule to get output value: get it upon unsuccessful call
const RET: u8 = 0x10;     // RETurn success flag
const DEL: u8 = 0x20;     // DELete key
const SETGET: u8 = GET | SET | RET;

// Extensions
const STAY: u8 = 0x20;  // STAY argument on stack in failure case
const CALLX: u8 = 0x40;   // CALLX to found value
const SWITCH: u8 = 0x80;  // SWITCH to found value

const CMD: u8 = 0x01;     // Get key from CMD

type KeyReader = fn(&StackItem, usize) -> Result<SliceData>;
type ValAccessor = fn(&mut Engine, &mut HashmapE, SliceData) -> Result<Option<StackItem>>;

// Legend: ret = 0 if INV or -1
// ((value if SET) key slice nbits -
// ((slice if SET or DEL) (value if GET) (ret if RET)) | ((slice if SET or DEL) (!ret if RET)))
fn dict(
    engine: &mut Engine,
    name: &'static str,
    keyreader: KeyReader,
    how: u8,
    handler: ValAccessor
) -> Status {
    let params = if how.bit(SET) {
        4
    } else if how.any(INV | CNV) {
        fail!("dict: {:X}", how)
    } else {
        3
    };
    let ret = how.bit(INV);
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, params)?;
    let nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
    let mut dict = HashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
    let key = keyreader(engine.cmd.var(2), nbits)?;
    if key.is_empty() {
        if how.any(SET | DEL) {
            err!(ExceptionCode::RangeCheckError, "key cannot be empty for set or delete")
        } else {
            if how.bit(RET) {
                engine.cc.stack.push(boolean!(false));
            }
            Ok(())
        }
    } else {
        let val = handler(engine, &mut dict, key)?;
        if how.any(SET | DEL) {
            engine.cc.stack.push(StackItem::dict(&dict));
        }
        match val {
            None => if how.bit(RET) {
                engine.cc.stack.push(boolean!(ret));
            },
            Some(val) => {
                if how.bit(GET) {
                    engine.cc.stack.push(val);
                }
                if how.bit(RET) {
                    engine.cc.stack.push(boolean!(!ret));
                }
            }
        };
        Ok(())
    }
}

// (key slice nbits - )
fn dictcont(
    engine: &mut Engine,
    name: &'static str,
    keyreader: KeyReader,
    how: u8
) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 3)?;
    let nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
    let dict = HashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
    let key = keyreader(engine.cmd.var(2), nbits)?;
    if let Some(data) = dict.get_with_gas(key, engine)? {
        engine.cmd.vars.push(StackItem::continuation(
            ContinuationData::with_code(data)
        ));
        let n = engine.cmd.var_count() - 1;
        if how.bit(SWITCH) {
            switch(engine, var!(n))
        } else if how.bit(CALLX) {
            callx(engine, n, false)
        } else {
            fail!("dictcont: {:X}", how)
        }
    } else if how.bit(STAY) {
        let var = engine.cmd.vars.remove(2);
        engine.cc.stack.push(var);
        Ok(())
    } else {
        Ok(())
    }
}

// (key slice nbits - (value' key' -1) | (0))
fn dictiter(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 3)?;
    let nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
    let dict = HashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
    let result = match read_key(engine.cmd.var(2), nbits, how)? {
        (Some(key), _) => iter_reader(engine, &dict, key, how)?,
        (None, neg) if !neg ^ how.bit(MIN) => finder(engine, &dict, how)?,
        _ => None
    };
    if let Some((key, value)) = result {
        engine.cc.stack.push(value);
        let key = write_key(engine, key, how)?;
        engine.cc.stack.push(key);
        engine.cc.stack.push(boolean!(true));
    } else {
        engine.cc.stack.push(boolean!(false));
    }
    Ok(())
}

// (slice nbits - (value' key -1) | (0))
fn find(
    engine: &mut Engine,
    name: &'static str,
    how: u8,
) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 2)?;
    let nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
    let mut dict = HashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
    if let Some((key, value)) = finder(engine, &dict, how)? {
        if how.bit(DEL) {
            dict.remove_with_gas(SliceData::load_builder(key.clone())?, engine)?;
            engine.cc.stack.push(StackItem::dict(&dict));
        }
        engine.cc.stack.push(value);
        let key = write_key(engine, key, how)?;
        engine.cc.stack.push(key);
        engine.cc.stack.push(boolean!(true));
    } else {
        if how.bit(DEL) {
            engine.cc.stack.push(StackItem::dict(&dict));
        }
        engine.cc.stack.push(boolean!(false));
    }
    Ok(())
}

// (value key slice nbits - slice -1|0)
fn pfxdictset(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    let params = if how.bit(DEL) {
        3
    } else {
        4
    };
    fetch_stack(engine, params)?;
    let nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
    let mut dict = PfxHashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
    let key = engine.cmd.var(2).as_slice()?.clone();
    let key_valid = if how.bit(DEL) { // remove
        dict.remove_with_gas(key, engine)?.is_some()
    } else {
        let value = engine.cmd.var(3).as_slice()?.clone();
        if how.bit(INV) { // add
            if !dict.is_prefix(key.clone())? && dict.get(key.clone())?.is_none() {
                dict.set_with_gas(key, &value, engine)?;
                true
            } else {
                dict.get_with_gas(key, engine)?;
                false
            }
        } else if how.bit(GET) { // replace
            dict.replace_with_gas(key, &value, engine)?.is_some()
        } else { // set
            if !dict.is_prefix(key.clone())? {
                dict.set_with_gas(key, &value, engine)?;
                true
            } else {
                dict.get_prefix_leaf_with_gas(key, engine)?;
                false
            }
        }
    };
    engine.cc.stack.push(StackItem::dict(&dict));
    engine.cc.stack.push(boolean!(key_valid));
    Ok(())
}

// (prefixed slice nbits - {prefix value suffix -1} | {prefixed | 0}
fn pfxdictget(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    let get_cont = how.bit(CALLX) || how.bit(SWITCH);
    let mut inst = Instruction::new(name);
    if how.bit(CMD) {
        inst = inst.set_opts(InstructionOptions::Dictionary(13, 10))
    }
    engine.load_instruction(inst)?;
    fetch_stack(engine, if how.bit(CMD) {1} else {3})?;
    let (nbits, dict, mut key);
    if how.bit(CMD) {
        nbits = engine.cmd.length();
        dict  = PfxHashmapE::with_hashmap(nbits, engine.cmd.slice().reference_opt(0));
        key   = engine.cmd.var(0).as_slice()?.clone();
    } else {
        nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
        dict = PfxHashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
        key   = engine.cmd.var(2).as_slice()?.clone();
    }
    if let (prefix, Some(value), suffix) = dict.get_prefix_leaf_with_gas(key.clone(), engine)? {
        engine.cc.stack.push(StackItem::Slice(key.shrink_data(prefix.remaining_bits()..)));
        if get_cont {
            engine.cmd.vars.push(StackItem::continuation(
                ContinuationData::with_code(value)
            ));
        } else {
            engine.cc.stack.push(StackItem::Slice(value));
        }
        engine.cc.stack.push(StackItem::Slice(suffix));
        if how.bit(RET) {
            engine.cc.stack.push(boolean!(true));
        }
        if get_cont {
            let n = engine.cmd.var_count();
            if how.bit(SWITCH) {
                switch(engine, var!(n - 1))
            } else if how.bit(CALLX) {
                callx(engine, n - 1, false)
            } else {
                fail!("pfxdictget: {:X}", how)
            }
        } else {
            Ok(())
        }
    } else if how.bit(RET) || get_cont {
        engine.cc.stack.push(engine.cmd.pop_var()?);
        if how.bit(RET) {
            engine.cc.stack.push(boolean!(false));
        }
        Ok(())
    } else {
        err!(ExceptionCode::CellUnderflow)
    }
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
    key.as_slice::<SignedIntegerBigEndianEncoding>(nbits)
}

fn keyreader_from_uint(key: &StackItem, nbits: usize) -> Result<SliceData> {
    let key = key.as_integer()?;
    if key.is_nan() || key.is_neg() {
        return err!(ExceptionCode::IntegerOverflow);
    }
    key.as_slice::<UnsignedIntegerBigEndianEncoding>(nbits)
}

fn read_key(key: &StackItem, nbits: usize, how: u8) -> Result<(Option<SliceData>, bool)> {
    if how.bit(SLC) {
        return Ok((Some(keyreader_from_slice(key, nbits)?), false))
    } else {
        let key = key.as_integer()?;
        if how.bit(SIGN) {
            if !key.is_nan() {
                if let Ok(slice) = key.as_slice::<SignedIntegerBigEndianEncoding>(nbits) {
                    return Ok((Some(slice), key.is_neg()))
                }
            }
        } else if !key.is_nan() && !key.is_neg() {
            if let Ok(slice) = key.as_slice::<UnsignedIntegerBigEndianEncoding>(nbits) {
                return Ok((Some(slice), false))
            }
        }
    }

    Ok((None, key.as_integer()?.is_neg()))
}

fn valreader_from_slice(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    Ok(dict.get_with_gas(key, engine)?.map(StackItem::Slice))
}

fn valreader_from_ref(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    dict.get_with_gas(key, engine)?.map(try_unref_leaf).transpose()
}

fn valreader_from_refopt(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    dict.get_with_gas(key, engine)?.map(try_unref_leaf).or(Some(Ok(StackItem::None))).transpose()
}

fn valwriter_add_slice(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = engine.cmd.var_mut(3).withdraw();
    match dict.add_with_gas(key, new_val.as_slice()?, engine)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None),
    }
}

fn valwriter_add_builder(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = engine.cmd.var_mut(3).withdraw();
    match dict.add_builder_with_gas(key, new_val.as_builder()?, engine)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None),
    }
}

fn valwriter_add_ref(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = engine.cmd.var(3).as_cell()?.clone();
    match dict.addref_with_gas(key, &new_val, engine)? {
        Some(val) => Ok(Some(try_unref_leaf(val)?)),
        None => Ok(None),
    }
}

fn valwriter_add_ref_without_unref(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let new_val = engine.cmd.var(3).as_cell()?.clone();
    match dict.get_with_gas(key.clone(), engine)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => {
            dict.setref_with_gas(key, &new_val, engine)?;
            Ok(None)
        }
    }
}

fn valwriter_add_or_remove_refopt(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    let old_value = match engine.cmd.var(3).as_dict()? {
        Some(new_val) => dict.setref_with_gas(key, &new_val.clone(), engine)?,
        None => dict.remove_with_gas(key, engine)?
    };
    old_value.map(try_unref_leaf).or(Some(Ok(StackItem::None))).transpose()
}

fn valwriter_remove_slice(engine: &mut Engine, dict: &mut HashmapE, key: SliceData) -> Result<Option<StackItem>> {
    Ok(dict.remove_with_gas(key, engine)?.map(StackItem::Slice))
}

fn valwriter_remove_ref(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    dict.remove_with_gas(key, engine)?.map(try_unref_leaf).transpose()
}

fn valwriter_replace_slice(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = engine.cmd.var_mut(3).withdraw();
    match dict.replace_with_gas(key, val.as_slice()?, engine)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None)
    }
}

fn valwriter_replace_builder(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = engine.cmd.var_mut(3).withdraw();
    match dict.replace_builder_with_gas(key, val.as_builder()?, engine)? {
        Some(val) => Ok(Some(StackItem::Slice(val))),
        None => Ok(None)
    }
}

fn valwriter_replace_ref(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = engine.cmd.var(3).as_cell()?.clone();
    match dict.replaceref_with_gas(key, &val, engine)? {
        Some(val) => Some(try_unref_leaf(val)).transpose(),
        None => Ok(None)
    }
}

fn valwriter_to_slice(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = engine.cmd.var_mut(3).withdraw();
    Ok(dict.set_with_gas(key, val.as_slice()?, engine)?.map(StackItem::Slice))
}

fn valwriter_to_builder(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = engine.cmd.var_mut(3).withdraw();
    Ok(dict.set_builder_with_gas(key, val.as_builder()?, engine)?.map(StackItem::Slice))
}

fn valwriter_to_ref(
    engine: &mut Engine,
    dict: &mut HashmapE,
    key: SliceData
) -> Result<Option<StackItem>> {
    let val = engine.cmd.var(3).as_cell()?.clone();
    dict.setref_with_gas(key, &val, engine)?.map(try_unref_leaf).transpose()
}

const PREV: u8 = 0x00;
const NEXT: u8 = 0x01;
const SAME: u8 = 0x02;
// const SLC:  u8 = 0x04;
const SIGN: u8 = 0x08;
const REF:  u8 = 0x10;
// const DEL: u8 = 0x20;     // DELete key
const MAX:  u8 = 0x00; // same as PREV
const MIN:  u8 = 0x01; // same as NEXT

fn iter_reader(
    engine: &mut Engine,
    dict: &HashmapE,
    key: SliceData,
    how: u8,
) -> Result<Option<(BuilderData, StackItem)>> {
    match dict.find_leaf(key, how.bit(NEXT), how.bit(SAME), how.bit(SIGN), engine)? {
        Some((key, val)) => Ok(Some((key, StackItem::Slice(val)))),
        None => Ok(None)
    }
}

fn finder(engine: &mut Engine, dict: &HashmapE, how: u8) -> Result<Option<(BuilderData, StackItem)>> {
    let key_val = if how.bit(MIN) {
        dict.get_min(how.bit(SIGN), engine)?
    } else {
        dict.get_max(how.bit(SIGN), engine)?
    };
    match key_val {
        Some((key, val)) => if how.bit(REF) {
            Ok(Some((key, try_unref_leaf(val)?)))
        } else {
            Ok(Some((key, StackItem::Slice(val))))
        }
        None => Ok(None)
    }
}

fn write_key(engine: &mut Engine, key: BuilderData, how: u8) -> Result<StackItem> {
    if how.bit(SLC) {
        let cell = engine.finalize_cell(key)?;
        Ok(StackItem::Slice(SliceData::load_cell(cell)?))
    } else if how.bit(SIGN) {
        let encoding = SignedIntegerBigEndianEncoding::new(key.length_in_bits());
        let ret = encoding.deserialize(key.data());
        Ok(StackItem::integer(ret))
    } else {
        let encoding = UnsignedIntegerBigEndianEncoding::new(key.length_in_bits());
        let ret = encoding.deserialize(key.data());
        Ok(StackItem::integer(ret))
    }
}

// Dictionary manipulation primitives *****************************************

// (value key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictadd(engine: &mut Engine) -> Status {
    dict(engine, "DICTADD", keyreader_from_slice, INV | RET | SET, valwriter_add_slice)
}

// (builder key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictaddb(engine: &mut Engine) -> Status {
    dict(engine, "DICTADDB", keyreader_from_slice, CNV | INV | RET | SET, valwriter_add_builder)
}

// (value key slice nbits - (slice -1) | (slice y 0))
pub(super) fn execute_dictaddget(engine: &mut Engine) -> Status {
    dict(engine, "DICTADDGET", keyreader_from_slice, INV | SETGET, valwriter_add_slice)
}

// (builder key slice nbits - (slice -1) | (slice value 0))
pub(super) fn execute_dictaddgetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTADDGETB", keyreader_from_slice, CNV | INV | SETGET, valwriter_add_builder)
}

// (cell key slice nbits - (slice' -1) | (slice cell 0))
pub(super) fn execute_dictaddgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTADDGETREF", keyreader_from_slice, INV | SETGET, valwriter_add_ref)
}

// (cell key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictaddref(engine: &mut Engine) -> Status {
    dict(engine, "DICTADDREF", keyreader_from_slice, INV | RET | SET, valwriter_add_ref_without_unref)
}

// (key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictdel(engine: &mut Engine) -> Status {
    dict(engine, "DICTDEL", keyreader_from_slice, DEL | RET, valwriter_remove_slice)
}

// (key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictdelget(engine: &mut Engine) -> Status {
    dict(engine, "DICTDELGET", keyreader_from_slice,  DEL | GET | RET, valwriter_remove_slice)
}

// (key slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictdelgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTDELGETREF", keyreader_from_slice, DEL | GET | RET, valwriter_remove_ref)
}

// (key slice nbits - (value -1) | (0))
pub(super) fn execute_dictget(engine: &mut Engine) -> Status {
    dict(engine, "DICTGET", keyreader_from_slice, GET | RET, valreader_from_slice)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetnext(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTGETNEXT", NEXT | SLC)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetnexteq(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTGETNEXTEQ", NEXT | SAME | SLC)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetprev(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTGETPREV", PREV | SLC)
}

// (key slice nbits - (value key -1) | (0))
pub(super) fn execute_dictgetpreveq(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTGETPREVEQ", PREV | SAME | SLC)
}

// (key slice nbits - (cell -1) | (0))
pub(super) fn execute_dictgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTGETREF", keyreader_from_slice, GET | RET, valreader_from_ref)
}

// (value int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictiadd(engine: &mut Engine) -> Status {
    dict(engine, "DICTIADD", keyreader_from_int, INV | RET | SET, valwriter_add_slice)
}

// (builder int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictiaddb(engine: &mut Engine) -> Status {
    dict(engine, "DICTIADDB", keyreader_from_int, CNV | INV | RET | SET, valwriter_add_builder)
}

// (value int slice nbits - (slice -1) | (slice value 0))
pub(super) fn execute_dictiaddget(engine: &mut Engine) -> Status {
    dict(engine, "DICTIADDGET", keyreader_from_int, INV | SETGET, valwriter_add_slice)
}

// (builder int slice nbits - (slice' -1) | (slice y 0))
pub(super) fn execute_dictiaddgetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTIADDGETB", keyreader_from_int, CNV | INV | SETGET, valwriter_add_builder)
}

// (cell int slice nbits - (slice -1) | (slice cell 0))
pub(super) fn execute_dictiaddgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIADDGETREF", keyreader_from_int, INV | SETGET, valwriter_add_ref)
}

// (cell int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictiaddref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIADDREF", keyreader_from_int, INV | RET | SET, valwriter_add_ref_without_unref)
}

// (int slice nbits - (slice' -1) | (slice 0))
pub(super) fn execute_dictidel(engine: &mut Engine) -> Status {
    dict(engine, "DICTIDEL", keyreader_from_int, DEL | RET, valwriter_remove_slice)
}

// (int slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictidelget(engine: &mut Engine) -> Status {
    dict(engine, "DICTIDELGET", keyreader_from_int, DEL | GET | RET, valwriter_remove_slice)
}

// (int slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictidelgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIDELGETREF", keyreader_from_int, DEL | GET | RET, valwriter_remove_ref)
}

// (int slice nbits - (value -1) | (0))
pub(super) fn execute_dictiget(engine: &mut Engine) -> Status {
    dict(engine, "DICTIGET", keyreader_from_int, GET | RET, valreader_from_slice)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetnext(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTIGETNEXT", SIGN | NEXT)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetnexteq(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTIGETNEXTEQ", SIGN | NEXT | SAME)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetprev(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTIGETPREV", SIGN | PREV)
}

// (int slice nbits - (value key -1) | (0))
pub(super) fn execute_dictigetpreveq(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTIGETPREVEQ", SIGN | PREV | SAME)
}

// (int slice nbits - (cell -1) | (0))
pub(super) fn execute_dictigetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIGETREF", keyreader_from_int, GET | RET, valreader_from_ref)
}

// (slice nbits - (value int -1) | (0))
pub(super) fn execute_dictimax(engine: &mut Engine) -> Status {
    find(engine, "DICTIMAX", MAX | SIGN)
}

// (slice nbits - (cell int -1) | (0))
pub(super) fn execute_dictimaxref(engine: &mut Engine) -> Status {
    find(engine, "DICTIMAXREF", MAX | REF | SIGN)
}

// (slice nbits - (value int -1) | (0))
pub(super) fn execute_dictimin(engine: &mut Engine) -> Status {
    find(engine, "DICTIMIN", MIN | SIGN)
}

// (slice nbits - (cell int -1) | (0))
pub(super) fn execute_dictiminref(engine: &mut Engine) -> Status {
    find(engine, "DICTIMINREF", MIN | REF | SIGN)
}

// (slice nbits - (slice value int -1) | (0))
pub(super) fn execute_dictiremmax(engine: &mut Engine) -> Status {
    find(engine, "DICTIREMMAX", DEL | MAX | SIGN)
}

// (slice nbits - (slice cell int -1) | (0))
pub(super) fn execute_dictiremmaxref(engine: &mut Engine) -> Status {
    find(engine, "DICTIREMMAXREF", DEL | MAX | REF | SIGN)
}

// (slice nbits - (slice value int -1) | (0))
pub(super) fn execute_dictiremmin(engine: &mut Engine) -> Status {
    find(engine, "DICTIREMMIN", DEL | MIN | SIGN)
}

// (slice nbits - (slice cell int -1) | (0))
pub(super) fn execute_dictiremminref(engine: &mut Engine) -> Status {
    find(engine, "DICTIREMMINREF", DEL | MIN | REF | SIGN)
}

// (value int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictireplace(engine: &mut Engine) -> Status {
    dict(engine, "DICTIREPLACE", keyreader_from_int, RET | SET, valwriter_replace_slice)
}

// (builder int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictireplaceb(engine: &mut Engine) -> Status {
    dict(engine, "DICTIREPLACEB", keyreader_from_int, CNV | RET | SET, valwriter_replace_builder)
}

// (value int slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictireplaceget(engine: &mut Engine) -> Status {
    dict(engine, "DICTIREPLACEGET", keyreader_from_int,  SETGET, valwriter_replace_slice)
}

// (builder int slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictireplacegetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTIREPLACEGETB", keyreader_from_int, CNV | SETGET, valwriter_replace_builder)
}

// (cell int slice nbits - (slice' cell -1) | (slice 0))
pub(super) fn execute_dictireplacegetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIREPLACEGETREF", keyreader_from_int, SETGET, valwriter_replace_ref)
}

// (cell int slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictireplaceref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIREPLACEREF", keyreader_from_int, RET | SET, valwriter_replace_ref)
}

// (value int slice nbits - slice)
pub(super) fn execute_dictiset(engine: &mut Engine) -> Status {
    dict(engine, "DICTISET", keyreader_from_int, SET, valwriter_to_slice)
}

// (builder int slice nbits - slice)
pub(super) fn execute_dictisetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTISETB", keyreader_from_int, CNV | SET, valwriter_to_builder)
}

// (value int slice nbits - (slice y -1) | (slice 0))
pub(super) fn execute_dictisetget(engine: &mut Engine) -> Status {
    dict(engine, "DICTISETGET", keyreader_from_int, SETGET, valwriter_to_slice)
}

// (builder int slice nbits - (slice' y -1) | (slice' 0))
pub(super) fn execute_dictisetgetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTISETGETB", keyreader_from_int, CNV | SETGET, valwriter_to_builder)
}

// (cell int slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictisetgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTISETGETREF", keyreader_from_int, SETGET, valwriter_to_ref)
}

// (cell int slice nbits - slice)
pub(super) fn execute_dictisetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTISETREF", keyreader_from_int, SET, valwriter_to_ref)
}

// (slice nbits - (value key -1) | (0))
pub(super) fn execute_dictmax(engine: &mut Engine) -> Status {
    find(engine, "DICTMAX", MAX | SLC)
}

// (slice nbits - (cell key -1) | (0))
pub(super) fn execute_dictmaxref(engine: &mut Engine) -> Status {
    find(engine, "DICTMAXREF", MAX | REF | SLC)
}

// (slice nbits - (value key -1) | (0))
pub(super) fn execute_dictmin(engine: &mut Engine) -> Status {
    find(engine, "DICTMIN", MIN | SLC)
}

// (slice nbits - (cell key -1) | (0))
pub(super) fn execute_dictminref(engine: &mut Engine) -> Status {
    find(engine, "DICTMINREF", MIN | REF | SLC)
}

// (slice nbits - (slice value key -1) | (0))
pub(super) fn execute_dictremmax(engine: &mut Engine) -> Status {
    find(engine, "DICTREMMAX", DEL | MAX | SLC)
}

// (slice nbits - (slice cell key -1) | (0))
pub(super) fn execute_dictremmaxref(engine: &mut Engine) -> Status {
    find(engine, "DICTREMMAXREF", DEL | MAX | REF | SLC)
}

// (slice nbits - (slice value key -1) | (0))
pub(super) fn execute_dictremmin(engine: &mut Engine) -> Status {
    find(engine, "DICTREMMIN", DEL | MIN | SLC)
}

// (slice nbits - (slice cell key -1) | (0))
pub(super) fn execute_dictremminref(engine: &mut Engine) -> Status {
    find(engine, "DICTREMMINREF", DEL | MIN | REF | SLC)
}

// (value key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictreplace(engine: &mut Engine) -> Status {
    dict(engine, "DICTREPLACE", keyreader_from_slice, RET | SET, valwriter_replace_slice)
}

// (builder key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictreplaceb(engine: &mut Engine) -> Status {
    dict(engine, "DICTREPLACEB", keyreader_from_slice, CNV | RET | SET, valwriter_replace_builder)
}

// (value key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictreplaceget(engine: &mut Engine) -> Status {
    dict(engine, "DICTREPLACEGET", keyreader_from_slice, SETGET, valwriter_replace_slice)
}

// (builder key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictreplacegetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTREPLACEGETB", keyreader_from_slice, CNV | SETGET, valwriter_replace_builder)
}

// (cell key slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictreplacegetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTREPLACEGETREF", keyreader_from_slice, SETGET, valwriter_replace_ref)
}

// (cell key slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictreplaceref(engine: &mut Engine) -> Status {
    dict(engine, "DICTREPLACEREF", keyreader_from_slice, RET | SET, valwriter_replace_ref)
}

// (value key slice nbits - slice)
pub(super) fn execute_dictset(engine: &mut Engine) -> Status {
    dict(engine, "DICTSET", keyreader_from_slice, SET, valwriter_to_slice)
}

// (builder key slice nbits - slice)
pub(super) fn execute_dictsetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTSETB", keyreader_from_slice, CNV | SET, valwriter_to_builder)
}

// (value key slice nbits - (slice y -1) | (slice 0))
pub(super) fn execute_dictsetget(engine: &mut Engine) -> Status {
    dict(engine, "DICTSETGET", keyreader_from_slice, SETGET, valwriter_to_slice)
}

// (builder key slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictsetgetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTSETGETB", keyreader_from_slice, CNV | SETGET, valwriter_to_builder)
}

// (cell key slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictsetgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTSETGETREF", keyreader_from_slice, SETGET, valwriter_to_ref)
}

// (cell key slice nbits - slice)
pub(super) fn execute_dictsetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTSETREF", keyreader_from_slice, SET, valwriter_to_ref)
}

// (value uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictuadd(engine: &mut Engine) -> Status {
    dict(engine, "DICTUADD", keyreader_from_uint, INV | RET | SET, valwriter_add_slice)
}

// (builder uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictuaddb(engine: &mut Engine) -> Status {
    dict(engine, "DICTUADDB", keyreader_from_uint, CNV | INV | RET | SET, valwriter_add_builder)
}

// (value uint slice nbits - (slice -1) | (slice value 0))
pub(super) fn execute_dictuaddget(engine: &mut Engine) -> Status {
    dict(engine, "DICTUADDGET", keyreader_from_uint, INV | SETGET, valwriter_add_slice)
}

// (builder uint slice nbits - (slice' -1) | (slice y 0))
pub(super) fn execute_dictuaddgetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTUADDGETB", keyreader_from_uint, CNV | INV | SETGET, valwriter_add_builder)
}

// (cell uint slice nbits - (slice -1) | (slice cell 0))
pub(super) fn execute_dictuaddgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUADDGETREF", keyreader_from_uint, INV | SETGET, valwriter_add_ref)
}

// (cell uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictuaddref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUADDREF", keyreader_from_uint, INV | RET | SET, valwriter_add_ref_without_unref)
}

// (uint slice nbits - (slice' -1) | (slice 0))
pub(super) fn execute_dictudel(engine: &mut Engine) -> Status {
    dict(engine, "DICTUDEL", keyreader_from_uint, DEL | RET, valwriter_remove_slice)
}

// (uint slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictudelget(engine: &mut Engine) -> Status {
    dict(engine, "DICTUDELGET", keyreader_from_uint, DEL | GET | RET, valwriter_remove_slice)
}

// (uint slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictudelgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUDELGETREF", keyreader_from_uint, DEL | GET | RET, valwriter_remove_ref)
}

// (uint slice nbits - (value -1) | (0))
pub(super) fn execute_dictuget(engine: &mut Engine) -> Status {
    dict(engine, "DICTUGET", keyreader_from_uint, GET | RET, valreader_from_slice)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetnext(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTUGETNEXT", NEXT)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetnexteq(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTUGETNEXTEQ", NEXT | SAME)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetprev(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTUGETPREV", PREV)
}

// (uint slice nbits - (value key -1) | (0))
pub(super) fn execute_dictugetpreveq(engine: &mut Engine) -> Status {
    dictiter(engine, "DICTUGETPREVEQ", PREV | SAME)
}

// (uint slice nbits - (cell -1) | (0))
pub(super) fn execute_dictugetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUGETREF", keyreader_from_uint, GET | RET, valreader_from_ref)
}

// (slice nbits - (value uint -1) | (0))
pub(super) fn execute_dictumax(engine: &mut Engine) -> Status {
    find(engine, "DICTUMAX", MAX)
}

// (slice nbits - (cell uint -1) | (0))
pub(super) fn execute_dictumaxref(engine: &mut Engine) -> Status {
    find(engine, "DICTUMAXREF", MAX | REF)
}

// (slice nbits - (value uint -1) | (0))
pub(super) fn execute_dictumin(engine: &mut Engine) -> Status {
    find(engine, "DICTUMIN", MIN)
}

// (slice nbits - (cell uint -1) | (0))
pub(super) fn execute_dictuminref(engine: &mut Engine) -> Status {
    find(engine, "DICTUMINREF", MIN | REF)
}

// (slice nbits - (slice value uint -1) | (0))
pub(super) fn execute_dicturemmax(engine: &mut Engine) -> Status {
    find(engine, "DICTUREMMAX", DEL | MAX)
}

// (slice nbits - (slice cell uint -1) | (0))
pub(super) fn execute_dicturemmaxref(engine: &mut Engine) -> Status {
    find(engine, "DICTUREMMAXREF", DEL | MAX | REF)
}

// (slice nbits - (slice value uint -1) | (0))
pub(super) fn execute_dicturemmin(engine: &mut Engine) -> Status {
    find(engine, "DICTUREMMIN", DEL | MIN)
}

// (slice nbits - (slice cell uint -1) | (0))
pub(super) fn execute_dicturemminref(engine: &mut Engine) -> Status {
    find(engine, "DICTUREMMINREF", DEL | MIN | REF)
}

// (value uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictureplace(engine: &mut Engine) -> Status {
    dict(engine, "DICTUREPLACE", keyreader_from_uint, RET | SET, valwriter_replace_slice)
}

// (builder uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictureplaceb(engine: &mut Engine) -> Status {
    dict(engine, "DICTUREPLACEB", keyreader_from_uint, CNV | RET | SET, valwriter_replace_builder)
}

// (value uint slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictureplaceget(engine: &mut Engine) -> Status {
    dict(engine, "DICTUREPLACEGET", keyreader_from_uint,  SETGET, valwriter_replace_slice)
}

// (builder uint slice nbits - (slice value -1) | (slice 0))
pub(super) fn execute_dictureplacegetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTUREPLACEGETB", keyreader_from_uint, CNV | SETGET, valwriter_replace_builder)
}

// (cell uint slice nbits - (slice' cell -1) | (slice 0))
pub(super) fn execute_dictureplacegetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUREPLACEGETREF", keyreader_from_uint, SETGET, valwriter_replace_ref)
}

// (cell uint slice nbits - (slice -1) | (slice 0))
pub(super) fn execute_dictureplaceref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUREPLACEREF", keyreader_from_uint, RET | SET, valwriter_replace_ref)
}

// (value uint slice nbits - slice)
pub(super) fn execute_dictuset(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSET", keyreader_from_uint, SET, valwriter_to_slice)
}

// (builder uint slice nbits - slice)
pub(super) fn execute_dictusetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSETB", keyreader_from_uint, CNV | SET, valwriter_to_builder)
}

// (value uint slice nbits - (slice y -1) | (slice 0))
pub(super) fn execute_dictusetget(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSETGET", keyreader_from_uint, SETGET, valwriter_to_slice)
}

// (builder uint slice nbits - (slice' y -1) | (slice' 0))
pub(super) fn execute_dictusetgetb(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSETGETB", keyreader_from_uint, CNV | SETGET, valwriter_to_builder)
}

// (cell uint slice nbits - (slice cell -1) | (slice 0))
pub(super) fn execute_dictusetgetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSETGETREF", keyreader_from_uint, SETGET, valwriter_to_ref)
}

// (cell uint slice nbits - slice)
pub(super) fn execute_dictusetref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSETREF", keyreader_from_uint, SET, valwriter_to_ref)
}

pub(super) fn execute_dictpushconst(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("DICTPUSHCONST").set_opts(InstructionOptions::Dictionary(13, 10))
    )?;
    let slice = engine.cmd.slice();
    if slice.remaining_references() == 0 {
        return err!(ExceptionCode::InvalidOpcode);
    } else {
        engine.cc.stack.push(StackItem::Cell(slice.reference(0)?));
    }
    let key = engine.cmd.length();
    engine.cc.stack.push(int!(key));
    Ok(())
}

// (int slice nbits - )
pub(super) fn execute_dictigetjmp(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTIGETJMP", keyreader_from_int, SWITCH)
}

// (int slice nbits - int or nothing)
pub(super) fn execute_dictigetjmpz(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTIGETJMPZ", keyreader_from_int, SWITCH | STAY)
}

// (uint slice nbits - )
pub(super) fn execute_dictugetjmp(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTUGETJMP", keyreader_from_uint, SWITCH)
}

// (uint slice nbits - int or nothing)
pub(super) fn execute_dictugetjmpz(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTUGETJMPZ", keyreader_from_uint, SWITCH | STAY)
}

// (int slice nbits - )
pub(super) fn execute_dictigetexec(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTIGETEXEC", keyreader_from_int, CALLX)
}

// (int slice nbits - int or nothing)
pub(super) fn execute_dictigetexecz(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTIGETEXECZ", keyreader_from_int, CALLX | STAY)
}

// (uint slice nbits - )
pub(super) fn execute_dictugetexec(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTUGETEXEC", keyreader_from_uint, CALLX)
}

// (uint slice nbits - int or nothing)
pub(super) fn execute_dictugetexecz(engine: &mut Engine) -> Status {
    dictcont(engine, "DICTUGETEXECZ", keyreader_from_uint, CALLX | STAY)
}

// (value key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictset(engine: &mut Engine) -> Status {
    pfxdictset(engine, "PFXDICTSET", 0)
}

// (value key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictreplace(engine: &mut Engine) -> Status {
    pfxdictset(engine, "PFXDICTREPLACE", GET)
}

// (value key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictadd(engine: &mut Engine) -> Status {
    pfxdictset(engine, "PFXDICTADD", INV | GET)
}

// (key slice nbits - slice -1|0)
pub(super) fn execute_pfxdictdel(engine: &mut Engine) -> Status {
    pfxdictset(engine, "PFXDICTDEL", DEL)
}

// (prefixed slice nbits - {prefix value suffix -1} | {prefixed | 0}
pub(super) fn execute_pfxdictgetq(engine: &mut Engine) -> Status {
    pfxdictget(engine, "PFXDICTGETQ", RET)
}

// (prefixed slice nbits - prefix value suffix -1}
pub(super) fn execute_pfxdictget(engine: &mut Engine) -> Status {
    pfxdictget(engine, "PFXDICTGET", 0)
}

// (s' s n - (s'' s''') | (s')))
pub(super) fn execute_pfxdictgetjmp(engine: &mut Engine) -> Status {
    pfxdictget(engine, "PFXDICTGETJMP", SWITCH)
}

// (s' s n - (s'' s'''))
pub(super) fn execute_pfxdictgetexec(engine: &mut Engine) -> Status {
    pfxdictget(engine, "PFXDICTGETEXEC", CALLX)
}

// (s' - (s'' s''') | (s')))
pub(super) fn execute_pfxdictswitch(engine: &mut Engine) -> Status {
    pfxdictget(engine, "PFXDICTSWITCH", CMD | SWITCH)
}

const QUIET: u8 = 0x01; // quiet variant
const DICT:  u8 = 0x02; // dictionary
const SLC:   u8 = 0x04; // slice
const REST:  u8 = 0x08; // remainder

fn load_dict(engine: &mut Engine, name: &'static str, how: u8) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 1)?;
    let mut slice = engine.cmd.var(0).as_slice()?.clone();
    let empty = if let Some(dict) = slice.get_dictionary_opt() {
        if how.bit(SLC) {
            engine.cc.stack.push(StackItem::Slice(dict));
        } else if how.bit(DICT) {
            engine.cc.stack.push(if dict.is_empty_root() {
                StackItem::None
            } else {
                StackItem::Cell(dict.reference(0)?)
            });
        }
        false
    } else {
        slice = engine.cmd.var(0).as_slice()?.clone();
        true
    };
    if how.bit(REST) {
        engine.cc.stack.push(StackItem::Slice(slice));
    }
    if how.bit(QUIET) {
        engine.cc.stack.push(boolean!(!empty));
    } else if empty {
        return err!(ExceptionCode::CellUnderflow)
    }
    Ok(())
}

// (slice - slice)
pub(super) fn execute_skipdict(engine: &mut Engine) -> Status {
    load_dict(engine, "SKIPDICT", REST)
}

// (slice - D slice)
pub(super) fn execute_lddict(engine: &mut Engine) -> Status {
    load_dict(engine, "LDDICT", REST | DICT)
}

// (slice - D)
pub(super) fn execute_plddict(engine: &mut Engine) -> Status {
    load_dict(engine, "PLDDICT", DICT)
}

// (slice - slice slice)
pub(super) fn execute_lddicts(engine: &mut Engine) -> Status {
    load_dict(engine, "LDDICTS", REST | SLC)
}

// (slice - slice)
pub(super) fn execute_plddicts(engine: &mut Engine) -> Status {
    load_dict(engine, "PLDDICTS", SLC)
}

// (slice - (D slice -1) | (slice 0))
pub(super) fn execute_lddictq(engine: &mut Engine) -> Status {
    load_dict(engine, "LDDICTQ", REST | DICT | QUIET)
}

// (slice - (D -1) | (0))
pub(super) fn execute_plddictq(engine: &mut Engine) -> Status {
    load_dict(engine, "PLDDICTQ", DICT | QUIET)
}

type IntoSubtree = fn(&mut HashmapE, prefix: &SliceData, &mut dyn GasConsumer) -> Result<()>;
fn subdict(engine: &mut Engine, name: &'static str, keyreader: KeyReader, into: IntoSubtree) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_stack(engine, 4)?;
    let nbits = engine.cmd.var(0).as_integer()?.into(0..=1023)?;
    let mut dict = HashmapE::with_hashmap(nbits, engine.cmd.var(1).as_dict()?.cloned());
    let lbits = engine.cmd.var(2).as_integer()?.into(0..=nbits)?;
    let key = keyreader(engine.cmd.var(3), lbits)?;
    into(&mut dict, &key, engine)?;
    engine.cc.stack.push(StackItem::dict(&dict));
    Ok(())
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictget(engine: &mut Engine) -> Status {
    subdict(engine, "SUBDICTGET", keyreader_from_slice, HashmapSubtree::into_subtree_with_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictiget(engine: &mut Engine) -> Status {
    subdict(engine, "SUBDICTIGET", keyreader_from_int, HashmapSubtree::into_subtree_with_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictuget(engine: &mut Engine) -> Status {
    subdict(engine, "SUBDICTUGET", keyreader_from_uint, HashmapSubtree::into_subtree_with_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictrpget(engine: &mut Engine) -> Status {
    subdict(engine, "SUBDICTRPGET", keyreader_from_slice, HashmapSubtree::into_subtree_without_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdictirpget(engine: &mut Engine) -> Status {
    subdict(engine, "SUBDICTIRPGET", keyreader_from_int, HashmapSubtree::into_subtree_without_prefix)
}

// prefix lbits dict nbits - dict'
pub(super) fn execute_subdicturpget(engine: &mut Engine) -> Status {
    subdict(engine, "SUBDICTURPGET", keyreader_from_uint, HashmapSubtree::into_subtree_without_prefix)
}
pub(super) fn execute_dictgetoptref(engine: &mut Engine) -> Status {
    dict(engine, "DICTGETOPTREF", keyreader_from_slice, GET, valreader_from_refopt)
}

pub(super) fn execute_dictigetoptref(engine: &mut Engine) -> Status {
    dict(engine, "DICTIGETOPTREF", keyreader_from_int, GET, valreader_from_refopt)
}

pub(super) fn execute_dictugetoptref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUGETOPTREF", keyreader_from_uint, GET, valreader_from_refopt)
}

pub(super) fn execute_dictsetgetoptref(engine: &mut Engine) -> Status {
    dict(engine, "DICTSETGETOPTREF", keyreader_from_slice, SET | GET, valwriter_add_or_remove_refopt)
}

pub(super) fn execute_dictisetgetoptref(engine: &mut Engine) -> Status {
    dict(engine, "DICTISETGETOPTREF", keyreader_from_int, SET | GET, valwriter_add_or_remove_refopt)
}

pub(super) fn execute_dictusetgetoptref(engine: &mut Engine) -> Status {
    dict(engine, "DICTUSETGETOPTREF", keyreader_from_uint, SET | GET, valwriter_add_or_remove_refopt)
}
