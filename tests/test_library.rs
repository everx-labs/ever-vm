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

use ever_block::{BuilderData, Cell, CellType, HashmapE, UInt256, ExceptionCode, SliceData, IBitstring};
use num::BigInt;
use std::collections::HashSet;
use std::str::FromStr;
use ever_block::{GlobalCapabilities, MerkleProof, SimpleLib, StateInitLib, Serializable};
use ever_assembler::compile_code;
use ever_vm::error::TvmError;
use ever_vm::executor::Engine;
use ever_vm::executor::gas::gas_state::Gas;
use ever_vm::stack::integer::math::utils::divmod;
use ever_vm::stack::integer::math::Round;
use ever_vm::stack::StackItem;

#[test]
fn test_use_library_normal_load_cell_from_ref() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();
    let hash = lib_code.repr_hash();

    let mut code_use_lib = BuilderData::with_raw(vec![2], 8).unwrap();
    code_use_lib.append_raw(hash.as_slice(), 256).unwrap();
    code_use_lib.set_type(CellType::LibraryReference);

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case_with_ref("
        ONE
        PUSHREF
        CTOS
        BLESS
        POP C0
    ", code_use_lib.into_cell().unwrap())
    .with_library(lib)
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .expect_int_stack(&[1, 2]);
}

#[test]
fn test_use_library_normal_compose_cell() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();
    let hash = lib_code.repr_hash();
    assert_eq!(hash, "d816dc4ba685aed03aacac298a2beb6bcd67241e35ddcf39c4020c7430b3cf8f".parse::<UInt256>().unwrap());

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case("
        ONE
        NEWC
        PUSHINT 2
        STUR 8
        PUSHSLICE xd816dc4ba685aed03aacac298a2beb6bcd67241e35ddcf39c4020c7430b3cf8f
        STSLICER
        TRUE
        ENDXC
        CTOS
        BLESS
        POP C0
    ")
    .with_library(lib)
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .expect_int_stack(&[1, 2]);
}

#[test]
fn test_use_library_normal_jmpref() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();
    let hash = lib_code.repr_hash();

    let mut code_use_lib = BuilderData::with_raw(vec![2], 8).unwrap();
    code_use_lib.append_raw(hash.as_slice(), 256).unwrap();
    code_use_lib.set_type(CellType::LibraryReference);

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case_with_ref("
        ONE
    ", code_use_lib.into_cell().unwrap())
    .with_library(lib)
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .expect_int_stack(&[1, 2]);
}

#[test]
fn test_use_library_with_wrong_cell_hash() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();
    let hash = lib_code.repr_hash();

    let mut code_use_lib = BuilderData::with_raw(vec![2], 8).unwrap();
    code_use_lib.append_raw(&[0; 32], 256).unwrap();
    code_use_lib.set_type(CellType::LibraryReference);

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case_with_ref("
        ONE
        PUSHREF
        CTOS
        BLESS
        POP C0
    ", code_use_lib.into_cell().unwrap())
    .with_library(lib)
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .expect_failure(ExceptionCode::CellUnderflow);
}

#[test]
fn test_use_library_with_cell_type_error() {
    let lib_code1 = BuilderData::with_raw(vec![0x71], 8).unwrap().into_cell().unwrap();
    let lib_code2 = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();
    let hash1 = lib_code1.repr_hash();
    let hash2 = lib_code2.repr_hash();

    let mut code_use_lib = BuilderData::with_raw(vec![3], 8).unwrap();
    code_use_lib.append_raw(&[0; 34], 272).unwrap();
    code_use_lib.checked_append_reference(Cell::default()).unwrap();
    code_use_lib.set_type(CellType::MerkleProof);

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash1.into(), &lib_code1).unwrap();
    lib.setref(hash2.into(), &lib_code2).unwrap();

    test_case_with_ref("
        ONE
        PUSHREF
        CTOS
        BLESS
        POP C0
    ", code_use_lib.into_cell().unwrap())
    .with_library(lib)
    .skip_fift_check(true) // this test is not working properly on fift
    .expect_failure(ExceptionCode::CellUnderflow);
}

