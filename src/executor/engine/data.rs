/*
* Copyright (C) 2019-2024 EverX. All Rights Reserved.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific EVERX DEV software governing permissions and
* limitations under the License.
*/

use crate::{
    executor::{engine::Engine, microcode::{VAR, CELL, SLICE, BUILDER, CONTINUATION}},
    stack::{StackItem, continuation::ContinuationData}, types::Status
};
use ever_block::{fail, GasConsumer};

// Utilities ******************************************************************

fn convert_any(engine: &mut Engine, x: u16, to: u16, from: u16) -> Status {
    if engine.cmd.vars.len() <= storage_index!(x) {
        fail!("convert_any no var {} in cmd", storage_index!(x));
    }
    let data = match address_tag!(x) {
        VAR => {
            match from {
                BUILDER => {
                    let var = engine.cmd.var_mut(storage_index!(x));
                    let builder = var.as_builder_mut()?;
                    let cell = engine.finalize_cell(builder)?;
                    match to {
                        CONTINUATION => StackItem::continuation(ContinuationData::with_code(engine.load_cell(cell)?)),
                        CELL => StackItem::Cell(cell),
                        SLICE => StackItem::Slice(engine.load_cell(cell)?),
                        _ => fail!("can convert builder only to cell, to slice or to continuation")
                    }
                }
                CELL => {
                    let var = engine.cmd.var(storage_index!(x));
                    let cell = var.as_cell()?.clone();
                    let slice = engine.load_cell(cell)?;
                    match to {
                        CONTINUATION => StackItem::continuation(ContinuationData::with_code(slice)),
                        SLICE => StackItem::Slice(slice),
                        _ => fail!("can convert cell only to slice or to continuation")
                    }
                }
                SLICE => {
                    let var = engine.cmd.var(storage_index!(x));
                    let slice = var.as_slice()?.clone();
                    match to {
                        CONTINUATION => StackItem::continuation(ContinuationData::with_code(slice)),
                        _ => fail!("can convert slice only to continuation")
                    }
                }
                _ => fail!("cannot convert")
            }
        }
        _ => StackItem::None
    };
    if data.is_null() {
        fail!("cannot convert_any x: {:X}, to: {:X}, from: {:X}", x, to, from)
    } else {
        *engine.cmd.var_mut(storage_index!(x)) = data;
    }
    Ok(())
}

// Microfunctions *************************************************************

// Convert type of x; x addressing is described in executor/microcode.rs
// to, from are one of { BUILDER, CELL, CONTINUATION, SLICE }
pub(in crate::executor) fn convert(engine: &mut Engine, x: u16, to: u16, from: u16) -> Status {
    convert_any(engine, x, to, from)
}
