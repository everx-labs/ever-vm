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

use crate::{error::TvmError, stack::{Stack, StackItem}, types::{Exception, ResultOpt}};
use std::{collections::{HashMap, hash_map::IterMut}, fmt};
use ton_types::{error, types::ExceptionCode};

#[derive(Clone, Debug)]
pub struct SaveList {
    storage: HashMap<usize, StackItem>,
}

impl SaveList {
    pub fn new() -> SaveList {
        SaveList {
            storage: HashMap::new(),
        }
    }
    pub fn can_put(index: usize, value: &StackItem) -> bool {
        match index {
            0..=3 => value.as_continuation().is_ok(),
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
}

impl PartialEq for SaveList {
    fn eq(&self, savelist: &SaveList) -> bool {
        if self.storage.len() != savelist.storage.len() {
            return false;
        }
        for k in self.storage.keys() {
            if !savelist.storage.contains_key(k) {
                return false;
            }
            if !Stack::eq_item(self.storage.get(k).unwrap(), savelist.storage.get(k).unwrap()) {
                return false;
            }
        }
        return true;
    }
}

impl fmt::Display for SaveList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "--- Control registers ------------------\n")?;
        for i in 0..16 {
            if self.storage.contains_key(&i) {
                write!(f, "{}: {}\n", i, self.get(i).unwrap())?
            }
        }        
        write!(f, "{:-<40}\n", "")
    }
}