#[test]
fn test_compose_exotic_cell_normal() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();

    let hash = lib_code.repr_hash();

    let mut code_use_lib = BuilderData::with_raw(vec![2], 8).unwrap();
    code_use_lib.append_raw(hash.as_slice(), 256).unwrap();
    code_use_lib.set_type(CellType::LibraryReference);

    test_case_with_ref("
        PUSHREF
        HASHCU
        NEWC
        PUSHINT 2   ; library reference exotic cell type
        STUR 8
        STU 256
        TRUE
        ENDXC
    ", lib_code)
    .expect_item(StackItem::Cell(code_use_lib.into_cell().unwrap()));
}

#[test]
fn test_compose_exotic_cell_and_load_normal() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();

    let hash = lib_code.repr_hash();

    let mut code_use_lib = BuilderData::default();
    code_use_lib.append_raw(hash.as_slice(), 256).unwrap();

    test_case_with_ref("
        PUSHREF
        HASHCU
        NEWC
        PUSHINT 2   ; library reference exotic cell type
        STUR 8
        STU 256
        TRUE
        ENDXC
        XCTOS
        THROWIFNOT 111
        LDU 8
        SWAP
        TWO
        EQUAL
        THROWIFNOT 112
    ", lib_code)
    .expect_item(StackItem::Slice(SliceData::load_builder(code_use_lib).unwrap()))
    .expect_gas(1000000000, 1000000000, 0, 999999037);
}

#[test]
fn test_incorrect_library() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();

    let hash = UInt256::from_str("0xd816dc4ba685aed03aacac298a2beb6bcd67241e35ddcf39c4020c7430b3cf80").unwrap();

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case_with_ref("
        PUSHREF
        PUSHSLICE xd816dc4ba685aed03aacac298a2beb6bcd67241e35ddcf39c4020c7430b3cf80
        NEWC
        PUSHINT 2   ; library reference exotic cell type
        STUR 8
        STSLICE
        TRUE
        ENDXC
        CTOS
    ", lib_code)
        .with_capability(GlobalCapabilities::CapSetLibCode)
        .with_library(lib)
        .expect_failure(ExceptionCode::DictionaryError);
}

