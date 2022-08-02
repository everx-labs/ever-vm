/*
* Copyright (C) 2019-2022 TON Labs. All Rights Reserved.
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

use crate::{error::TvmError, types::Exception};
use ton_types::{
    error, fail, 
    BuilderData, Cell, ExceptionCode, GasConsumer, MAX_DATA_BITS, Result, SliceData
};

/// Pack data as a list of single-reference cells
pub fn pack_data_to_cell(bytes: &[u8], engine: &mut dyn GasConsumer) -> Result<Cell> {
    let mut cell = BuilderData::default();
    let cell_length_in_bytes = MAX_DATA_BITS / 8;
    for cur_slice in bytes.chunks(cell_length_in_bytes).rev() {
        if cell.bits_used() != 0 {
            let mut new_cell = BuilderData::new();
            new_cell.append_reference_cell(engine.finalize_cell(cell)?);
            cell = new_cell;
        }
        cell.append_raw(cur_slice, cur_slice.len() * 8)?;
    }
    engine.finalize_cell(cell)
}

/// Pack string as a list of single-reference cells
pub fn pack_string_to_cell(string: &str, engine: &mut dyn GasConsumer) -> Result<Cell> {
    pack_data_to_cell(string.as_bytes(), engine)
}

/// Unpack data as a list of single-reference cells
pub fn unpack_data_from_cell(
    mut cell: SliceData, 
    engine: &mut dyn GasConsumer,
) -> Result<Vec<u8>> {
    let mut data = vec![];
    loop {
        if cell.remaining_bits() % 8 != 0 {
            fail!(
                "Cannot parse string from cell because of length of cell bits len: {}",
                cell.remaining_bits()
            )
        }
        data.extend_from_slice(&cell.get_bytestring(0));
        match cell.remaining_references() {
            0 => return Ok(data),
            1 => cell = engine.load_cell(cell.reference(0)?)?,
            _ => return err!(
                ExceptionCode::TypeCheckError,
                "Incorrect representation of string in cells"
            )
        }
    }
}

pub(crate) fn bytes_to_string(data: Vec<u8>) -> Result<String> {
    String::from_utf8(data).map_err(|err| {
        exception!(
            ExceptionCode::TypeCheckError,
            "Cannot create utf8 string: {}",
            err
        )
    })
}

/// Unpack string as a list of single-reference cells
pub fn unpack_string_from_cell(cell: SliceData, engine: &mut dyn GasConsumer) -> Result<String> {
    bytes_to_string(unpack_data_from_cell(cell, engine)?)
}
