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
    types::{Exception, ResultOpt}
};
use std::{fmt, mem};
use ton_types::{BuilderData, Cell, IBitstring, Result, error, types::ExceptionCode};
use super::{slice_serialize, slice_deserialize};

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuationData {
    code: SliceData,
    last_cmd: u8,
    pub nargs: isize,
    pub savelist: SaveList,
    pub stack: Stack,
    pub type_of: ContinuationType,
}

impl ContinuationData {
    pub fn new_empty() -> Self {
        ContinuationData {
            code: SliceData::default(),
            last_cmd: 0,
            nargs: -1,
            savelist: SaveList::new(),
            stack: Stack::new(),
            type_of: ContinuationType::Ordinary,
        }
    }

    pub fn copy_without_stack(&self) -> Self {
        ContinuationData {
            code: self.code.clone(),
            last_cmd: self.last_cmd,
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

    pub fn last_cmd(&self) -> u8 {
        self.last_cmd
    }

    pub fn next_cmd(&mut self) -> Result<u8> {
        match self.code.get_next_byte() {
            Ok(cmd) => {
                self.last_cmd = cmd;
                Ok(cmd)
            }
            Err(_) => {
                // TODO: combine error! and err!
                // panic!("n >= 8 is expected, actual value: {}", self.code.remaining_bits());
                log::error!(
                    target: "tvm",
                    "remaining bits expected >= 8, but actual value is: {}\n",
                    self.code.remaining_bits()
                );
                err!(ExceptionCode::InvalidOpcode)
            }
        }
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
        let mut cont = ContinuationData::new_empty();
        cont.code = code;
        cont
    }

    pub fn with_type(type_of: ContinuationType) -> Self {
        let mut cont = ContinuationData::new_empty();
        cont.type_of = type_of;
        cont
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

    pub fn serialize(&self) -> Result<(BuilderData, i64)> {
        let mut gas = 0;
        let mut builder = BuilderData::new();
        match &self.type_of {
            ContinuationType::AgainLoopBody(body) => {
                builder.append_bits(0xd, 4)?;
                builder.append_reference_cell(slice_serialize(body)?.into_cell()?);
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
                builder.append_reference_cell(slice_serialize(code)?.into_cell()?);
                builder.append_bits(*counter as usize, 32)?;
            }
            ContinuationType::UntilLoopCondition(body) => {
                builder.append_bits(0xa, 4)?;
                builder.append_reference_cell(slice_serialize(body)?.into_cell()?);
            }
            ContinuationType::WhileLoopCondition(body, cond) => {
                builder.append_bits(0xc, 4)?;
                builder.append_reference_cell(slice_serialize(cond)?.into_cell()?);
                builder.append_reference_cell(slice_serialize(body)?.into_cell()?);
            }
        }

        let mut stack = BuilderData::new();
        stack.append_bits(self.stack.depth(), 24)?;
        let mut stack_list = BuilderData::new();
        for item in self.stack.iter().rev() {
            let mut cons = BuilderData::new();
            let (serialized, gas2) = item.serialize()?;
            gas += gas2;
            cons.append_builder(&serialized)?;
            cons.append_reference_cell(stack_list.into_cell()?);
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
        let (serialized, gas2) = self.savelist.serialize()?;
        gas += gas2;
        builder.append_builder(&serialized)?;
        builder.append_bits(0, 16)?; // codepage
        builder.append_builder(&slice_serialize(&self.code)?)?;
        Ok((builder, gas))
    }

    pub fn deserialize(slice: &mut SliceData) -> Result<(Self, i64)> {
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
                        let mut body_slice = SliceData::from(slice.checked_drain_reference()?);
                        let body: SliceData = slice_deserialize(&mut body_slice)?;
                        Ok(ContinuationType::UntilLoopCondition(body))
                    }
                    _ => err!(ExceptionCode::UnknownError)
                }
            },
            3 => {
                match slice.get_next_int(2)? {
                    0 => {
                        let mut cond_slice = SliceData::from(slice.checked_drain_reference()?);
                        let cond: SliceData = slice_deserialize(&mut cond_slice)?;
                        gas += Gas::load_cell_price(true);
                        let mut body_slice = SliceData::from(slice.checked_drain_reference()?);
                        let body: SliceData = slice_deserialize(&mut body_slice)?;
                        gas += Gas::load_cell_price(true);
                        Ok(ContinuationType::WhileLoopCondition(body, cond))
                    }
                    1 => {
                        let mut body_slice = SliceData::from(slice.checked_drain_reference()?);
                        let body: SliceData = slice_deserialize(&mut body_slice)?;
                        Ok(ContinuationType::AgainLoopBody(body))
                    }
                    2 => {
                        let mut code_slice = SliceData::from(slice.checked_drain_reference()?);
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
                    let (item, gas2) = StackItem::deserialize(slice)?;
                    gas += gas2;
                    stack.push(item);
                    let mut cell = slice.checked_drain_reference()?;
                    for _ in 1..depth {
                        let mut slice = SliceData::from(cell);
                        let (item, gas2) = StackItem::deserialize(&mut slice)?;
                        stack.push(item);
                        gas += gas2;
                        cell = slice.checked_drain_reference().unwrap_or_default();
                    }
                }
                stack
            }
        };
        let (save, gas2) = SaveList::deserialize(slice)?;
        gas += gas2;
        slice.get_next_int(16)?; // codepage
        let code = slice_deserialize(slice)?;
        gas += Gas::load_cell_price(true);
        Ok((ContinuationData {
            code,
            last_cmd: 0,
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
        write!(f, "{{\n    {}\n    nargs: {}\n    stack: ", self.code, self.nargs)?;
        if self.stack.depth() == 0 {
            writeln!(f, "empty")?;
        } else {
            for x in self.stack.storage.iter() {
                writeln!(f)?;
                write!(f, "        {}", x)?;
            }
        }
        write!(f, "}}")
    }
}