#[test]
fn test_merkle_proof_cell() {
    let merkle_data = BuilderData::with_raw(vec![0x80; 36], 288 - 8 - 8).unwrap().into_cell().unwrap();
    let mut v = vec![0x03];
    v.append(&mut vec![0x80; 36]);
    let merkle_data_full = BuilderData::with_raw_and_refs(v, 280, vec![Cell::default()]).unwrap().into_cell().unwrap();

    test_case_with_ref("
        PUSHREF
        CTOS

        NEWC
        ENDC

        NEWC
        STREF
        PUSHINT 3   ; MerkleProof exotic cell type
        STUR 8
        STSLICE
        TRUE
        ENDXC

        XCTOS
        THROWIFNOT 111
    ", merkle_data)
        .expect_item(StackItem::Slice(SliceData::load_cell(merkle_data_full).unwrap()))
        .expect_gas(1000000000, 1000000000, 0, 999998513);

    let code = "
        NEWC
        PUSHINT 1
        STUR 8
        ENDC
        DUP
        HASHCU

        NEWC
        PUSHINT 3   ; MerkleProof exotic cell type
        STUR 8
        STU 256
        ZERO
        STUR 16
        STREF
        TRUE
        ENDXC

        CTOS
        LDU 8
        ENDS
    ";

    expect_exception(code, ExceptionCode::CellUnderflow);

    test_case(code)
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_int_stack(&[1])
        .expect_gas(1000000000, 1000000000, 0, 999998391);

    let code = "
        NEWC
        PUSHINT 1
        STUR 8
        ENDC

        NEWC
        PUSHINT 3   ; MerkleProof exotic cell type
        STUR 8
        ZERO
        STUR 256
        ZERO
        STUR 16
        STREF
        TRUE
        ENDXC

        CTOS
        LDU 8
        ENDS
    ";

    test_case(code)
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_failure(ExceptionCode::CellUnderflow)
        .expect_gas(1000000000, 1000000000, 0, 999998508);

}

#[test]
fn test_load_merkle_tree() {
    let c1 = BuilderData::with_raw(vec![0x01], 8).unwrap().into_cell().unwrap();
    let c2 = BuilderData::with_raw(vec![0x02], 8).unwrap().into_cell().unwrap();
    let c3 = BuilderData::with_raw(vec![0x03], 8).unwrap().into_cell().unwrap();
    let c4 = BuilderData::with_raw_and_refs(vec![0x04], 8, [c1, c2]).unwrap().into_cell().unwrap();
    let hash3 = c3.repr_hash();
    // let hash4 = c4.repr_hash();
    let root = BuilderData::with_raw_and_refs(vec![0x05], 8, [c4, c3]).unwrap().into_cell().unwrap();
    let hash5 = root.repr_hash();

    // let mut include = HashSet::<UInt256>::default();
    let merkle = MerkleProof::create_with_subtrees(
        &root,
        |hash| hash == &hash3,
        |hash| hash == &hash5,
    ).unwrap();
    let merkle = merkle.serialize().unwrap();

    println!("{:#.100}", root);
    println!("{:#.100}", merkle);

    test_case_with_ref("
        PUSHREF
        DUP
        CDEPTH
        SWAP
        CTOS
        DUP
        SDEPTH
        SWAP
        PLDREFIDX 0
        CDEPTH
        ", merkle)
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_int_stack(&[2, 2, 1])
        .expect_success();
}

#[test]
fn test_complex_merkle_cells() {
    let mut include = HashSet::<UInt256>::default();
    let cell1 = BuilderData::with_raw(vec![11], 8).unwrap().into_cell().unwrap();
    let cell2 = BuilderData::with_raw(vec![12], 8).unwrap().into_cell().unwrap();
    let cell3 = BuilderData::with_raw(vec![13], 8).unwrap().into_cell().unwrap();
    let cell4 = BuilderData::with_raw_and_refs(vec![43], 8, vec![cell3]).unwrap().into_cell().unwrap();
    include.insert(cell1.repr_hash());
    include.insert(cell2.repr_hash());
    let cell12 = BuilderData::with_raw_and_refs(vec![14], 8, vec![cell1, cell2]).unwrap().into_cell().unwrap();
    include.insert(cell12.repr_hash());
    let cell = BuilderData::with_raw_and_refs(vec![111], 8, vec![cell4, cell12]).unwrap().into_cell().unwrap();
    include.insert(cell.repr_hash());

    let merkle_proof = MerkleProof::create_with_subtrees(
        &cell,
        |hash| include.get(hash).is_some(),
        |hash| include.get(hash).is_some(),
    ).unwrap();

    let merkle_cell = merkle_proof.serialize().unwrap();

    println!("{:#.100}", merkle_cell);

    // cannot resolve without capability
    test_case_with_ref("PUSHREFSLICE", merkle_cell.clone())
        .expect_failure(ExceptionCode::CellUnderflow);

    // check if it is a merkle cell
    test_case_with_ref("
        PUSHREF
        XCTOS
        SWAP
        LDU 8
        DROP
    ", merkle_cell.clone())
        .expect_int_stack(&[-1, 3]);

    // try to load pruned cell
    test_case_with_ref("
        PUSHREFSLICE
        LDREFRTOS
    ", merkle_cell.clone())
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_failure(ExceptionCode::CellUnderflow);

    // skip pruned cell - load only valid cells
    test_case_with_ref("
        PUSHREFSLICE
        LDU 8 ; 111 in root cell
        LDREF
        NIP
        LDREFRTOS
        SWAP
        ENDS
        LDU 8 ; it sould be 14
        LDREFRTOS
        LDU 8 ; it sould be 11
        ENDS
        SWAP
        LDREFRTOS
        LDU 8 ; it sould be 12
        ENDS
        SWAP
        ENDS
    ", merkle_cell)
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_int_stack(&[111, 14, 11, 12]);
}

#[test]
fn test_merkle_update_cell() {
    let merkle_data = BuilderData::with_raw(vec![0x80; 70], 70 * 8 - 8 - 8).unwrap().into_cell().unwrap();
    let mut v = vec![0x04];
    v.append(&mut vec![0x80; 70]);
    let merkle_data_full = BuilderData::with_raw_and_refs(v, 70 * 8 - 8, vec![Cell::default(); 2]).unwrap().into_cell().unwrap();

    test_case_with_ref("
        PUSHREF
        CTOS

        NEWC
        ENDC
        NEWC
        ENDC

        NEWC
        STREF
        STREF
        PUSHINT 4   ; MerkleUpdate exotic cell type
        STUR 8
        STSLICE
        TRUE
        ENDXC

        XCTOS
        THROWIFNOT 111
    ", merkle_data)
        .expect_item(StackItem::Slice(SliceData::load_cell(merkle_data_full).unwrap()))
        .expect_gas(1000000000, 1000000000, 0, 999997959);

    let code = "
        NEWC
        PUSHINT 2
        STUR 8
        ENDC
        DUP
        HASHCU

        NEWC
        PUSHINT 1
        STUR 8
        ENDC
        DUP
        HASHCU

        NEWC
        PUSHINT 4   ; MerkleUpdate exotic cell type
        STUR 8
        STU 256
        ZERO
        STUR 16
        STREF
        STU 256
        ZERO
        STUR 16
        STREF
        TRUE
        ENDXC

        CTOS
        LDU 8
        ENDS
    ";

    expect_exception(code, ExceptionCode::CellUnderflow);

    test_case(code)
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_int_stack(&[2])
        .expect_gas(1000000000, 1000000000, 0, 999997663);

        let code = "
        NEWC
        PUSHINT 2
        STUR 8
        ENDC

        NEWC
        PUSHINT 1
        STUR 8
        ENDC

        NEWC
        PUSHINT 4   ; MerkleUpdate exotic cell type
        STUR 8
        ZERO
        STUR 256
        ZERO
        STUR 16
        STREF
        ZERO
        STUR 256
        ZERO
        STUR 16
        STREF
        TRUE
        ENDXC

        CTOS
        LDU 8
        ENDS
    ";

    expect_exception(code, ExceptionCode::CellUnderflow);

    test_case(code)
        .with_capability(GlobalCapabilities::CapResolveMerkleCell)
        .expect_failure(ExceptionCode::CellUnderflow)
        .expect_gas(1000000000, 1000000000, 0, 999997798);

}

#[test]
fn test_compose_exotic_cell_wrong_type() {
    expect_exception("ONE TRUE ENDXC", ExceptionCode::TypeCheckError);
    expect_exception("NIL TRUE ENDXC", ExceptionCode::TypeCheckError);
    expect_exception("NULL TRUE ENDXC", ExceptionCode::TypeCheckError);
    expect_exception("PUSHCONT {} TRUE ENDXC", ExceptionCode::TypeCheckError);
    expect_exception("PUSHSLICE x12 TRUE ENDXC", ExceptionCode::TypeCheckError);
    expect_exception("NEWC ENDC TRUE ENDXC", ExceptionCode::TypeCheckError);
}

#[test]
fn test_compose_exotic_cell_wrong_cell_format() {
    expect_exception("
        NEWC
        TRUE
        ENDXC
    ", ExceptionCode::CellOverflow);

    expect_exception("
        NEWC
        TWO
        STUR 8
        TRUE
        ENDXC
    ", ExceptionCode::CellOverflow);
}

#[test]
fn test_compose_exotic_cell_and_load_as_cell() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();

    let hash = lib_code.repr_hash();

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case_with_ref("
        PUSHREF
        HASHCU
        NEWC
        PUSHINT 2   ; library reference exotic cell type
        STUR 8
        STU 256
        TRUE
        ENDXC
        XLOAD
    ", lib_code.clone())
    .with_library(lib)
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .expect_item(StackItem::Cell(lib_code));
}

#[test]
fn test_compose_exotic_cell_and_load_quite_as_cell() {
    let lib_code = BuilderData::with_raw(vec![0x72], 8).unwrap().into_cell().unwrap();

    let hash = lib_code.repr_hash();

    let mut lib = HashmapE::with_bit_len(256);
    lib.setref(hash.into(), &lib_code).unwrap();

    test_case_with_ref("
        PUSHREF
        HASHCU
        NEWC
        PUSHINT 2   ; library reference exotic cell type
        STUR 8
        STU 256
        TRUE
        ENDXC
        XLOADQ
        THROWIFNOT 100
    ", lib_code.clone())
    .with_library(lib)
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .expect_item(StackItem::Cell(lib_code));
}

#[test]
fn test_load_exotic_cell_as_cell_wrong_type() {
    expect_exception("ONE XLOAD", ExceptionCode::TypeCheckError);
    expect_exception("NIL XLOAD", ExceptionCode::TypeCheckError);
    expect_exception("NULL XLOAD", ExceptionCode::TypeCheckError);
    expect_exception("NEWC XLOAD", ExceptionCode::TypeCheckError);
    expect_exception("PUSHCONT {} XLOAD", ExceptionCode::TypeCheckError);
    expect_exception("PUSHSLICE x12 XLOAD", ExceptionCode::TypeCheckError);

    expect_exception("ONE XLOADQ", ExceptionCode::TypeCheckError);
    expect_exception("NIL XLOADQ", ExceptionCode::TypeCheckError);
    expect_exception("NULL XLOADQ", ExceptionCode::TypeCheckError);
    expect_exception("NEWC XLOADQ", ExceptionCode::TypeCheckError);
    expect_exception("PUSHCONT {} XLOADQ", ExceptionCode::TypeCheckError);
    expect_exception("PUSHSLICE x12 XLOADQ", ExceptionCode::TypeCheckError);
}

#[test]
fn test_compose_exotic_cell_wrong_cell_type() {
    test_case("
        NEWC
        PUSHINT 0
        STUR 8
        TRUE
        ENDXC
    ")
    .expect_failure(ExceptionCode::CellOverflow);

    test_case("
        NEWC
        PUSHINT 5
        STUR 8
        TRUE
        ENDXC
    ")
    .expect_failure(ExceptionCode::CellOverflow);
}

fn test_bigint_div(dividend: &str, divisor: &str, round_mode: Round, quot_ans: &str, remainder_ans: &str) {
    let dividend = BigInt::from_str(dividend).unwrap();
    let divisor = BigInt::from_str(divisor).unwrap();

    let (quot, remainder) = divmod(&dividend, &divisor, round_mode);

    let quot_ans = BigInt::from_str(quot_ans).unwrap();
    let remainder_ans = BigInt::from_str(remainder_ans).unwrap();

    assert_eq!(quot, quot_ans);
    assert_eq!(remainder, remainder_ans);
}

#[test]
fn tests_bigint_div() {
    test_bigint_div("1000", "9", Round::Ceil, "112", "-8");
    test_bigint_div("1000000000000000000", "9000000000000000", Round::Ceil, "112", "-8000000000000000");
    test_bigint_div("-1000", "9", Round::Ceil, "-111", "-1");
    test_bigint_div("-1000000000000000000", "9000000000000000", Round::Ceil, "-111", "-1000000000000000");
    test_bigint_div("1000", "-9", Round::Ceil, "-111", "1");
    test_bigint_div("1000000000000000000", "-9000000000000000", Round::Ceil, "-111", "1000000000000000");
    test_bigint_div("-1000", "-9", Round::Ceil, "112", "8");
    test_bigint_div("-1000000000000000000", "-9000000000000000", Round::Ceil, "112", "8000000000000000");

    test_bigint_div("1000", "9", Round::Nearest, "111", "1");
    test_bigint_div("1000000000000000000", "9000000000000000", Round::Nearest, "111", "1000000000000000");
    test_bigint_div("-1000", "9", Round::Nearest, "-111", "-1");
    test_bigint_div("-1000000000000000000", "9000000000000000", Round::Nearest, "-111", "-1000000000000000");
    test_bigint_div("1000", "-9", Round::Nearest, "-111", "1");
    test_bigint_div("1000000000000000000", "-9000000000000000", Round::Nearest, "-111", "1000000000000000");
    test_bigint_div("-1000", "-9", Round::Nearest, "111", "-1");
    test_bigint_div("-1000000000000000000", "-9000000000000000", Round::Nearest, "111", "-1000000000000000");

    test_bigint_div("5000000000000000000", "2000000000000000000", Round::Nearest, "3", "-1000000000000000000");
    test_bigint_div("-5000000000000000000", "2000000000000000000", Round::Nearest, "-2", "-1000000000000000000");
    test_bigint_div("5000000000000000000", "-2000000000000000000", Round::Nearest, "-2", "1000000000000000000");
    test_bigint_div("-5000000000000000000", "-2000000000000000000", Round::Nearest, "3", "1000000000000000000");

    test_bigint_div("1000", "9", Round::FloorToNegativeInfinity, "111", "1");
    test_bigint_div("1000000000000000000", "9000000000000000", Round::FloorToNegativeInfinity, "111", "1000000000000000");
    test_bigint_div("-1000", "9", Round::FloorToNegativeInfinity, "-112", "8");
    test_bigint_div("-1000000000000000000", "9000000000000000", Round::FloorToNegativeInfinity, "-112", "8000000000000000");
    test_bigint_div("1000", "-9", Round::FloorToNegativeInfinity, "-112", "-8");
    test_bigint_div("1000000000000000000", "-9000000000000000", Round::FloorToNegativeInfinity, "-112", "-8000000000000000");
    test_bigint_div("-1000", "-9", Round::FloorToNegativeInfinity, "111", "-1");
    test_bigint_div("-1000000000000000000", "-9000000000000000", Round::FloorToNegativeInfinity, "111", "-1000000000000000");

    test_bigint_div("303424019600764000", "67374462762615477834925", Round::Nearest, "0", "303424019600764000");
    test_bigint_div("3034724019600764000", "67374462762615477834925", Round::Nearest, "0", "3034724019600764000");
    test_bigint_div("30934724019600764000", "67374462762615477834925", Round::Nearest, "0", "30934724019600764000");
    test_bigint_div("30934724401965080764000", "67374462762615477834925", Round::Nearest, "0", "30934724401965080764000");
    test_bigint_div("309347247401965080764000", "67374462762615477834925", Round::Nearest, "5", "-27525066411112308410625");
    test_bigint_div("2309347247401965080764000", "67374462762615477834925", Round::Nearest, "34", "18615513473038834376550");
    test_bigint_div("23093472474019650810764000", "67374462762615477834925", Round::Nearest, "343", "-15968253557458086615275");
    test_bigint_div("23093472474019650810764000", "68374492762615427834911", Round::Nearest, "338", "-17106079744363797435918");
    test_bigint_div("23093472474019650810764000", "18374592262615427834517", Round::Nearest, "1257", "-3390000087941977223869");
    test_bigint_div(
        "2573300903069472664743019426508341031766400380", "1858237458809226201617542047938488403458417",
        Round::Nearest, "1385", "-357977381305624497276309886465407023507165");
    test_bigint_div(
        "25733009030694726647744301942650558341009131766400380", "1858237458809226201617534420474793848840393458417",
        Round::Nearest, "13848", "136701104562207744685287915613122267363154241764");
    test_bigint_div(
        "2530090306947266477443019426505584100931766400380", "1858237458809226201617534420474793848840393458417",
        Round::Nearest, "1", "671852848138040275825485006030790252091372941963");
    test_bigint_div(
        "253009030694726647743019426505584100931766400380", "1858237458809226201617534420474793848840393458417",
        Round::Nearest, "0", "253009030694726647743019426505584100931766400380");
    test_bigint_div(
        "25300903069472664774301942650558410093176640038", "1858237458809226201617534420474793848840393458417",
        Round::Nearest, "0", "25300903069472664774301942650558410093176640038");
    test_bigint_div("253009030698", "1858237458809226201617534420474793848840393458417", Round::Nearest, "0", "253009030698");
    test_bigint_div(
        "19251874299181659011971842759180", "35863987523541299551572269966",
        Round::Nearest, "537", "-7087000960018847222466212562");
    test_bigint_div(
        "15313291547689795475496512878911", "20557862036838502455082",
        Round::Nearest, "744887358", "8940664709053968225555");
    test_bigint_div(
        "12381583265651812264166510594403", "98847588943634081944421359",
        Round::Nearest, "125259", "33122161150793890235587422");
    //test_bigint_div("12381583265651812264166510594403", "0", Round::Nearest, "NaN", "NaN"); panic
    test_bigint_div(
        "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        "115792089237316195423570985008687907853269984665640564039457584007913129639934",
        Round::Nearest, "1", "1");
    test_bigint_div(
        "156", "115792089237316195423570985008687907853269984665640564039457584007913129639934",
        Round::Nearest, "0", "156");
    test_bigint_div(
        "115792089237316195423570985008687907853269984665640564039457584007913129639935", "156",
        Round::Nearest, "742256982290488432202378109030050691367115286318208743842676820563545702820", "15");
}

fn make_recursive_code_and_library(count_libs: u64) -> (SliceData, HashmapE) {
    let mut hash = UInt256::new();
    let mut cell = Cell::default();
    let mut library = StateInitLib::default();
    for _ in 0..count_libs {
        let mut b = BuilderData::default();
        b.set_type(CellType::LibraryReference);
        b.append_i8(2).unwrap();
        b.append_bytestring(&hash.clone().into()).unwrap();
        cell = b.clone().into_cell().unwrap();
        hash = cell.repr_hash();

        library.set(&hash, &SimpleLib::new(cell.clone(), false)).unwrap();
    }
    let library = library.inner();

    let bytecode = compile_code("").unwrap();

    let mut builder = bytecode.into_builder();
    builder.checked_prepend_reference(cell).unwrap();
    let code = SliceData::load_builder(builder).unwrap();

    (code, library)
}

#[test]
fn recursive_load_cell() {
    let (code, library) = make_recursive_code_and_library(50000);

    let mut executor = Engine::with_capabilities(GlobalCapabilities::CapSetLibCode as u64)
        .setup_with_libraries(
            code,
            None,
            None,
            Some(Gas::test_with_limit(3000000)),
            vec![library]
        );
    let err = executor.execute();
    if let Some(TvmError::TvmExceptionFull(e, _msg)) = err.err().unwrap().downcast_ref() {
        assert_eq!(e.exception_code().unwrap(), ExceptionCode::OutOfGas);
    } else {
        unreachable!()
    }
}

#[test]
fn recursive_load_cell_without_cap() {
    let (code, library) = make_recursive_code_and_library(3000);

    let mut executor = Engine::with_capabilities(0)
        .setup_with_libraries(
            code,
            None,
            None,
            Some(Gas::test_with_limit(10000)),
            vec![library]
        );
    let err = executor.execute();
    if let Some(TvmError::TvmExceptionFull(e, _msg)) = err.err().unwrap().downcast_ref() {
        assert_eq!(e.exception_code().unwrap(), ExceptionCode::InvalidOpcode);
    } else {
        unreachable!()
    }
}
