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

use crate::{error::TvmError, executor::gas::gas_state::Gas, stack::{Stack, StackItem}, types::{Exception, ResultOpt}};
use std::{collections::{HashMap, hash_map::IterMut}, fmt};
use ton_types::{BuilderData, HashmapE, HashmapType, IBitstring, Result, SliceData, error, types::ExceptionCode};

#[derive(Clone, Debug)]
pub struct SaveList {
    storage: HashMap<usize, StackItem>,
}

impl Default for SaveList {
    fn default() -> Self {
        SaveList::new()
    }
}

impl SaveList {
    pub fn new() -> SaveList {
        SaveList {
            storage: HashMap::new(),
        }
    }
    pub fn can_put(index: usize, value: &StackItem) -> bool {
        match index {
            0 | 1 | 3 => value.as_continuation().is_ok(),
            2 => value.as_continuation().is_ok() || value.is_null(),
            4 | 5 => value.as_cell().is_ok(),
            7 => value.as_tuple().is_ok(),
            8..=15 => true,
            _ => false
        }
    }
    pub fn clear(&mut self) {
        self.storage.clear()
    }
    pub fn get(&self, index: usize) -> Option<&StackItem> {
        self.storage.get(&index)
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut StackItem> {
        self.storage.get_mut(&index)
    }
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }
    pub fn iter_mut(&mut self) -> IterMut<usize, StackItem> {
        self.storage.iter_mut()
    }
    pub fn put(&mut self, index: usize, value: &mut StackItem) -> ResultOpt<StackItem> {
        if !SaveList::can_put(index, value) {
            err!(ExceptionCode::TypeCheckError)
        } else if value.is_null() {
            Ok(self.storage.remove(&index))
        } else {
            Ok(self.storage.insert(index, value.withdraw()))
        }
    }
    pub fn remove(&mut self, index: usize) -> Option<StackItem> {
        self.storage.remove(&index)
    }
    pub fn len(&self) -> usize {
        self.storage.keys().len()
    }
    pub fn serialize(&self) -> Result<(BuilderData, i64)> {
        let mut gas = 0;
        let mut dict = HashmapE::with_bit_len(4);
        for (index, item) in self.storage.iter() {
            let mut builder = BuilderData::new();
            builder.append_bits(*index, 4)?;
            let key = builder.into_cell()?.into();
            let (value, gas2) = item.serialize()?;
            gas += gas2;
            dict.set_builder(key, &value)?;
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
                let mut hashmap = HashMap::new();
                for item in dict.iter() {
                    let (key, value) = item?;
                    let key = SliceData::from(key.into_cell()?).get_next_int(4)? as usize;
                    let (value, gas2) = StackItem::deserialize(&mut value.clone())?;
                    gas += gas2;
                    hashmap.insert(key, value);
                }
                Ok((Self { storage: hashmap }, gas))
            }
        }
    }
}

impl PartialEq for SaveList {
    fn eq(&self, savelist: &SaveList) -> bool {
        if self.storage.len() != savelist.storage.len() {
            return false;
        }
        for k in self.storage.keys() {
            if !savelist.storage.contains_key(k) {
                return false
            }
            if !Stack::eq_item(self.storage.get(k).unwrap(), savelist.storage.get(k).unwrap()) {
                return false
            }
        }
        true
    }
}

impl fmt::Display for SaveList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "--- Control registers ------------------")?;
        for i in 0..16 {
            if self.storage.contains_key(&i) {
                writeln!(f, "{}: {}", i, self.get(i).unwrap())?
            }
        }
        writeln!(f, "{:-<40}", "")
    }
}
