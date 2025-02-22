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
use ever_block::types::ExceptionCode;
use ever_vm::{
    boolean, int, stack::{Stack, StackItem, integer::IntegerData},
};

#[test]
fn sgn() {
    test_case(
        "PUSHINT 3
         SGN",
    ).expect_item(int!(1));
    test_case(
        "PUSHINT -3
         SGN",
    ).expect_item(int!(-1));
    test_case(
        "PUSHINT 0
         SGN",
    ).expect_item(int!(0));
    test_case(
        "PUSHNAN
         SGN",
    ).expect_failure(ExceptionCode::IntegerOverflow);
    test_case(
        "PUSHNAN
         QSGN",
    ).expect_item(int!(nan));
}

#[test]
fn less() {
    test_case(
        "PUSHINT 2
         PUSHINT 3
         LESS",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 2
         PUSHINT 2
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 3
         PUSHINT 2
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         PUSHINT 0
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         PUSHINT 1
         LESS",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         PUSHINT 0
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         PUSHINT -2
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -2
         PUSHINT -1
         LESS",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         PUSHINT 1
         LESS",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         PUSHINT -1
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         PUSHINT -1
         LESS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         PUSHINT 0
         LESS",
    ).expect_item(boolean!(true));
}

#[test]
fn equal() {
    test_case(
        "PUSHINT 0
         PUSHINT 0
         EQUAL",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         PUSHINT 1
         EQUAL",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         PUSHINT 0
         EQUAL",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         PUSHINT 1
         EQUAL",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -2
         PUSHINT -2
         EQUAL",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -2
         PUSHINT 2
         EQUAL",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -3
         PUSHINT -4
         EQUAL",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 3
         PUSHINT 4
         EQUAL",
    ).expect_item(boolean!(false));
}

#[test]
fn less_or_equal() {
    test_case(
        "PUSHINT 0
        PUSHINT 0
        LEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
        PUSHINT 0
        LEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
        PUSHINT 0
        LEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
        PUSHINT -1
        LEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -2
        PUSHINT -3
        LEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -3
        PUSHINT -2
        LEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 2
        PUSHINT 3
        LEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 2
         PUSHINT 2
         LEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 3
         PUSHINT 2
         LEQ",
    ).expect_item(boolean!(false));
}

#[test]
fn greater() {
    test_case(
        "PUSHINT 0
         PUSHINT 1
         GREATER",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         PUSHINT 0
         GREATER",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         PUSHINT 0
         GREATER",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         PUSHINT -1
         GREATER",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         PUSHINT -1
         GREATER",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 2
         PUSHINT 3
         GREATER",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 2
         PUSHINT 2
         GREATER",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 3
         PUSHINT 2
         GREATER",
    ).expect_item(boolean!(true));
}

#[test]
fn not_equal() {
    test_case(
        "PUSHINT 0
         PUSHINT 0
         NEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         PUSHINT 0
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         PUSHINT 1
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         PUSHINT 0
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         PUSHINT -1
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         PUSHINT -1
         NEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         PUSHINT 1
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         PUSHINT -1
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 2
         PUSHINT 3
         NEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 2
         PUSHINT 2
         NEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 3
         PUSHINT 2
         NEQ",
    ).expect_item(boolean!(true));
}

#[test]
fn greater_or_equal() {
    test_case(
        "PUSHINT 0
         PUSHINT 0
         GEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         PUSHINT 0
         GEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         PUSHINT 0
         GEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         PUSHINT 1
         GEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         PUSHINT 1
         GEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         PUSHINT -1
         GEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 2
         PUSHINT 3
         GEQ",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 2
         PUSHINT 2
         GEQ",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 3
         PUSHINT 2
         GEQ",
    ).expect_item(boolean!(true));
}

#[test]
fn cmp() {
    test_case(
        "PUSHINT 0
         PUSHINT 0
         CMP",
    ).expect_item(int!(0));
    test_case(
        "PUSHINT -1
         PUSHINT -1
         CMP",
    ).expect_item(int!(0));
    test_case(
        "PUSHINT 0
         PUSHINT 1
         CMP",
    ).expect_item(int!(-1));
    test_case(
        "PUSHINT 1
         PUSHINT 0
         CMP",
    ).expect_item(int!(1));
    test_case(
        "PUSHINT -1
         PUSHINT 0
         CMP",
    ).expect_item(int!(-1));
    test_case(
        "PUSHINT 0
         PUSHINT -1
         CMP",
    ).expect_item(int!(1));
    test_case(
        "PUSHINT -1
         PUSHINT 1
         CMP",
    ).expect_item(int!(-1));
    test_case(
        "PUSHINT 2
         PUSHINT 3
         CMP",
    ).expect_item(int!(-1));
    test_case(
        "PUSHINT 2
         PUSHINT 2
         CMP",
    ).expect_item(int!(0));
    test_case(
        "PUSHINT 3
         PUSHINT 2
         CMP",
    ).expect_item(int!(1));
}

