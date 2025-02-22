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

mod common;
use common::*;
use ever_vm::{
    int, stack::{StackItem, integer::IntegerData},
};

#[test]
fn super_long_flat_main_function() {
    let mut source = "PUSHINT 0".to_string();
    (0..3000).for_each(|_| source.push_str(" INC"));
    test_case(&source).expect_item(int!(3000));
}

#[test]
fn super_long_continuation_function() {
    let mut source = "PUSHINT 0 PUSHCONT {".to_string();
    (0..3000).for_each(|_| source.push_str(" INC"));
    source.push_str("} JMPX");
    test_case(&source).expect_item(int!(3000));
}

#[test]
fn test_continuation_from_1_to_1000() {
    for i in 1..=1000 {
        let mut source = String::new();
        source += "PUSHINT 0 ";
        source += "PUSHCONT { ";
        (0..i).for_each(|_| source += "INC ");
        source += "} CALLX";
        test_case(&source).expect_item(int!(i));
    }
}

#[test]
fn test_4_sibling_continuations() {
    let n = 127;
    let mut source = String::new();
    source += "PUSHINT 0 ";
    source += "PUSHCONT { ";
    (0..n).for_each(|_| source += "INC ");
    source += "} CALLX ";
    source += "PUSHCONT { ";
    (0..n).for_each(|_| source += "DEC ");
    source += "} CALLX ";
    source += "PUSHCONT { ";
    (0..n).for_each(|_| source += "INC ");
    source += "} CALLX ";
    source += "PUSHCONT { ";
    (0..n).for_each(|_| source += "DEC ");
    source += "} CALLX";
    test_case(&source).expect_item(int!(0));
}