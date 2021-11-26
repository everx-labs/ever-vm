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
    executor::{engine::{Engine, storage::fetch_stack}, types::Instruction},
    stack::{StackItem, integer::IntegerData},
    types::{Exception, Status}
};
use ton_types::{BuilderData, error, IBitstring, types::ExceptionCode};
use std::sync::Arc;

// slice - uint slice'
fn load_var(engine: &mut Engine, name: &'static str, max_bytes: u8, sign: bool) -> Status {
    engine.load_instruction(Instruction::new(name))?;
    fetch_stack(engine, 1)?;
    let mut slice = engine.cmd.var(0).as_slice()?.clone();
    let len = 8 - (max_bytes - 1).leading_zeros() as usize;
    let bytes = slice.get_next_int(len)? as usize;
    let vec = slice.get_next_bytes(bytes)?;
    let value = match sign {
        true => num::BigInt::from_signed_bytes_be(&vec),
        false => num::BigInt::from_bytes_be(num::bigint::Sign::Plus, &vec)
    };
    engine.cc.stack.push(int!(value));
    engine.cc.stack.push(StackItem::Slice(slice));
    Ok(())
}

pub(super) fn execute_ldgrams(engine: &mut Engine) -> Status {
    load_var(engine, "LDGRAMS", 16, false)
}
pub(super) fn execute_ldvarint16(engine: &mut Engine) -> Status {
    load_var(engine, "LDVARINT16", 16, true)
}
pub(super) fn execute_ldvaruint32(engine: &mut Engine) -> Status {
    load_var(engine, "LDVARUINT32", 32, false)
}
pub(super) fn execute_ldvarint32(engine: &mut Engine) -> Status {
    load_var(engine, "LDVARINT32", 32, true)
}

// builder uint - builder'
fn store_var(engine: &mut Engine, name: &'static str, max_bits: usize, sign: bool) -> Status {
    engine.load_instruction(Instruction::new(name))?;
    fetch_stack(engine, 2)?;
    let x = engine.cmd.var(0).as_integer()?;
    let b = engine.cmd.var(1).as_builder()?;
    let (bits, vec) = match sign {
        false => match x.is_neg() {
            true => return err!(ExceptionCode::RangeCheckError),
            false => (x.ubitsize(), x.take_value_of(|x| Some(x.to_bytes_be().1))?)
        }
        true => (x.bitsize(), x.take_value_of(|x| Some(x.to_signed_bytes_be()))?)
    };
    if bits > max_bits {
        return err!(ExceptionCode::RangeCheckError)
    }
    let len = 16 - (max_bits as u16 / 8).leading_zeros();
    match max_bits {
        120 => debug_assert_eq!(len, 4),
        248 => debug_assert_eq!(len, 5),
        _ => debug_assert_eq!(len, 0)
    }
    let mut x = BuilderData::new();
    let bytes = if bits != 0 {
        vec.len()
    } else {
        0
    };
    x.append_bits(bytes, len as usize)?;
    x.append_raw(&vec, bytes * 8)?;
    if b.can_append(&x) {
        let mut b = engine.cmd.var_mut(1).as_builder_mut()?;
        b.append_builder(&x).expect("free space was checked before");
        engine.cc.stack.push_builder(b);
        Ok(())
    } else {
        err!(ExceptionCode::CellOverflow)
    }
}

pub(super) fn execute_stgrams(engine: &mut Engine) -> Status {
    store_var(engine, "STGRAMS", 120, false)
}

pub(super) fn execute_stvarint16(engine: &mut Engine) -> Status {
    store_var(engine, "STVARINT16", 120, true)
}

pub(super) fn execute_stvaruint32(engine: &mut Engine) -> Status {
    store_var(engine, "STVARUINT32", 248, false)
}

pub(super) fn execute_stvarint32(engine: &mut Engine) -> Status {
    store_var(engine, "STVARINT32", 248, true)
}
