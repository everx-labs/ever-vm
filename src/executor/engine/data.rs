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
    executor::{engine::Engine, microcode::{VAR, CELL, SLICE, BUILDER, CONTINUATION}},
    stack::{StackItem, continuation::ContinuationData}, types::Status
};
use std::sync::Arc;
use ton_types::{fail, GasConsumer};

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
                        CELL => StackItem::Cell(cell),
                        SLICE => StackItem::Slice(engine.load_cell(cell)?),
                        _ => StackItem::None
                    }
                }
                CELL => {
                    let var = engine.cmd.var(storage_index!(x));
                    let cell = var.as_cell()?.clone();
                    let slice = engine.load_cell(cell)?;
                    match to {
                        CONTINUATION => StackItem::Continuation(Arc::new(ContinuationData::with_code(slice))),
                        SLICE => StackItem::Slice(slice),
                        _ => StackItem::None
                    }
                }
                SLICE => {
                    let var = engine.cmd.var(storage_index!(x));
                    let slice = var.as_slice()?.clone();
                    match to {
                        CONTINUATION => StackItem::Continuation(Arc::new(ContinuationData::with_code(slice))),
                        SLICE => StackItem::Slice(slice),
                        CELL => StackItem::Cell(slice.cell().clone()),
                        _ => StackItem::None
                    }
                }
                CONTINUATION => { // it only for undo
                    let var = engine.cmd.var(storage_index!(x));
                    let slice = var.as_continuation()?.code();
                    match to {
                        CELL => StackItem::Cell(slice.cell().clone()),
                        SLICE => StackItem::Slice(slice.clone()),
                        _ => StackItem::None
                    }
                }
                _ => StackItem::None
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