#[test]
fn equal_to() {
    test_case(
        "PUSHINT 0
         EQINT 0",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         EQINT 0",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         EQINT 1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         EQINT 0",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         EQINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         EQINT 1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 2
         EQINT 2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         EQINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 3
         EQINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHNAN
         EQINT 2",
    ).expect_failure(ExceptionCode::IntegerOverflow);
    test_case(
        "PUSHINT 3
         QEQINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHNAN
         QEQINT 2",
    ).expect_item(int!(nan));
}

#[test]
fn is_zero() {
    test_case(
        "PUSHINT 2
         ISZERO",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         ISZERO",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         ISZERO",
    ).expect_item(boolean!(true));
}

#[test]
fn less_than() {
    test_case(
        "PUSHINT 2
         LESSINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         LESSINT 2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 3
         LESSINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         LESSINT -2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -2
         LESSINT -1",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         LESSINT 2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 1
         LESSINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         LESSINT 0",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         LESSINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         LESSINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHNAN
         LESSINT 1",
    ).expect_failure(ExceptionCode::IntegerOverflow);
    test_case(
        "PUSHINT -1
         QLESSINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHNAN
         QLESSINT 1",
    ).expect_item(int!(nan));
}

#[test]
fn is_negative() {
    test_case(
        "PUSHINT 2
         ISNEG",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         ISNEG",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         ISNEG",
    ).expect_item(boolean!(false));
}

#[test]
fn is_not_positive() {
    test_case(
        "PUSHINT 2
         ISNPOS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         ISNPOS",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         ISNPOS",
    ).expect_item(boolean!(true));
}

#[test]
fn greater_than() {
    test_case(
        "PUSHINT 2
         GTINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         GTINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 3
         GTINT 2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         GTINT -2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -2
         GTINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         GTINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         GTINT -1",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         GTINT 0",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 0
         GTINT -1",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         GTINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHNAN
         GTINT 1",
    ).expect_failure(ExceptionCode::IntegerOverflow);
    test_case(
        "PUSHINT -1
         QGTINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHNAN
         QGTINT 1",
    ).expect_item(int!(nan));
}

#[test]
fn is_positive() {
    test_case(
        "PUSHINT 2
         ISPOS",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         ISPOS",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         ISPOS",
    ).expect_item(boolean!(false));
}

#[test]
fn is_not_negative() {
    test_case("ISNNEG").expect_bytecode(vec![0xC2, 0xFF, 0x80]);
    test_case("GTINT -1").expect_bytecode(vec![0xC2, 0xFF, 0x80]);
}

#[test]
fn not_equal_to() {
    test_case(
        "PUSHINT 2
         NEQINT 2",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT 1
         NEQINT 2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 3
         NEQINT 2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -3
         NEQINT -2",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT -1
         NEQINT -1",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         NEQINT 1",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHINT 0
         NEQINT 0",
    ).expect_item(boolean!(false));
    test_case(
        "PUSHINT -1
         NEQINT 1",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHNAN
         NEQINT 1",
    ).expect_failure(ExceptionCode::IntegerOverflow);
    test_case(
        "PUSHINT -1
         QNEQINT 1",
    ).expect_item(boolean!(true));
    test_case(
        "PUSHNAN
         QNEQINT 1",
    ).expect_item(int!(nan));
}

#[test]
fn is_nan() {
    test_case(
       "PUSHINT 2
        ISNAN",
    ).expect_item(boolean!(false));
    test_case(
       "PUSHNAN
        ISNAN",
    ).expect_item(boolean!(true));
}

#[test]
fn check_nan() {
    test_case(
       "PUSHINT 2
        CHKNAN",
    ).expect_stack(Stack::new().push(int!(2)));
    test_case(
       "PUSHNAN
        CHKNAN",
    ).expect_failure(ExceptionCode::IntegerOverflow);
}
