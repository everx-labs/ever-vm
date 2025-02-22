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

use ever_block::types::ExceptionCode;
use ever_vm::{
    int, stack::{StackItem, integer::IntegerData},
};

mod common;
use common::*;

#[test]
fn test_rand_normal_case() {
    test_case("
        PUSHINT 124711402 ; magic 0x076ef1ea
        ZERO
        ZERO
        ZERO
        ZERO
        ZERO
        ZERO
        ZERO
        ZERO
        ZERO
        TUPLE 10
        SINGLE
        POP C7
        PUSHINT 1234567890
        SETRAND
        PUSHINT 1234567890
        ADDRAND
        PUSHINT 789000
        RAND
        PUSHINT 191575
        EQUAL
    ").expect_item(int!(-1));
}

#[test]
fn test_randu_normal_case() {
    test_case("
        PUSHINT 124711402 ; magic 0x076ef1ea
        ZERO
        ZERO
        ZERO
        ZERO
        ZERO
        PUSHINT 1234567890
        ZERO
        ZERO
        ZERO
        TUPLE 10
        SINGLE
        POP C7
        RANDU256
        PUSHINT 55155587004147699562571990193761432594891582513305377283752159430470838410715
        EQUAL
    ").expect_item(int!(-1));
}

#[test]
fn test_rand_error_flow(){
    expect_exception("ADDRAND", ExceptionCode::StackUnderflow);
    expect_exception("SETRAND", ExceptionCode::StackUnderflow);
    expect_exception("RAND", ExceptionCode::StackUnderflow);
    expect_exception("NULL ADDRAND", ExceptionCode::TypeCheckError);
    expect_exception("NULL SETRAND", ExceptionCode::TypeCheckError);
    expect_exception("NULL RAND", ExceptionCode::TypeCheckError);
}
