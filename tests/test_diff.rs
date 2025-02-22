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

#![cfg(feature = "gosh_test")]

use std::ops::AddAssign;
use ever_block::GlobalCapabilities;
use ever_block::{BuilderData, Cell, ExceptionCode, SliceData};

mod common;
use common::*;
use ever_vm::{
    stack::StackItem, utils::{pack_string_to_cell, unpack_string_from_cell}
};
use ever_vm::executor::gas::gas_state::Gas;

fn cell_to_utf8(str: &str) {
    let cell = pack_string_to_cell(str, &mut 0).unwrap();
    let result = unpack_string_from_cell(SliceData::load_cell(cell).unwrap(), &mut 0).unwrap();
    assert_eq!(str.to_string(), result);
}

fn test_cell(ref1: &str, ref2: &str) -> Cell {
    let ref1_cell = pack_string_to_cell(ref1, &mut 0).unwrap();
    let ref2_cell = pack_string_to_cell(ref2, &mut 0).unwrap();
    let mut cell = BuilderData::default();
    cell.checked_append_reference(ref1_cell).unwrap();
    cell.checked_append_reference(ref2_cell).unwrap();
    cell.into_cell().unwrap()
}

fn test_case_with_cell(code: &str, cell: Cell) -> TestCaseInputs {
    test_case_with_ref(code, cell)
        .with_capability(GlobalCapabilities::CapDiff)
        .skip_fift_check(true)
}

#[test]
fn test_cell_to_utf() {
    
    cell_to_utf8("My string");
    cell_to_utf8("");
    cell_to_utf8("1");
    cell_to_utf8("\
         This is long, long, very long, too very long, long, not small, large, \
         big, string string, sss, string, sssssss, tring, ring, ing, ng, g, gn, \
         gni, gnir, gnirt, gnirts, mmmm oiuy
    ");
    cell_to_utf8("String with Русскими символами ttttt");
    cell_to_utf8("String with \u{1F605} ttttt");

    let symbols = "0123456789abcdefghijklmnopqrstuvwxyz,-=//*ABCDEFGHIJKLMNOPQRSTUVWXYZ@#$%^&";
    let mut cur_string = String::with_capacity(100000);
    while cur_string.len() < 50000 {
        let pos = cur_string.len() % symbols.len();
        cur_string.add_assign(&symbols[pos..std::cmp::min(symbols.len(), pos + 37)]);
        cell_to_utf8(&*cur_string);
    }

}

fn execute_diff(original: &str, modified: &str, answer: &str, gas_used: i64) {
    let cell = test_cell(original, modified);
    let answer_cell = pack_string_to_cell(answer, &mut 0).unwrap();
    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF
        ",
        cell
    )
    .expect_item(StackItem::Cell(answer_cell))
    .expect_gas(1000000000, 1000000000, 0, 1000000000 - gas_used);
}

#[test]
fn test_diff() {

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\n";
    let answer = "@@ -4,0 +5 @@\n+Oathbringer\n";
    execute_diff(original, modified, answer, 996);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\n";
    let answer = "@@ -5 +4,0 @@\n-Bbbbbbbb\n";
    execute_diff(original, modified, answer, 996);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\n";
    let answer = "@@ -5 +5 @@\n-Bbbbbbbb\n+Oathbringer\n";
    execute_diff(original, modified, answer, 996);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbbbbbbb\nSuffix\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\nSuffix\n";
    let answer = "@@ -5 +5 @@\n-Bbbbbbbb\n+Oathbringer\n";
    execute_diff(original, modified, answer, 1012);

    let original = "The Way of Kings\nThe Way of Kings\nThe Русские буквы Kings\nWords of Radiance\nBbbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Русские буквы Kings\nWords of Radiance\nOathbringer\n";
    let answer = "@@ -5 +5 @@\n-Bbbbbbbb\n+Oathbringer\n";
    execute_diff(original, modified, answer, 996);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbРусскиеБуквыbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringeРусскиеБуквыr\n";
    let answer = "@@ -5 +5 @@\n-BbРусскиеБуквыbbb\n+OathbringeРусскиеБуквыr\n";
    execute_diff(original, modified, answer, 996);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nB\u{1F605}bbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbring\u{1F605}er\n";
    let answer = "@@ -5 +5 @@\n-B\u{1F605}bbbbbbb\n+Oathbring\u{1F605}er\n";
    execute_diff(original, modified, answer, 996);

}

#[test]
fn test_diff_failure_1() {
    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\n";
    let cell = test_cell(original, modified);
    test_case_with_ref(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF
        ",
        cell,
    )
    .skip_fift_check(true)
    .expect_failure(ExceptionCode::InvalidOpcode);
}

#[test]
fn test_diff_failure_2() {
    test_case_with_cell("DIFF", Cell::default()).expect_failure(ExceptionCode::StackUnderflow);
}

