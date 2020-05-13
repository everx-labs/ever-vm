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

use crate::{
    error::TvmError, stack::{SliceData, Stack, StackItem, savelist::SaveList}, 
    types::{Exception, ResultOpt}
};
use std::{fmt, mem};
use ton_types::{Cell, error, Result, types::ExceptionCode};

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
                    "n >= 8 is expected, actual value: {}", 
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

}

impl fmt::Display for ContinuationData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{\n    {}\n    nargs: {}\n    stack: ", self.code, self.nargs)?;
        if self.stack.depth() == 0 {
            write!(f, "empty\n")?;
        } else {
            for x in self.stack.storage.iter() {
                write!(f, "\n        {}", x)?;
            }
        }
        write!(f, "}}")
    }
}

