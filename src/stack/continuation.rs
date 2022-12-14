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
    stack::{SliceData, Stack, StackItem, savelist::SaveList},
    types::{Exception, ResultOpt}
};
use std::{fmt, mem};
use ton_types::{BuilderData, Cell, IBitstring, Result, error, types::ExceptionCode, GasConsumer};
use super::{slice_serialize, slice_deserialize, items_deserialize, items_serialize};

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

    pub fn serialize(&self, gas_consumer: &mut dyn GasConsumer) -> Result<BuilderData> {
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
            let stack = items_serialize(&self.stack.storage, 24, gas_consumer)?;
            builder.append_builder(&stack)?; // second ref
        }
        let savelist = self.savelist.serialize(gas_consumer)?;
        builder.append_builder(&savelist)?; // third ref
        builder.append_bits(0, 16)?; // codepage
        builder.append_builder(&slice_serialize(&self.code)?)?; // last ref
        Ok(builder)
    }

    pub fn deserialize(slice: &mut SliceData, gas_consumer: &mut dyn GasConsumer) -> Result<Self> {
        let cont_type = match slice.get_next_int(2)? {
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
            0x3fffff => -1,
            x => x
        };
        let stack = if slice.get_next_bit()? {
            items_deserialize(slice, 24, gas_consumer)?
        } else {
            vec!()
        };
        let save = SaveList::deserialize(slice, gas_consumer)?;
        slice.get_next_int(16)?; // codepage
        let code = slice_deserialize(slice)?;
        Ok(ContinuationData {
            code,
            nargs,
            savelist: save,
            stack: Stack {
                storage: stack
            },
            type_of: cont_type
        })
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