fn execute_patch(original: &str, patch: &str, answer: &str, gas_used: i64, binary: bool) {
    let cell = test_cell(original, patch);
    let answer_cell = pack_string_to_cell(answer, &mut 0).unwrap();
    test_case_with_cell(
        &format!(
            "PUSHREFSLICE
            LDREF
            LDREF
            DROP
            {}", 
            if binary {
                "DIFF_PATCH_BINARY"
            } else {
                "DIFF_PATCH"
            }
        ),
        cell
    )
    .expect_item(StackItem::Cell(answer_cell))
    .expect_gas(1000000000, 1000000000, 0, 1000000000 - gas_used);
}

#[test]
fn test_patch() {

    let patch = "\
@@ -10,6 +10,8 @@
 First:
     Life before death,
     strength before weakness,
     journey before destination.
 Second:
-    I will put the law before all else.
+    I swear to seek justice,
+    to let it guide me,
+    until I find a more perfect Ideal.
";

    let original = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I will put the law before all else.
";

    let answer = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I swear to seek justice,
    to let it guide me,
    until I find a more perfect Ideal.
";

    execute_patch(original, patch, answer, 1551, false);
    execute_patch(original, patch, answer, 1547, true);

}

#[test]
fn test_patch_q_with_out_of_gas() {

    let patch = "\
@@ -10,6 +10,8 @@
 First:
     Life before death,
     strength before weakness,
     journey before destination.
 Second:
-    I will put the law before all else.
+    I swear to seek justice,
+    to let it guide me,
+    until I find a more perfect Ideal.
";

    let original = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I will put the law before all else.
";

    let cell = test_cell(original, patch);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCHQ
        ",
        cell.clone()
    )
    .with_gas(Gas::new(500, 0, 500, 1))
    .expect_failure(ExceptionCode::OutOfGas);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH_BINARYQ
        ",
        cell
    )
    .with_gas(Gas::new(500, 0, 500, 1))
    .expect_failure(ExceptionCode::OutOfGas);

}

#[test]
fn execute_patch_failure_1() {
    let cell = test_cell("123", "456");
    test_case_with_ref(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH
        ",
        cell
    )
    .skip_fift_check(true)
    .expect_failure(ExceptionCode::InvalidOpcode);
}

#[test]
fn execute_patch_failure_2() {
    test_case_with_cell("DIFF_PATCH", Cell::default())
        .expect_failure(ExceptionCode::StackUnderflow);
}

#[test]
fn test_patch_failure_3() {

    let patch = "\
@@ -10,6 +10,8 @@
 Ttttt:
     Life before death,
     strength before weakness,
     journey before destination.
 Second:
-    I mmm put the law before all else.
+    I swear to seek justice,
+    to let it guide me,
+    until I find a more perfect Ideal.
";

    let original = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I will put the law before all else.
";

    let cell = test_cell(original, patch);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH
        ",
        cell.clone()
    )
    .expect_failure(ExceptionCode::TypeCheckError);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH_BINARY
        ",
        cell
    )
    .expect_failure(ExceptionCode::TypeCheckError);

}

#[test]
fn test_patch_failure_4() {

    let patch = "\
@@ -10,6 +10, @@
 Ttttt:
     Life before death,
     strength before weakness,
     journey before destination.
 Second:
-    I mmm put the law before all else.
+    I swear to seek justice,
+    to let it guide me,
+    until I find a more perfect Ideal.
";

    let original = "\
First:
";

    let cell = test_cell(original, patch);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH
        ",
        cell.clone()
    )
    .expect_failure(ExceptionCode::TypeCheckError);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCHQ
        ",
        cell.clone()
    )
    .expect_item(StackItem::None);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH_BINARY
        ",
        cell.clone()
    )
    .expect_failure(ExceptionCode::TypeCheckError);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH_BINARYQ
        ",
        cell
    )
    .expect_item(StackItem::None);

}

fn zip_unzip(str: &str) {
    let cell = pack_string_to_cell(str, &mut 0).unwrap();
    test_case_with_cell(
        "PUSHREF
        ZIP
        UNZIP
        ",
        cell.clone()
    )
    .expect_item(StackItem::Cell(cell));
}

#[test]
fn test_zip_unzip() {

    zip_unzip("My string");
    zip_unzip("");
    zip_unzip("1");
    zip_unzip("This is long, long, very long, too very long, long, not small, large, big, string string, sss, string, sssssss, tring, ring, ing, ng, g, gn, gni, gnir, gnirt, gnirts, mmmm oiuy");
    zip_unzip("String with Русскими символами ttttt");
    zip_unzip("String with \u{1F605} ttttt");

    let symbols = "0123456789abcdefghijklmnopqrstuvwxyz,-=//*ABCDEFGHIJKLMNOPQRSTUVWXYZ@#$%^&";
    let mut cur_string = String::with_capacity(100000);
    while cur_string.len() < 50000 {
        let pos = cur_string.len() % symbols.len();
        cur_string.add_assign(&symbols[pos..std::cmp::min(symbols.len(), pos + 37)]);
        zip_unzip(&*cur_string);
    }

}

