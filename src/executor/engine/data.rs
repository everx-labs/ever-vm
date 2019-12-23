/*
* Copyright 2018-2019 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.  You may obtain a copy of the
* License at: https://ton.dev/licenses
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use executor::microcode::{VAR, CELL, SLICE, BUILDER, CONTINUATION};
use executor::types::{Ctx, Undo};
use stack::{ContinuationData, SliceData, StackItem};
use std::mem;
use std::sync::Arc;
use types::{Result, Status};

// Utilities ******************************************************************

fn convert_any(ctx: &mut Ctx, x: u16, to: u16, from: u16) -> Status {
    match address_tag!(x) {
        VAR => {
            let x = ctx.engine.cmd.var_mut(storage_index!(x));
            let data = match to {
                CONTINUATION => StackItem::Continuation(Arc::new(ContinuationData::with_code(match from {
                    SLICE => x.as_slice()?.clone(),
                    CELL => SliceData::from_cell_ref(x.as_cell()?, &mut ctx.engine.gas),
                    _ => unimplemented!()
                }))),
                CELL => StackItem::Cell(match from {
                    BUILDER => x.as_builder_mut()?.finalize(&mut ctx.engine.gas),
                    CONTINUATION => x.as_continuation()?.code().into_cell(), // it only for undo
                    // SLICE => x.as_slice()?.into_cell(),
                    _ => unimplemented!("to: {:X}, from: {:X}", to, from)
                }),
                SLICE => StackItem::Slice(match from {
                    BUILDER => x.as_builder_mut()?.finalize_and_load(&mut ctx.engine.gas),
                    CELL => SliceData::from_cell_ref(x.as_cell()?, &mut ctx.engine.gas),
                    // CONTINUATION => x.as_continuation()?.code().clone(),
                    _ => unimplemented!("to: {:X}, from: {:X}", to, from)
                }),
                _ => unimplemented!("to: {:X}, from: {:X}", to, from)
            };
            mem::replace(x, data);
        }
        _ => unimplemented!("x: {:X}, to: {:X}, from: {:X}", x, to, from)
    };
    Ok(())
}

// Microfunctions *************************************************************

// Convert type of x; x addressing is described in executor/microcode.rs
// to, from are one of { BUILDER, CELL, CONTINUATION, SLICE }
pub(in executor) fn convert(mut ctx: Ctx, x: u16, to: u16, from: u16) -> Result<Ctx> {  
    convert_any(&mut ctx, x, to, from)?;                                                                             
    ctx.engine.cmd.undo.push(Undo::WithCodeTriplet(undo_convert, x, to, from));
    Ok(ctx)
}

fn undo_convert(ctx: &mut Ctx, x: u16, to: u16, from: u16) {
    convert_any(ctx, x, from, to).unwrap()
}
