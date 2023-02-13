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
    executor::gas::gas_state::Gas,
    stack::{SliceData, Stack, StackItem, savelist::SaveList},
    types::{Exception, ResultOpt},
};
use std::{fmt, mem};
use ton_types::{BuilderData, Cell, IBitstring, Result, error, ExceptionCode, GasConsumer, HashmapE, HashmapType};
use super::{slice_serialize, slice_deserialize, items_deserialize, items_serialize, prepare_cont_serialize_vars, DeserializeItem};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContinuationType {
    AgainLoopBody(SliceData),
    TryCatch,
    Ordinary,
    PushInt(i32),
    Quit(i32),
    RepeatLoopBody(SliceData, isize),
    UntilLoopCondition(SliceData),
    WhileLoopCondition(SliceData, SliceData),
}

impl Default for ContinuationType {
    fn default() -> Self {
        Self::Ordinary
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuationData {
    code: SliceData,
    pub nargs: isize,
    pub savelist: SaveList,
    pub stack: Stack,
    pub type_of: ContinuationType,
}

impl ContinuationData {
    pub fn new_empty() -> Self {
        Self {
            code: SliceData::default(),
            nargs: -1,
            savelist: SaveList::new(),
            stack: Stack::new(),
            type_of: ContinuationType::Ordinary,
        }
    }

    pub fn move_without_stack(cont: &mut ContinuationData, body: SliceData) -> Self {
        debug_assert!(cont.code.is_empty());
        debug_assert!(cont.nargs < 0);
        debug_assert!(cont.savelist.is_empty());
        Self {
            code: mem::replace(&mut cont.code, body),
            nargs: -1,
            savelist: Default::default(),
            stack: Stack::new(),
            type_of: mem::take(&mut cont.type_of)
        }
    }

    pub fn copy_without_stack(&self) -> Self {
        Self {
            code: self.code.clone(),
            nargs: self.nargs,
            savelist: self.savelist.clone(),
            stack: Stack::new(),
            type_of: self.type_of.clone(),
        }
    }

    pub fn code(&self) -> &SliceData {
        &self.code
    }

    pub fn code_mut(&mut self) -> &mut SliceData {
        &mut self.code
    }

    pub fn can_put_to_savelist_once(&self, i: usize) -> bool {
        self.savelist.get(i).is_none()
    }

    pub fn move_to_end(&mut self) {
        self.code = SliceData::default()
    }

    pub fn put_to_savelist(&mut self, i: usize, val: &mut StackItem) -> ResultOpt<StackItem> {
        self.savelist.put(i, val)
    }

    pub fn remove_from_savelist(&mut self, i: usize) -> Option<StackItem> {
        self.savelist.remove(i)
    }

    pub fn with_code(code: SliceData) -> Self {
        ContinuationData {
           code,
           nargs: -1,
           savelist: SaveList::new(),
           stack: Stack::new(),
           type_of: ContinuationType::Ordinary,
        }
    }

    pub fn with_type(type_of: ContinuationType) -> Self {
        ContinuationData {
           code: SliceData::default(),
           nargs: -1,
           savelist: SaveList::new(),
           stack: Stack::new(),
           type_of,
        }
    }

    pub fn withdraw(&mut self) -> Self {
        mem::replace(self, ContinuationData::new_empty())
    }

    pub fn undrain_reference(&mut self) {
        self.code.undrain_reference();
    }

    pub fn drain_reference(&mut self) -> Result<Cell> {
        self.code.checked_drain_reference()
            .map_err(|_| exception!(ExceptionCode::InvalidOpcode))
    }

    pub(crate) fn serialize(&self, gas_consumer: &mut dyn GasConsumer) -> Result<BuilderData> {
        let mut items = Vec::new();
        prepare_cont_serialize_vars(self, BuilderData::default(), &mut items, false);
        items_serialize(items, gas_consumer)
    }

    pub(super) fn serialize_internal(&self, stack: BuilderData, savelist: HashmapE, gas_consumer: &mut dyn GasConsumer) -> Result<BuilderData> {
        let mut builder = BuilderData::new();
        match &self.type_of {
            ContinuationType::AgainLoopBody(body) => {
                builder.append_bits(0xd, 4)?;
                let child_cell = gas_consumer.finalize_cell(slice_serialize(body)?)?;
                builder.checked_append_reference(child_cell)?;
            }
            ContinuationType::TryCatch => {
                builder.append_bits(0x9, 4)?;
            }
            ContinuationType::Ordinary => {
                builder.append_bits(0x0, 2)?;
            }
            ContinuationType::PushInt(value) => {
                builder.append_bits(0xf, 4)?;
                builder.append_bits(*value as usize, 32)?;
            }
            ContinuationType::Quit(exit_code) => {
                builder.append_bits(0x8, 4)?;
                builder.append_bits(*exit_code as usize, 32)?;
            }
            ContinuationType::RepeatLoopBody(body, counter) => {
                builder.append_bits(0xe, 4)?;
                let child_cell = gas_consumer.finalize_cell(slice_serialize(body)?)?;
                builder.checked_append_reference(child_cell)?;
                builder.append_bits(*counter as usize, 32)?;
            }
            ContinuationType::UntilLoopCondition(body) => {
                builder.append_bits(0xa, 4)?;
                let child_cell = gas_consumer.finalize_cell(slice_serialize(body)?)?;
                builder.checked_append_reference(child_cell)?;
            }
            ContinuationType::WhileLoopCondition(body, cond) => {
                builder.append_bits(0xc, 4)?;
                let mut child_cell = slice_serialize(cond)?;
                child_cell.append_builder(&slice_serialize(body)?)?;
                let child_cell = gas_consumer.finalize_cell(child_cell)?;
                builder.checked_append_reference(child_cell)?;
            }
        }
        // can be one reference

        builder.append_bits(self.nargs as usize, 22)?;
        if self.stack.is_empty() {
            builder.append_bit_zero()?;
        } else {
            builder.append_bit_one()?;
            builder.append_bits(self.stack.depth(), 24)?;
            let cell = gas_consumer.finalize_cell(stack)?;
            builder.checked_append_reference(cell)?; // second ref
        }
        savelist.write_hashmap_data(&mut builder)?; // third ref
        builder.append_bits(0, 16)?; // codepage
        builder.append_builder(&slice_serialize(self.code())?)?; // last ref
        Ok(builder)
    }

    pub(crate) fn deserialize(slice: &mut SliceData, gas_consumer: &mut dyn GasConsumer) -> Result<Self> {
        let mut list = Vec::new();
        ContinuationData::deserialize_internal(&mut list, slice, gas_consumer)?;
        Ok(std::mem::replace(
            items_deserialize(list, gas_consumer)?.remove(0).as_continuation_mut()?,
            ContinuationData::new_empty()
        ))
    }

    pub(crate) fn deserialize_internal(
        list: &mut Vec<DeserializeItem>,
        slice: &mut SliceData,
        gas_consumer: &mut dyn GasConsumer
    ) -> Result<()> {
        let mut new_list = Vec::new();
        let type_of = match slice.get_next_int(2)? {
            0 => ContinuationType::Ordinary,
            1 => ContinuationType::TryCatch,
            2 => {
                match slice.get_next_int(2)? {
                    0 => {
                        let exit_code = slice.get_next_int(32)? as i32;
                        ContinuationType::Quit(exit_code)
                    }
                    2 => {
                        let mut child_slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
                        let body = slice_deserialize(&mut child_slice)?;
                        ContinuationType::UntilLoopCondition(body)
                    }
                    typ => return err!(ExceptionCode::UnknownError, "wrong continuation type 10{:2b}", typ)
                }
            }
            3 => {
                match slice.get_next_int(2)? {
                    0 => {
                        let mut child_slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
                        let cond = slice_deserialize(&mut child_slice)?;
                        let body = slice_deserialize(&mut child_slice)?;
                        ContinuationType::WhileLoopCondition(body, cond)
                    }
                    1 => {
                        let mut child_slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
                        let body = slice_deserialize(&mut child_slice)?;
                        ContinuationType::AgainLoopBody(body)
                    }
                    2 => {
                        let mut child_slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
                        let code = slice_deserialize(&mut child_slice)?;
                        let counter = slice.get_next_int(32)? as isize;
                        ContinuationType::RepeatLoopBody(code, counter)
                    }
                    3 => {
                        let value = slice.get_next_int(32)? as i32;
                        ContinuationType::PushInt(value)
                    }
                    typ => return err!(ExceptionCode::UnknownError, "wrong continuation type 10{:2b}", typ)
                }
            }
            typ => return err!(ExceptionCode::UnknownError, "wrong continuation type {:2b}", typ)
        };

        let nargs = match slice.get_next_int(22)? as isize {
            0x3fffff => -1, // 4194303
            x => x
        };
        let stack = if slice.get_next_bit()? {
            let length = slice.get_next_int(24)? as usize;
            let stack_slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
            DeserializeItem::Items(length, stack_slice)
        } else {
            DeserializeItem::Items(0, SliceData::default())
        };
        // savelist
        if slice.get_next_bit()? {
            let dict = HashmapE::with_hashmap(4, Some(slice.checked_drain_reference()?));
            dict.iterate_slices(|mut key, value| {
                let key = key.get_next_int(4)? as usize;
                new_list.push(DeserializeItem::SaveListItem(key));
                new_list.push(DeserializeItem::Items(1, value));
                Ok(true)
            })?;
        }
        new_list.push(DeserializeItem::SaveList);
        let _codepage = slice.get_next_int(16)?; // codepage
        let code = slice_deserialize(slice)?;
        let cont = ContinuationData {
            code,
            nargs,
            savelist: SaveList::default(),
            stack: Stack::default(),
            type_of,
        };
        list.push(DeserializeItem::Cont(cont));
        list.append(&mut new_list);
        list.push(stack);
        Ok(())
    }
}

impl ContinuationData {
    pub fn serialize_old(&self) -> Result<(BuilderData, i64)> {
        let mut gas = 0;
        let mut builder = BuilderData::new();
        match &self.type_of {
            ContinuationType::AgainLoopBody(body) => {
                builder.append_bits(0xd, 4)?;
                builder.checked_append_reference(slice_serialize(body)?.into_cell()?)?;
            }
            ContinuationType::TryCatch => {
                builder.append_bits(0x9, 4)?;
            }
            ContinuationType::Ordinary => {
                builder.append_bits(0x0, 2)?;
            }
            ContinuationType::PushInt(value) => {
                builder.append_bits(0xf, 4)?;
                builder.append_bits(*value as usize, 32)?;
            }
            ContinuationType::Quit(exit_code) => {
                builder.append_bits(0x8, 4)?;
                builder.append_bits(*exit_code as usize, 32)?;
            }
            ContinuationType::RepeatLoopBody(code, counter) => {
                builder.append_bits(0xe, 4)?;
                builder.checked_append_reference(slice_serialize(code)?.into_cell()?)?;
                builder.append_bits(*counter as usize, 32)?;
            }
            ContinuationType::UntilLoopCondition(body) => {
                builder.append_bits(0xa, 4)?;
                builder.checked_append_reference(slice_serialize(body)?.into_cell()?)?;
            }
            ContinuationType::WhileLoopCondition(body, cond) => {
                builder.append_bits(0xc, 4)?;
                builder.checked_append_reference(slice_serialize(cond)?.into_cell()?)?;
                builder.checked_append_reference(slice_serialize(body)?.into_cell()?)?;
            }
        }

        let mut stack = BuilderData::new();
        stack.append_bits(self.stack.depth(), 24)?;
        let mut stack_list = BuilderData::new();
        for item in self.stack.iter().rev() {
            let mut cons = BuilderData::new();
            let (serialized, gas2) = item.serialize_old()?;
            gas += gas2;
            cons.append_builder(&serialized)?;
            cons.checked_append_reference(stack_list.into_cell()?)?;
            gas += Gas::finalize_price();
            stack_list = cons;
        }
        stack.append_builder(&stack_list)?;

        builder.append_bits(self.nargs as usize, 22)?;
        if self.stack.depth() == 0 {
            builder.append_bit_zero()?;
        } else {
            builder.append_bit_one()?;
            builder.append_builder(&stack)?;
        }
        let (serialized, gas2) = self.savelist.serialize_old()?;
        gas += gas2;
        builder.append_builder(&serialized)?;
        builder.append_bits(0, 16)?; // codepage
        builder.append_builder(&slice_serialize(&self.code)?)?;
        Ok((builder, gas))
    }

    pub fn deserialize_old(slice: &mut SliceData) -> Result<(Self, i64)> {
        let mut gas = 0;
        let cont_type = match slice.get_next_int(2)? {
            0 => Ok(ContinuationType::Ordinary),
            1 => Ok(ContinuationType::TryCatch),
            2 => {
                match slice.get_next_int(2)? {
                    0 => {
                        let exit_code = slice.get_next_int(32)? as i32;
                        Ok(ContinuationType::Quit(exit_code))
                    }
                    2 => {
                        let mut body_slice = SliceData::load_cell(slice.checked_drain_reference()?)?;
                        let body: SliceData = slice_deserialize(&mut body_slice)?;
                        Ok(ContinuationType::UntilLoopCondition(body))
                    }
                    _ => err!(ExceptionCode::UnknownError)
                }
            },
            3 => {
                match slice.get_next_int(2)? {
                    0 => {
                        let mut cond_slice = SliceData::load_cell(slice.checked_drain_reference()?)?;
                        let cond: SliceData = slice_deserialize(&mut cond_slice)?;
                        gas += Gas::load_cell_price(true);
                        let mut body_slice = SliceData::load_cell(slice.checked_drain_reference()?)?;
                        let body: SliceData = slice_deserialize(&mut body_slice)?;
                        gas += Gas::load_cell_price(true);
                        Ok(ContinuationType::WhileLoopCondition(body, cond))
                    }
                    1 => {
                        let mut body_slice = SliceData::load_cell(slice.checked_drain_reference()?)?;
                        let body: SliceData = slice_deserialize(&mut body_slice)?;
                        Ok(ContinuationType::AgainLoopBody(body))
                    }
                    2 => {
                        let mut code_slice = SliceData::load_cell(slice.checked_drain_reference()?)?;
                        let code: SliceData = slice_deserialize(&mut code_slice)?;
                        let counter = slice.get_next_int(32)? as isize;
                        Ok(ContinuationType::RepeatLoopBody(code, counter))
                    }
                    3 => {
                        let value = slice.get_next_int(32)? as i32;
                        Ok(ContinuationType::PushInt(value))
                    }
                    _ => err!(ExceptionCode::UnknownError)
                }
            }
            _ => err!(ExceptionCode::UnknownError)
        }?;

        let nargs = match slice.get_next_int(22)? as isize {
            0x3fffff => -1,
            x => x
        };
        let stack = match slice.get_next_bit()? {
            false => vec![],
            true => {
                let depth = slice.get_next_int(24)? as usize;
                let mut stack = vec![];
                if depth > 0 {
                    let (item, gas2) = StackItem::deserialize_old(slice)?;
                    gas += gas2;
                    stack.push(item);
                    let mut cell = slice.checked_drain_reference()?;
                    for _ in 1..depth {
                        let mut slice = SliceData::load_cell(cell)?;
                        let (item, gas2) = StackItem::deserialize_old(&mut slice)?;
                        stack.push(item);
                        gas += gas2;
                        cell = slice.checked_drain_reference().unwrap_or_default();
                    }
                }
                stack
            }
        };
        let (save, gas2) = SaveList::deserialize_old(slice)?;
        gas += gas2;
        slice.get_next_int(16)?; // codepage
        let code = slice_deserialize(slice)?;
        gas += Gas::load_cell_price(true);
        Ok((ContinuationData {
            code,
            nargs,
            savelist: save,
            stack: Stack {
                storage: stack
            },
            type_of: cont_type
        }, gas))
    }
}

impl fmt::Display for ContinuationData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{\n    type: {}\n    code: {}    nargs: {}\n    stack: ", self.type_of, self.code, self.nargs)?;
        if self.stack.depth() == 0 {
            writeln!(f, "empty")?;
        } else {
            writeln!(f)?;
            for x in self.stack.storage.iter() {
                write!(f, "        {}", x)?;
                writeln!(f)?;
            }
        }
        write!(f, "    savelist: ")?;
        if self.savelist.is_empty() {
            writeln!(f, "empty")?;
        } else {
            writeln!(f)?;
            for i in SaveList::REGS {
                if let Some(item) = self.savelist.get(i) {
                    writeln!(f, "        {}: {}", i, item)?
                }
            }
        }
        write!(f, "}}")
    }
}

impl fmt::Display for ContinuationType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            ContinuationType::AgainLoopBody(_) => "again",
            ContinuationType::TryCatch => "try-catch",
            ContinuationType::Ordinary => "ordinary",
            ContinuationType::PushInt(_) => "pushint",
            ContinuationType::Quit(_) => "quit",
            ContinuationType::RepeatLoopBody(_, _) => "repeat",
            ContinuationType::UntilLoopCondition(_) => "until",
            ContinuationType::WhileLoopCondition(_, _) => "while",
        };
        write!(f, "{}", name)
    }
}