#[test]
fn test_empty_zip_unzip() {

    let str = "";
    let cell = pack_string_to_cell(str, &mut 0).unwrap();

    test_case_with_cell(
        "PUSHREF
        ZIP
        ",
        cell.clone()
    )
    .expect_item(StackItem::Cell(cell.clone()));

    test_case_with_cell(
        "PUSHREF
        UNZIP
        ",
        cell.clone()
    )
    .expect_item(StackItem::Cell(cell));

}

#[test]
fn test_zip_unzip_fee() {
    let string = "This is long, long, very long, too very long, long, not small, large, big, string string, sss, string, sssssss, tring, ring, ing, ng, g, gn, gni, gnir, gnirt, gnirts, mmmm oiuy";
    let cell = pack_string_to_cell(string, &mut 0).unwrap();
    test_case_with_cell(
        "PUSHREF
        ZIP
        UNZIP
        ",
        cell.clone()
    )
    .expect_item(StackItem::Cell(cell))
    .expect_gas(1000000000, 1000000000, 0, 1000000000 - 1720);
}

#[test]
fn test_zip_failure_1() {
    let cell = pack_string_to_cell("My string", &mut 0).unwrap();
    test_case_with_ref(
        "PUSHREFSLICE
        ZIP",
        cell
    )
    .skip_fift_check(true)
    .expect_failure(ExceptionCode::InvalidOpcode);
}

#[test]
fn test_zip_failure_2() {
    let cell = pack_string_to_cell("My string", &mut 0).unwrap();
    test_case_with_cell(
        "PUSHREF
        UNZIP",
        cell
    )
    .expect_failure(ExceptionCode::UnknownError);
}

fn execute_diff_zip(original: &str, modified: &str) {
    let cell = test_cell(original, modified);
    let answer_cell = pack_string_to_cell(modified, &mut 0).unwrap();
    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP

        ZIP
        SWAP
        ZIP

        DUP
        ROT

        DIFF_ZIP
        DIFF_PATCH_ZIP
        UNZIP
        ",
        cell
    )
    .expect_item(StackItem::Cell(answer_cell));
}

#[test]
fn test_diff_zip() {

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\n";
    execute_diff_zip(original, modified);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\n";
    execute_diff_zip(original, modified);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\n";
    execute_diff_zip(original, modified);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbbbbbbb\nSuffix\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringer\nSuffix\n";
    execute_diff_zip(original, modified);

    let original = "The Way of Kings\nThe Way of Kings\nThe Русские буквы Kings\nWords of Radiance\nBbbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Русские буквы Kings\nWords of Radiance\nOathbringer\n";
    execute_diff_zip(original, modified);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nBbРусскиеБуквыbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbringeРусскиеБуквыr\n";
    execute_diff_zip(original, modified);

    let original = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nB\u{1F605}bbbbbbb\n";
    let modified = "The Way of Kings\nThe Way of Kings\nThe Way of Kings\nWords of Radiance\nOathbring\u{1F605}er\n";
    execute_diff_zip(original, modified);

}

#[test]
fn test_patch_zip_failure() {

    let patch = "\
@@ -10,6 +10,8 @@
 First:
     Life before death,
     strength before weakness,
     journey before destination.
 Second:
-    I will put the law before all else.
+    I swear to seek justice,
+    to let it guide me,
+    until I find a more perfect Ideal.
";

    let original = "\
First:
    Life before death,
    strength before weakness,
    journey before destination.
Second:
    I will put the law before all else.
";

    let cell = test_cell(original, patch);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH_ZIPQ
        ",
        cell.clone()
    )
    .expect_item(StackItem::None);

    test_case_with_cell(
        "PUSHREFSLICE
        LDREF
        LDREF
        DROP
        DIFF_PATCH_ZIP
        ",
        cell
    )
    .expect_failure(ExceptionCode::UnknownError);

}

#[test]
fn test_diff_similar_lib_panic() {
    let answer = "@@ -1 +1,0 @@\n-\u{18}\n@@ -2,0 +2,2 @@\n+\n+\r";
    let answer_cell = pack_string_to_cell(answer, &mut 0).unwrap();
    test_case(
        "NEWC STSLICECONST x180a0a ENDC
         NEWC STSLICECONST x0a0a0d ENDC
         DIFF"
    )
    .with_capability(GlobalCapabilities::CapDiff)
    .expect_item(StackItem::Cell(answer_cell));
}
