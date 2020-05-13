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

#[macro_use]
mod microcode;
#[macro_use]
mod engine;
mod blockchain;
mod serialization;
mod deserialization;
mod continuation;
mod crypto;
mod currency;
mod dictionary;
mod exceptions;
mod globals;
mod math;
mod slice_comparison;
mod stack;
mod tuple;
mod types;
pub mod gas;
mod dump;
mod null;
mod config;
mod rand;

pub use engine::Engine;
use ton_types::{BuilderData, Cell, IBitstring, Result};


pub trait Mask {
    fn bit(&self, bits: Self) -> bool;
    fn mask(&self, mask: Self) -> Self;
    fn any(&self, bits: Self) -> bool;
    fn non(&self, bits: Self) -> bool;
}
impl Mask for u8 {
    fn bit(&self, bits: Self) -> bool {
        (self & bits) == bits
    }
    fn mask(&self, mask: Self) -> u8 {
        self & mask
    }
    fn any(&self, bits: Self) -> bool {
        (self & bits) != 0
    }
    fn non(&self, bits: Self) -> bool {
        (self & bits) == 0
    }
}

fn serialize_grams(grams: u128) -> Result<BuilderData> {
    let bytes = 16 - grams.leading_zeros() as usize / 8;
    let mut builder = BuilderData::with_raw(vec!((bytes as u8) << 4), 4)?;
    builder.append_raw(&grams.to_be_bytes()[16 - bytes..], bytes * 8)?;
    Ok(builder)
}

pub fn serialize_currency_collection(grams: u128, other: Option<Cell>) -> Result<BuilderData> {
    let mut builder = serialize_grams(grams)?;
    if let Some(cell) = other {
        builder.append_bit_one()?;
        builder.checked_append_reference(cell)?;
    } else {
        builder.append_bit_zero()?;
    }
    Ok(builder)
}
