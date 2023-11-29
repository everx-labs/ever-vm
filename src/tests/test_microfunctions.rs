/*
* Copyright (C) 2019-2023 TON Labs. All Rights Reserved.
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
    executor::{Engine, engine::storage::swap, microcode::{CTRL, SAVELIST, VAR}},
    stack::{StackItem, continuation::ContinuationData}
};
use ton_types::{Cell, SliceData, types::ExceptionCode};
use ton_block::GlobalCapabilities;

#[test]
fn test_swap_with_any() {
    let mut engine = Engine::with_capabilities(0).setup_with_libraries(
        SliceData::new_empty(), None, None, None, vec![]
    );
    let mut c0 = ContinuationData::new_empty();
    let mut c1 = ContinuationData::new_empty();
    let s0 = StackItem::Cell(SliceData::new(vec![1, 2, 3, 4, 5]).into_cell());
    let s1 = StackItem::Cell(SliceData::new(vec![6, 7, 8, 9, 0]).into_cell());
    c0.put_to_savelist(4, &mut s0.clone()).unwrap();
    c1.put_to_savelist(5, &mut s1.clone()).unwrap();
    engine.cmd.push_var(StackItem::continuation(c0));
    engine.cmd.push_var(StackItem::continuation(c1));
    swap(&mut engine, var!(0), ctrl!(0)).unwrap();
    swap(&mut engine, var!(1), ctrl!(1)).unwrap();
    swap(&mut engine, savelist!(ctrl!(0), 4), savelist!(ctrl!(1), 5)).unwrap();
    let ctrls = engine.ctrl(0).unwrap();
    let cont = ctrls.as_continuation().unwrap();
    assert_eq!(cont.savelist.get(4).unwrap(), &s1);
    let ctrls = engine.ctrl(1).unwrap();
    let cont = ctrls.as_continuation().unwrap();
    assert_eq!(cont.savelist.get(5).unwrap(), &s0);
}

#[test]
fn test_swap_with_none() {
    let mut engine = Engine::with_capabilities(GlobalCapabilities::CapsTvmBugfixes2022 as u64).setup_with_libraries(
        SliceData::new_empty(), None, None, None, vec![]
    );
    engine.cmd.push_var(StackItem::Cell(Cell::default()));
    engine.cmd.push_var(StackItem::None);
    //try to put CELL to c4 - Ok
    swap(&mut engine, var!(0), ctrl!(4)).unwrap();
    assert_ne!(engine.cmd.var(0), &StackItem::None);
    //try to put NULL to c4 - Type Check Error
    swap(&mut engine, var!(0), ctrl!(4)).unwrap();
    assert_eq!(
        crate::error::tvm_exception_code(&swap(&mut engine, var!(1), ctrl!(4)).unwrap_err()),
        Some(ExceptionCode::TypeCheckError)
    );
    // try to put NULL to c2 - Ok
    assert!(!engine.ctrl(2).unwrap().is_null());
    swap(&mut engine, var!(1), ctrl!(2)).unwrap();
    assert_eq!(engine.ctrl(2).unwrap(), &StackItem::None);
    // try to put CONT to c2 - Ok
    engine.cmd.vars[0] = StackItem::continuation(ContinuationData::new_empty());
    swap(&mut engine, var!(0), ctrl!(2)).unwrap();
    assert_eq!(engine.cmd.var(0), &StackItem::None);
}

#[test]
fn test_swap_with_ctrl() {
    let mut engine = Engine::with_capabilities(0).setup_with_libraries(
        SliceData::new_empty(), None, None, None, vec![]
    );
    let c0 = ContinuationData::new_empty();
    let c1 = ContinuationData::new_empty();
    engine.cmd.push_var(StackItem::continuation(c0));
    engine.cmd.push_var(StackItem::continuation(c1));
    swap(&mut engine, var!(0), ctrl!(0)).unwrap();
    swap(&mut engine, var!(0), savelist!(ctrl!(0), 0)).unwrap();
    assert_eq!(engine.cmd.var(0), &StackItem::None);
    swap(&mut engine, var!(1), ctrl!(1)).unwrap();
    swap(&mut engine, var!(1), savelist!(ctrl!(1), 1)).unwrap();
    assert_eq!(engine.cmd.var(1), &StackItem::None);
}
