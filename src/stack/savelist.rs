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

use crate::{error::TvmError, executor::gas::gas_state::Gas, stack::StackItem, types::{Exception, ResultOpt}};
use std::fmt;
use ton_types::{BuilderData, HashmapE, HashmapType, IBitstring, Result, SliceData, error, types::ExceptionCode};

#[derive(Clone, Debug, PartialEq)]
pub struct SaveList {
    storage: [Option<StackItem>; Self::NUMREGS],
}

impl Default for SaveList {
    fn default() -> Self {
        Self::new()
    }
}

impl SaveList {
    const NUMREGS: usize = 7;
    pub const REGS: [usize; Self::NUMREGS] = [0, 1, 2, 3, 4, 5, 7];
    const fn adjust(index: usize) -> usize {
        if index == 7 { 6 } else { index }
    }

    pub fn new() -> Self {
        Self {
            storage: Default::default()
        }
    }
    pub fn can_put(index: usize, value: &StackItem) -> bool {
        match index {
            0 | 1 | 3 => value.as_continuation().is_ok(),
            2 => value.as_continuation().is_ok() || value.is_null(),
            4 | 5 => value.as_cell().is_ok(),
            7 => value.as_tuple().is_ok(),
            _ => false
        }
    }
    pub fn get(&self, index: usize) -> Option<&StackItem> {
        self.storage[Self::adjust(index)].as_ref()
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut StackItem> {
        self.storage[Self::adjust(index)].as_mut()
    }
    pub fn is_empty(&self) -> bool {
        for v in &self.storage {
            if v.is_some() {
                return false;
            }
        }
        true
    }
    pub fn put(&mut self, index: usize, value: &mut StackItem) -> ResultOpt<StackItem> {
        if !Self::can_put(index, value) {
            err!(ExceptionCode::TypeCheckError)
        } else {
            self.put_opt(index, value)
        }
    }
    pub fn put_opt(&mut self, index: usize, value: &mut StackItem) -> ResultOpt<StackItem> {
        debug_assert!(Self::can_put(index, value));
        debug_assert!(!value.is_null());
        Ok(std::mem::replace(&mut self.storage[Self::adjust(index)], Some(value.withdraw())))
    }
    pub fn apply(&mut self, other: &mut Self) {
        for index in 0..Self::NUMREGS {
            if other.storage[index].is_some() {
                self.storage[index] = std::mem::take(&mut other.storage[index]);
            }
        }
    }
    pub fn remove(&mut self, index: usize) -> Option<StackItem> {
        std::mem::replace(&mut self.storage[Self::adjust(index)], None)
    }
    pub fn serialize(&self) -> Result<(BuilderData, i64)> {
        let mut gas = 0;
        let mut dict = HashmapE::with_bit_len(4);
        for index in 0..Self::NUMREGS {
            if let Some(ref item) = self.storage[index] {
                let mut builder = BuilderData::new();
                builder.append_bits(if index == 6 { 7 } else { index }, 4)?;
                let key = builder.into_cell()?.into();
                let (value, gas2) = item.serialize()?;
                gas += gas2;
                dict.set_builder(key, &value)?;
            }
        }
        let mut builder = BuilderData::new();
        match dict.data() {
            Some(cell) => {
                builder.append_bit_one()?;
                builder.append_reference_cell(cell.clone());
                gas += Gas::finalize_price();
            }
            None => {
                builder.append_bit_zero()?;
            }
        }
        Ok((builder, gas))
    }
    pub fn deserialize(slice: &mut SliceData) -> Result<(Self, i64)> {
        let mut gas = 0;
        match slice.get_next_bit()? {
            false => Ok((Self::new(), gas)),
            true => {
                let dict = HashmapE::with_hashmap(4, slice.checked_drain_reference().ok());
                gas += Gas::load_cell_price(true);
                let mut savelist = SaveList::new();
                for item in dict.iter() {
                    let (key, value) = item?;
                    let key = SliceData::from(key.into_cell()?).get_next_int(4)? as usize;
                    let (mut value, gas2) = StackItem::deserialize(&mut value.clone())?;
                    gas += gas2;
                    savelist.put(key, &mut value)?;
                }
                Ok((savelist, gas))
            }
        }
    }
}

impl fmt::Display for SaveList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "--- Control registers ------------------")?;
        for i in 0..Self::NUMREGS {
            if let Some(item) = &self.storage[i] {
                writeln!(f, "{}: {}", i, item)?
            }
        }
        writeln!(f, "{:-<40}", "")
    }
}
