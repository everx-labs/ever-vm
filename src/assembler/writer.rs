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

use crate::assembler::OperationError;
use ton_types::{BuilderData, SliceData};

pub trait Writer : 'static {
    fn new() -> Self;
    fn write_command(&mut self, command: &[u8]) -> Result<(), OperationError>;
    fn write_composite_command(&mut self, code: &[u8], references: SliceData) -> Result<(), OperationError>;
    fn finalize(self) -> SliceData;
}

pub(crate) struct CodePage0 {
    cells: Vec<BuilderData>,
}

impl Writer for CodePage0 {
    /// Constructs new Writes
    fn new() -> Self {
        Self {
            cells: vec![BuilderData::new()],
        }
    }
    /// write simple command
    fn write_command(&mut self, command: &[u8]) -> Result<(), OperationError> {
        if !self.cells.is_empty() {
            if self.cells.last_mut().unwrap().append_raw(command, command.len() * 8).is_ok() {
                return Ok(());
            }
        }
        let mut code = BuilderData::new();
        if code.append_raw(command, command.len() * 8).is_ok() {
            self.cells.push(code);
            return Ok(());
        }
        Err(OperationError::NotFitInSlice)
    }
    /// writes command with additional reference
    fn write_composite_command(
        &mut self, 
        command: &[u8], 
        reference: SliceData
    ) -> Result<(), OperationError> {
        let mut code = BuilderData::new();
        if code.append_raw(command, command.len() * 8).is_ok()
            && code.checked_append_reference(reference.into_cell()).is_ok() {
            self.cells.push(code);
            return Ok(());
            }
        Err(OperationError::NotFitInSlice)
    }
    /// puts every cell as reference to previous
    fn finalize(mut self) -> SliceData {
        let mut cursor = self.cells.pop().expect("cells can't be empty");
        while !self.cells.is_empty() {
            let mut destination = self.cells.pop()
                .expect("vector is not empty");
            destination.append_reference(cursor);
            cursor = destination; 
        }
        cursor.into()
    }
}
