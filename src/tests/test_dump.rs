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
   executor::{
       engine::Engine,
       dump::{
           BIN, dump_var, execute_dump_bin, execute_dump_hex, execute_dump_stack,
           execute_dump_stack_top, execute_dump_str, execute_dump_string,
           execute_print_bin, execute_print_hex, execute_print_str, HEX, STR
       }
   },
   stack::{Stack, StackItem, integer::IntegerData}
};
use ever_block::{BuilderData, SliceData};

#[test]
fn test_dump_var() {
    [0, 15, 23466454, 347387434, 4383434].iter().for_each(|value| {
        assert_eq!(format!("{}", *value), dump_var(&int!(*value), 0));
        assert_eq!(format!("{:X}", *value), dump_var(&int!(*value), HEX));
        assert_eq!(format!("{:b}", *value), dump_var(&int!(*value), BIN));
    });
    [-15, -23466454, -476343874].iter().for_each(|value| {
        assert_eq!(format!("{}", *value), dump_var(&int!(*value), 0));
        assert_eq!(format!("-{:X}", -*value), dump_var(&int!(*value), HEX));
        assert_eq!(format!("-{:b}", -*value), dump_var(&int!(*value), BIN));
    });

    let slice = StackItem::Slice(SliceData::new(vec![0x41, 0x42, 0x43, 0x80]));
    assert_eq!("ABC".to_string(), dump_var(&slice, STR));
    assert_eq!("CS<414243>(0..24)", dump_var(&slice, HEX));
    assert_eq!("CS<010000010100001001000011>(0..24)", dump_var(&slice, BIN));
}

#[test]
fn test_dump_commands() {
    let int = -15;
    let builder = BuilderData::with_raw(vec![0x41, 0x42, 0x43], 24).unwrap(); // ABC

    let mut stack = Stack::new();
    stack.push_builder(builder.clone());
    stack.push(StackItem::Cell(builder.clone().into_cell().unwrap()));
    stack.push(StackItem::Slice(SliceData::load_builder(builder).unwrap()));
    stack.push(int!(int));
    let engine = &mut Engine::with_capabilities(0).setup_with_libraries(
        SliceData::new(vec![1, 0, 0x0A, 0x80]), None, Some(stack), None, vec![]
    );
    log::trace!("--- {} as str\n", int);
    execute_dump_str(engine).unwrap();
    log::trace!("--- {} as hex\n", int);
    execute_dump_hex(engine).unwrap();
    log::trace!("--- {} as bin\n", int);
    execute_dump_bin(engine).unwrap();
    log::trace!("--- stack\n");
    execute_dump_stack(engine).unwrap();
    log::trace!("--- top 2 of stack\n");
    assert_eq!(engine.next_cmd().unwrap(), 1);
    execute_dump_stack_top(engine).unwrap();
    log::trace!("--- str, hex, bin\n");
    assert_eq!(engine.next_cmd().unwrap(), 0);
    execute_print_hex(engine).unwrap();
    execute_print_bin(engine).unwrap();
    execute_print_str(engine).unwrap();
    execute_dump_string(engine).unwrap(); // flush with LF
}
