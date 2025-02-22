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

use ever_assembler::compile_code_to_cell;
use ever_block::{
    ed25519_generate_private_key, types::ExceptionCode, AccountId, BuilderData, Cell,
    GlobalCapabilities, HashmapE, HashmapType, IBitstring, MsgAddressInt, Result, Serializable,
    Sha256, SliceData,
    ACTION_CHANGE_LIB, ACTION_COPYLEFT, ACTION_RESERVE, ACTION_SEND_MSG, ACTION_SET_CODE,
    ED25519_PUBLIC_KEY_LENGTH, ED25519_SIGNATURE_LENGTH,
};
#[cfg(feature = "signature_no_check")]
use ever_vm::executor::BehaviorModifiers;
use ever_vm::{
    boolean,
    executor::serialize_currency_collection,
    int,
    stack::{
        integer::{
            serialization::{Encoding, UnsignedIntegerBigEndianEncoding},
            IntegerData,
        },
        serialization::{Deserializer, Serializer},
        Stack, StackItem,
    },
    SmartContractInfo,
};

mod common;
use common::*;
use rand::RngCore;

fn gen_test_tree_of_cells() -> Cell {
    let mut random = rand::thread_rng();
    let mut buffer = [0u8; 127];
    //test cell with data and one not empty reference
    let mut builder = BuilderData::new();
    random.fill_bytes(&mut buffer[..]);
    builder.append_raw(&buffer, buffer.len() * 8).unwrap();
    let mut ref0 = BuilderData::new();
    random.fill_bytes(&mut buffer[..]);
    ref0.append_raw(&buffer, buffer.len() * 8).unwrap();
    builder.checked_append_reference(ref0.into_cell().unwrap()).unwrap();
    builder.into_cell().unwrap()
}

#[test]
fn test_chksignu_real() {
    let pair = ed25519_generate_private_key().unwrap();

    //test cell with data and one not empty reference
    let test_cell = gen_test_tree_of_cells();
    let cell_hash = test_cell.repr_hash();

    //sign hash of data cell
    let signature = pair.sign(cell_hash.as_slice()).to_vec();

    //put signature to separate slice
    let len = signature.len() * 8;
    let signature = SliceData::from_raw(signature, len);

    //put public key to integer
    let pub_key = BuilderData::with_raw(
        pair.verifying_key().to_vec(),
        ED25519_PUBLIC_KEY_LENGTH * 8
    ).unwrap();

    //put hash to integer
    let hash = BuilderData::with_raw(cell_hash.as_slice().to_vec(), 256).unwrap();

    test_case_with_refs("
        PUSHREFSLICE
        PLDU 256
        PUSHREFSLICE
        PUSHREFSLICE
        PLDU 256
        NOP
        ;s0 - pub key: integer
        ;s1 - signature: slice
        ;s2 - hash: integer
        CHKSIGNU
    ", vec![hash.into_cell().unwrap(), signature.into_cell(), pub_key.into_cell().unwrap()])
    .expect_stack(Stack::new().push(int!(-1)));

    test_case("
        PUSHINT 66217541034200756890641849847588029095699779625619746207976976137706939289808
        PUSHSLICE xfb53f9005a9e7c91c7dc8fcaeecb2dd0d5af17703042cf4daf0c7ec7bc1da281e4f0b3c748bace798548e65697f52968848d830f6015c0709d8fad51d421c304
        PUSHINT 15336109783281190428388939426462642574584905613548735486866417552072882909493
        CHKSIGNU
    ")
    .expect_stack(Stack::new().push(int!(-1)));

}

#[cfg(feature = "signature_no_check")]
#[test]
fn test_chksignu_always() {
    let pair = ed25519_generate_private_key().unwrap();

    //test cell with data and one not empty reference
    let test_cell = gen_test_tree_of_cells();
    let cell_hash = test_cell.repr_hash();

    //fake signature of data cell
    let signature = SliceData::from_raw(vec![0; 64], 512).into_cell();

    //put public key to integer
    let pub_key = SliceData::from_raw(
        pair.verifying_key().to_vec(),
        ED25519_PUBLIC_KEY_LENGTH * 8
    ).into_cell();

    //put hash to integer
    let hash = SliceData::from_raw(cell_hash.as_slice().to_vec(), 256).into_cell();

    let modifiers = BehaviorModifiers {
        chksig_always_succeed: true
    };

    let code = "
        PUSHREFSLICE
        PLDU 256
        PUSHREFSLICE
        PUSHREFSLICE
        PLDU 256
        NOP
        ;s0 - pub key: integer
        ;s1 - signature: slice
        ;s2 - hash: integer
        CHKSIGNU
    ";

    test_case_with_refs(code, vec![hash.clone(), signature.clone(), pub_key.clone()])
    .expect_stack(Stack::new().push(int!(0)));

    test_case_with_refs(code, vec![hash, signature, pub_key])
    .with_behavior_modifiers(modifiers.clone())
    .expect_stack(Stack::new().push(int!(-1)));

    test_case("
        PUSHINT 66217541034200756890641849847588029095699779625619746207976976137706939289808
        PUSHSLICE xfb53f9005a9e7c91c7dc8fcaeecb2dd0d5af17703042cf4daf0c7ec7bc1da281e4f0b3c748bace798548e65697f52968848d830f6015c0709d8fad51d421c304
        PUSHINT 0
        CHKSIGNU
    ")
    .expect_stack(Stack::new().push(int!(0)));

    test_case("
        PUSHINT 66217541034200756890641849847588029095699779625619746207976976137706939289808
        PUSHSLICE xfb53f9005a9e7c91c7dc8fcaeecb2dd0d5af17703042cf4daf0c7ec7bc1da281e4f0b3c748bace798548e65697f52968848d830f6015c0709d8fad51d421c304
        PUSHINT 0
        CHKSIGNU
    ")
    .with_behavior_modifiers(modifiers)
    .expect_stack(Stack::new().push(int!(-1)));

}

#[test]
fn test_chksigns_underflow() {
    test_case("
        PUSHSLICE x00
        PUSHSLICE x00
        PUSHINT 0
        CHKSIGNS
    ")
    .expect_failure(ExceptionCode::CellUnderflow);

    test_case("
        PUSHSLICE x01_
        PUSHSLICE x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
        PUSHINT 0
        CHKSIGNS
    ")
    .expect_failure(ExceptionCode::CellUnderflow);
}

fn generate_tree_and_hash() -> (Cell, IntegerData) {
    let test_cell = gen_test_tree_of_cells();

    let hash_int = UnsignedIntegerBigEndianEncoding::new(256)
        .deserialize(test_cell.repr_hash().as_slice());

    (test_cell, hash_int)
}

fn generate_tree_and_sha256() -> (Cell, IntegerData) {
    let test_slice = gen_test_tree_of_cells();
    let mut hasher = Sha256::new();
    hasher.update(test_slice.data());
    let sha256_hash = UnsignedIntegerBigEndianEncoding::new(256)
        .deserialize(hasher.finalize().as_ref());

    (test_slice, sha256_hash)
}

fn call_hash_primitive(code: &str) {
    for _ in 0..10 {
        let (slice, hash) = generate_tree_and_hash();
        test_case_with_ref(code, slice)
            .expect_item(StackItem::int(hash));
    }
}

fn call_sha256u(code: &str){
    for _ in 0..10 {
        let (slice, hash) = generate_tree_and_sha256();
        test_case_with_ref(code, slice)
                .expect_item(StackItem::int(hash));
    }
}

#[test]
fn test_hashcu() {
    call_hash_primitive("
        PUSHREF
        HASHCU
    ");
}

#[test]
fn test_hashsu() {
    call_hash_primitive("
        PUSHREFSLICE
        HASHSU
    ");
}

#[test]
fn test_sha256u(){
    call_sha256u("
        PUSHREFSLICE
        SHA256U
    ");
}

#[test]
fn test_sha256u_cell_underflow(){
    expect_exception("PUSHINT 5 NEWC STI 7 ENDC CTOS SHA256U",
        ExceptionCode::CellUnderflow);
    expect_exception("PUSHSLICE xFF_ SHA256U", ExceptionCode::CellUnderflow);
    expect_exception("PUSHSLICE x05_ SHA256U", ExceptionCode::CellUnderflow);
}

#[test]
fn test_sha256u_stack_underflow(){
    expect_exception("SHA256U", ExceptionCode::StackUnderflow);
    expect_exception("STSLICECONST x05_ SHA256U", ExceptionCode::StackUnderflow);
}

#[test]
fn test_sha256u_type_error(){
    expect_exception("PUSHINT 5 SHA256U", ExceptionCode::TypeCheckError)
}

fn call_chksignu(hash_primitive: &str, push_primitive: &str) {
    let (slice, hash) = generate_tree_and_hash();
    let pair = ed25519_generate_private_key().unwrap();

    let cell = SliceData::load_builder(UnsignedIntegerBigEndianEncoding::new(256)
        .try_serialize(&hash).unwrap()
    ).unwrap();

    let hash_bytes = cell.get_bytestring(0);
    //sign hash of tree of cells
    let signature = pair.sign(&hash_bytes).to_vec();
    let signature = BuilderData::with_raw(
        signature,
        ED25519_SIGNATURE_LENGTH * 8
    ).unwrap();
    let key_slice = BuilderData::with_raw(
        pair.verifying_key().to_vec(),
        ED25519_PUBLIC_KEY_LENGTH * 8
    ).unwrap().into_cell().unwrap();

    test_case_with_refs(&format!("
        PUSHREFSLICE
        PUSHREFSLICE
        {push_primitive}
        NOP
        {hash_primitive}
        XCHG s2
        LDU 256
        ENDS
        CHKSIGNU
    ", push_primitive = push_primitive,
       hash_primitive = hash_primitive),
       vec![key_slice, signature.into_cell().unwrap(), slice]
    ).expect_item(int!(-1));
}

#[test]
fn test_chksignu_error() {
    expect_exception("CHKSIGNU", ExceptionCode::StackUnderflow);
    expect_exception("NULL CHKSIGNU", ExceptionCode::StackUnderflow);
    expect_exception("NULL NULL CHKSIGNU", ExceptionCode::StackUnderflow);

    expect_exception("
        PUSHINT 123456
        PUSHSLICE x123
        PUSHINT 987654
        CHKSIGNU
    ", ExceptionCode::CellUnderflow);

    expect_exception("
        NULL
        PUSHSLICE x123
        PUSHINT 987654
        CHKSIGNU
    ", ExceptionCode::TypeCheckError);

    expect_exception("
        PUSHINT 123456
        NULL
        PUSHINT 987654
        CHKSIGNU
    ", ExceptionCode::TypeCheckError);

    expect_exception("
        PUSHINT 123456
        PUSHSLICE x123
        NULL
        CHKSIGNU
    ", ExceptionCode::TypeCheckError);
}

#[test]
fn test_chksignu_bad_slice() {
    let signature = SliceData::new(vec![0xAA; 65]).into_cell();
    let code = "
        PUSHINT 1234567
        PUSHREFSLICE
        PUSHINT 987654
        CHKSIGNU
    ";

    test_case_with_ref(code, signature.clone())
        .expect_item(boolean!(false));

    test_case_with_ref(code, signature)
        .with_capability(GlobalCapabilities::CapsTvmBugfixes2022)
        .expect_item(boolean!(false));
}

#[test]
fn test_chksignu_bad_pubkey() {
    let code = "
        PUSHINT 1234567
        PUSHSLICE x0000000000000000000000000000000000000000000000000000000000bc614e00000000000000000000000000000000000000000000000000000000075bcd15
        PUSHINT 123456
        CHKSIGNU
    ";

    test_case(code)
        .with_capability(GlobalCapabilities::CapsTvmBugfixes2022)
        .expect_item(boolean!(false));
}

#[test]
fn test_hashcu_and_chksign() {
    call_chksignu("HASHCU", "PUSHREF");
}

#[test]
fn test_hashsu_and_chksign() {
    call_chksignu("HASHSU", "PUSHREFSLICE");
}


#[test]
fn test_sendrawmsg_stackunderflow() {
    expect_exception("PUSHINT 0 SENDRAWMSG", ExceptionCode::StackUnderflow);
    expect_exception("SENDRAWMSG", ExceptionCode::StackUnderflow);
    expect_exception("NEWC ENDC SENDRAWMSG", ExceptionCode::StackUnderflow);
}

#[test]
fn test_sendrawmsg_too_big_int() {
    expect_exception(
       "NEWC
        ENDC
        PUSHINT 256
        SENDRAWMSG",
        ExceptionCode::RangeCheckError,
    );
}

#[test]
fn test_sendrawmsg_wrong_argument_order() {
    expect_exception(
       "PUSHINT 0
        NEWC
        ENDC
        SENDRAWMSG",
        ExceptionCode::TypeCheckError,
    );
}

#[test]
fn test_two_sendrawmsg_with_parsing() {
    test_case(format!(
        "
        ; init c5 register with empty cell (by spec)
        NEWC
        ENDC
        POPCTR c5

        ; create fake msg cell
        PUSHINT 12345
        NEWC
        STU 32
        ENDC
        PUSHINT 99
        SENDRAWMSG

        ; create another fake msg cell
        PUSHINT 67890
        NEWC
        STU 32
        ENDC
        PUSHINT 255
        SENDRAWMSG

        ; check:
        ; c5 =  cell (tag(u32) + mode(u8))
        ;       cell.ref0 - cell with prev action
        ;       cell.ref1 - cell with msg
        PUSHCTR c5
        CTOS

        LDREF       ; load prev action cell
        LDREF       ; load msg cell
        LDU 32      ; load tag
        LDU 8       ; load mode
        ENDS

        PUSHINT 255
        EQUAL
        THROWIFNOT 100

        PUSHINT {tag}
        EQUAL
        THROWIFNOT 100

        ; parse msg
        CTOS
        LDU 32          ; load int from fake msg
        ENDS
        PUSHINT 67890
        EQUAL
        THROWIFNOT 100

        ; check tag and mode in prev action
        CTOS
        LDREF       ; load prev action cell
        LDREF       ; load msg cell
        LDU 32      ; load tag
        LDU 8       ; load mode
        ENDS

        PUSHINT 99
        EQUAL
        THROWIFNOT 100

        PUSHINT {tag}
        EQUAL
        THROWIFNOT 100

        CTOS
        LDU 32
        ENDS
        PUSHINT 12345
        EQUAL
        THROWIFNOT 100

        CTOS
        SEMPTY
        THROWIFNOT 100
        ",
        tag = ACTION_SEND_MSG //4.4.11 blockchain spec
    )).expect_success();
}

#[test]
fn test_rawreserve_with_parsing() {
    let reserved_grams = 123456789u128;
    let flags = 3u8;
    let mut out_actions = BuilderData::new();
    out_actions
        .append_u32(ACTION_RESERVE)
        .and_then(|b| b.append_u8(flags))
        .and_then(|b| b.append_builder(
            &serialize_currency_collection(reserved_grams, None).unwrap()
        )).unwrap();
    out_actions.checked_append_reference(Cell::default()).unwrap();

    test_case(format!("
        PUSHINT {}
        PUSHINT {}
        RAWRESERVE
        PUSHCTR c5
        ", reserved_grams, flags))
    .expect_item(StackItem::Cell(out_actions.into_cell().unwrap()));
}

#[test]
fn test_rawreservex_with_parsing() {
    let reserved_grams = 123456789u128;
    let mut other = HashmapE::with_bit_len(32);
    let key = BuilderData::new().append_u32(1).unwrap().clone();
    let value = BuilderData::new().append_u128(0).unwrap().append_u128(100).unwrap().clone();
    other.set_builder(SliceData::load_builder(key).unwrap(), &value).unwrap();
    let key = BuilderData::new().append_u32(2).unwrap().clone();
    let value = BuilderData::new().append_u128(0).unwrap().append_u128(200).unwrap().clone();
    other.set_builder(SliceData::load_builder(key).unwrap(), &value).unwrap();
    let currency = &serialize_currency_collection(reserved_grams, other.data().cloned()).unwrap();
    let flags = 3u8;
    let mut out_actions = BuilderData::new();
    out_actions.checked_append_reference(Cell::default()).unwrap();
    out_actions
        .append_u32(ACTION_RESERVE)
        .and_then(|b| b.append_u8(flags))
        .and_then(|b| b.append_builder(currency))
        .unwrap();

    test_case_with_ref(&format!("
        PUSHINT {}
        PUSHREF
        PUSHINT {}
        RAWRESERVEX
        PUSHCTR c5
        ", reserved_grams, flags), currency.references()[0].clone())
    .expect_item(StackItem::Cell(out_actions.into_cell().unwrap()));
}

#[test]
fn test_setcode_with_parsing() {
    let code = compile_code_to_cell("PUSHINT 1").unwrap();
    let mut out_actions = BuilderData::new();
    out_actions.append_u32(ACTION_SET_CODE).unwrap();
    out_actions.checked_append_reference(Cell::default()).unwrap();
    out_actions.checked_append_reference(code.clone()).unwrap();

    test_case("
        PUSHREF
        SETCODE
        PUSHCTR c5
    ")
    .with_ref(code)
    .expect_item(StackItem::Cell(out_actions.into_cell().unwrap()));
}

#[test]
fn test_copyleft_with_parsing() {
    let acc_addr = SliceData::from_raw(vec![0x11; 32], 256);
    let mut cell = BuilderData::default();
    cell.append_bytestring(&acc_addr).unwrap();
    let mut out_actions = BuilderData::new();
    out_actions.checked_append_reference(Cell::default()).unwrap();
    out_actions.append_u32(ACTION_COPYLEFT).unwrap();
    out_actions.append_u8(3).unwrap();
    out_actions.append_builder(&cell.clone()).unwrap();
    let cell = SliceData::load_builder(cell).unwrap();

    let myself = MsgAddressInt::with_standart(None, 0, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    test_case(
        "PUSHREFSLICE
        PUSHINT 3
        COPYLEFT
        PUSHCTR c5"
    )
        .with_ref(cell.clone().into_cell())
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc.clone())
        .expect_item(StackItem::Cell(out_actions.into_cell().unwrap()));

    test_case(
        "PUSHREFSLICE
        DUP
        PUSHINT 3
        COPYLEFT
        PUSHINT 3
        COPYLEFT"
    )
        .with_ref(cell.into_cell())
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_failure(ExceptionCode::IllegalInstruction);
}

#[test]
fn test_copyleft_masterchain() {
    let acc_addr = SliceData::from_raw(vec![0x11; 32], 256);
    let mut cell = BuilderData::default();
    cell.append_bytestring(&acc_addr).unwrap();
    let cell = SliceData::load_builder(cell).unwrap();

    let myself = MsgAddressInt::with_standart(None, -1, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    test_case(
        "PUSHREFSLICE
        PUSHINT 3
        COPYLEFT
        PUSHCTR c5"
    )
        .with_ref(cell.into_cell())
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_item(StackItem::Cell(Default::default()));
}

#[test]
fn test_copyleft_stackunderflow() {
    let myself = MsgAddressInt::with_standart(None, 0, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    test_case(
        "PUSHINT 0 COPYLEFT"
    )
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc.clone())
        .expect_failure(ExceptionCode::StackUnderflow);

    test_case(
        "COPYLEFT"
    )
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc.clone())
        .expect_failure(ExceptionCode::StackUnderflow);

    test_case(
        "NEWC ENDC CTOS COPYLEFT", 
    )
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_failure(ExceptionCode::StackUnderflow);
}

#[test]
fn test_copyleft_incorrect_address() {
    let myself = MsgAddressInt::with_standart(None, 0, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    let acc_addr = SliceData::from_raw(vec![0x11; 30], 30 * 8);
    let mut cell = BuilderData::default();
    cell.append_bytestring(&acc_addr).unwrap();
    let cell = cell.into_cell().unwrap();

    test_case(
        "PUSHREFSLICE
        PUSHINT 3
        COPYLEFT
        PUSHCTR c5",
    )
        .with_ref(cell)
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_failure(ExceptionCode::TypeCheckError);
}

#[test]
fn test_copyleft_bad_caps() {
    let myself = MsgAddressInt::with_standart(None, 0, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    test_case(
        "PUSHINT 0 COPYLEFT", 
    )
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_failure(ExceptionCode::InvalidOpcode);
}

#[test]
fn test_copyleft_too_big_int() {
    let myself = MsgAddressInt::with_standart(None, 0, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    test_case(
        "NEWC
        ENDC
        CTOS
        PUSHINT 256
        COPYLEFT",
    )
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_failure(ExceptionCode::RangeCheckError);
}

#[test]
fn test_copyleft_wrong_argument_order() {
    let myself = MsgAddressInt::with_standart(None, 0, AccountId::from([0x22; 32])).unwrap();
    let myself = SliceData::load_builder(myself.write_to_new_cell().unwrap()).unwrap();
    let smc = SmartContractInfo::with_myself(myself);

    test_case(
        "PUSHINT 0
        NEWC
        ENDC
        CTOS
        COPYLEFT",
    )
        .with_capability(GlobalCapabilities::CapCopyleft)
        .skip_fift_check(true)
        .with_temp_data(smc)
        .expect_failure(ExceptionCode::TypeCheckError);
}

#[test]
fn test_setlibcode_with_parsing() {
    let code = compile_code_to_cell("PUSHINT 1").unwrap();
    let mut out_actions = BuilderData::new();
    out_actions.append_u32(ACTION_CHANGE_LIB).unwrap();
    out_actions.append_u8(3).unwrap();
    out_actions.checked_append_reference(Cell::default()).unwrap();
    out_actions.checked_append_reference(code.clone()).unwrap();

    test_case(
        "PUSHREF
        PUSHINT 1
        SETLIBCODE
        PUSHCTR c5", 
    )
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .with_ref(code)
    .expect_item(StackItem::Cell(out_actions.into_cell().unwrap()));
}

#[test]
fn test_changelib_with_parsing() {

    let code = compile_code_to_cell("PUSHINT 1").unwrap();
    let hash = code.repr_hash();
    let mut out_actions = BuilderData::new();
    out_actions.append_u32(ACTION_CHANGE_LIB).unwrap();
    out_actions.append_u8(2).unwrap();
    out_actions.append_raw(hash.as_slice(), 256).unwrap();
    out_actions.checked_append_reference(Cell::default()).unwrap();
    
    test_case(
        "PUSHREF
        HASHCU
        PUSHINT 1
        CHANGELIB
        PUSHCTR c5", 
    )
    .with_capability(GlobalCapabilities::CapSetLibCode)
    .with_ref(code)
    .expect_item(StackItem::Cell(out_actions.into_cell().unwrap()));
}

#[test]
fn test_changelib_errors() {

    fn check(code: &str, exception: ExceptionCode) {
        expect_exception_with_capability(
            code, 
            exception, 
            GlobalCapabilities::CapSetLibCode,
            true
        )
    }

    expect_exception(
        "SETLIBCODE", 
        ExceptionCode::InvalidOpcode, 
    );
    check("SETLIBCODE", ExceptionCode::StackUnderflow);
    check("ZERO SETLIBCODE", ExceptionCode::StackUnderflow);
    check("NULL ZERO SETLIBCODE", ExceptionCode::TypeCheckError);
    check("NEWC ENDC TEN SETLIBCODE", ExceptionCode::RangeCheckError);
    check("CHANGELIB", ExceptionCode::StackUnderflow);
    check("ZERO CHANGELIB", ExceptionCode::StackUnderflow);
    check("NULL ZERO CHANGELIB", ExceptionCode::TypeCheckError);
    check("ZERO TEN CHANGELIB", ExceptionCode::RangeCheckError);
    check("PUSHINT -1 ZERO CHANGELIB", ExceptionCode::RangeCheckError);
    check("PUSHNEGPOW2 256 ZERO CHANGELIB", ExceptionCode::RangeCheckError);

}

#[test]
fn test_rawreserve_stackunderflow() {
    expect_exception("SETCODE", ExceptionCode::StackUnderflow);
    expect_exception("NULL SETCODE", ExceptionCode::TypeCheckError);
    expect_exception("ZERO SETCODE", ExceptionCode::TypeCheckError);
    expect_exception("RAWRESERVE", ExceptionCode::StackUnderflow);
    expect_exception("PUSHINT 0 RAWRESERVE", ExceptionCode::StackUnderflow);
    expect_exception("RAWRESERVEX", ExceptionCode::StackUnderflow);
    expect_exception("PUSHINT 0 RAWRESERVEX", ExceptionCode::StackUnderflow);
    expect_exception("NEWC ENDC CTOS RAWRESERVEX", ExceptionCode::StackUnderflow);
}

#[test]
fn test_rawreserve_range_check_err() {
    expect_exception(
       "PUSHINT 10
        PUSHINT 16
        RAWRESERVE",
        ExceptionCode::RangeCheckError,
    );

    expect_exception(
       "PUSHINT 10
        PUSHINT -1
        RAWRESERVE",
        ExceptionCode::RangeCheckError,
    );

    expect_exception(
       "PUSHINT -1
        PUSHINT 0
        RAWRESERVE",
        ExceptionCode::RangeCheckError,
    );

    expect_exception(
       "PUSHINT 10
        NULL
        PUSHINT 16
        RAWRESERVEX",
        ExceptionCode::RangeCheckError,
    );

    expect_exception(
       "PUSHINT -1
        NULL
        PUSHINT 1
        RAWRESERVEX",
        ExceptionCode::RangeCheckError,
    );
}

#[test]
fn test_rawreserve_type_check_err() {
    expect_exception(
       "PUSHSLICE x8_
        PUSHINT 0
        RAWRESERVE",
        ExceptionCode::TypeCheckError,
    );

    expect_exception(
       "PUSHINT 0
        PUSHSLICE x8_
        RAWRESERVE",
        ExceptionCode::TypeCheckError,
    );

    expect_exception(
       "PUSHSLICE x8_
        NULL
        PUSHSLICE x8_
        RAWRESERVEX",
        ExceptionCode::TypeCheckError,
    );

    expect_exception(
       "PUSHINT 0
        PUSHINT 0
        PUSHINT 0
        RAWRESERVEX",
        ExceptionCode::TypeCheckError,
    );
}

fn write_msg_adress(tuple: &[StackItem]) -> Result<BuilderData> {
    let addr_type = tuple[0].as_integer()?.into(0..=3u8)?;
    let mut cell = BuilderData::with_raw(vec!(addr_type << 6), 2)?;
    match addr_type {
        0b00 => (),
        0b01 => {
            let address = tuple[1].as_slice()?;
            let bits = address.remaining_bits();
            cell.append_bits(bits, 9)?;
            cell.append_bytestring(address)?;
        }
        0b10 => {
            match &tuple[1] {
                StackItem::Slice(rewrite_pfx) => {
                    cell.append_bit_one()?;
                    let bits = rewrite_pfx.remaining_bits();
                    cell.append_bits(bits, 5)?;
                    cell.append_bytestring(rewrite_pfx)?;
                }
                StackItem::None => {
                    cell.append_bit_zero()?;
                }
                _ => unreachable!()
            }
            let workchain_id = tuple[2].as_integer()?.into(-128i8..=127i8)?;
            cell.append_i8(workchain_id)?;
            cell.append_bytestring(tuple[3].as_slice()?)?;
        }
        0b11 => {
            match &tuple[1] {
                StackItem::Slice(rewrite_pfx) => {
                    cell.append_bit_one()?;
                    let bits = rewrite_pfx.remaining_bits();
                    cell.append_bits(bits, 5)?;
                    cell.append_bytestring(rewrite_pfx)?;
                }
                StackItem::None => {
                    cell.append_bit_zero()?;
                }
                _ => unreachable!()
            }
            let address = tuple[3].as_slice()?;
            let bits = address.remaining_bits();
            cell.append_bits(bits, 9)?;
            let workchain_id = tuple[2].as_integer()?.into(std::i32::MIN..=std::i32::MAX)?;
            cell.append_i32(workchain_id)?;
            cell.append_bytestring(address)?;
        }
        _ => unreachable!()
    }
    Ok(cell)
}

fn check_msg_adr(tuple: Vec<StackItem>, rewrite: Option<SliceData>) {
    let mut cell = write_msg_adress(&tuple).unwrap();
    let slice = SliceData::load_builder(cell.clone()).unwrap();
    let remainder = [0xEE, 0xAB, 0x80];
    cell.append_raw(&remainder, 16).unwrap();
    let cell = cell.into_cell().unwrap();

    test_case_with_ref("
        PUSHREFSLICE
        LDMSGADDR
    ", cell.clone())
    .expect_stack(Stack::new()
        .push(StackItem::Slice(slice.clone()))
        .push(create::slice(remainder))
    );

    test_case_with_ref("
        PUSHREFSLICE
        LDMSGADDRQ
    ", cell)
    .expect_stack(Stack::new()
        .push(StackItem::Slice(slice.clone()))
        .push(create::slice(remainder))
        .push(boolean!(true))
    );

    test_case_with_ref("
        PUSHREFSLICE
        PARSEMSGADDR
    ", slice.clone().into_cell())
    .expect_stack(Stack::new()
        .push(create::tuple(&tuple))
    );

    test_case_with_ref("
        PUSHREFSLICE
        PARSEMSGADDRQ
    ", slice.clone().into_cell())
    .expect_stack(Stack::new()
        .push(create::tuple(&tuple))
        .push(boolean!(true))
    );

    let execution_result = test_case_with_ref("
        PUSHREFSLICE
        REWRITESTDADDR
        NEWC
        STU 256
        ENDC
        CTOS
    ", slice.clone().into_cell());
    let _ = match rewrite {
        None => execution_result.expect_failure(ExceptionCode::CellUnderflow),
        Some(ref rewrite) => if rewrite.remaining_bits() == 256 {
            execution_result.expect_stack(Stack::new()
                .push(tuple[2].clone())
                .push(StackItem::Slice(rewrite.clone())))
        } else {
            execution_result.expect_failure(ExceptionCode::CellUnderflow)
        }
    };

    let execution_result = test_case_with_ref("
        PUSHREFSLICE
        REWRITESTDADDR
    ", slice.clone().into_cell());
    let _ = match rewrite {
        None => execution_result.expect_failure(ExceptionCode::CellUnderflow),
        Some(ref rewrite) => if rewrite.remaining_bits() == 256 {
            execution_result.expect_stack(Stack::new()
                .push(tuple[2].clone())
                .push(StackItem::integer(IntegerData::from_unsigned_bytes_be(rewrite.get_bytestring(0)))))
        } else {
            execution_result.expect_failure(ExceptionCode::CellUnderflow)
        }
    };

    let execution_result = test_case_with_ref("
        PUSHREFSLICE
        REWRITESTDADDRQ
    ", slice.clone().into_cell());
    let _ = match rewrite {
        None => execution_result.expect_item(boolean!(false)),
        Some(ref rewrite) => if rewrite.remaining_bits() == 256 {
            execution_result.expect_stack(Stack::new()
                .push(tuple[2].clone())
                .push(StackItem::integer(IntegerData::from_unsigned_bytes_be(rewrite.get_bytestring(0))))
                .push(boolean!(true)))
        } else {
            execution_result.expect_item(boolean!(false))
        }
    };

    let execution_result = test_case_with_ref("
        PUSHREFSLICE
        REWRITEVARADDR
    ", slice.clone().into_cell());
    let _ = match rewrite {
        None => execution_result.expect_failure(ExceptionCode::CellUnderflow),
        Some(ref rewrite) => execution_result.expect_stack(Stack::new()
            .push(tuple[2].clone())
            .push(StackItem::Slice(rewrite.clone())))
    };

    let execution_result = test_case_with_ref("
        PUSHREFSLICE
        REWRITEVARADDRQ
    ", slice.into_cell());
    let _ = match rewrite {
        None => execution_result.expect_item(boolean!(false)),
        Some(ref rewrite) => execution_result.expect_stack(Stack::new()
            .push(tuple[2].clone())
            .push(StackItem::Slice(rewrite.clone()))
            .push(boolean!(true)))
    };
}

#[test]
fn test_load_msg_addr_normal() {
    let short_addr = SliceData::new(vec![0x11, 0x22, 0x80]);
    let long_addr  = SliceData::new(vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x80]);
    let acc_addr   = SliceData::from_raw(vec![0x11; 32], 256);
    let prefix     = SliceData::new(vec![0x77, 0x88, 0x99, 0x80]);

    let prefix_slice = StackItem::Slice(prefix);
    let short_slice  = StackItem::Slice(short_addr.clone());
    let long_slice   = StackItem::Slice(long_addr.clone());
    let acc_slice    = StackItem::Slice(acc_addr.clone());

    // let rewrite_pfx  = Some(AnycastInfo::with_rewrite_pfx(prefix));

    let long_prefix = SliceData::new(vec![0x77, 0x88, 0x99, 0x44, 0x55, 0x66, 0x80]);
    let acc_prefix  = SliceData::new(vec![0x77, 0x88, 0x99, 0x11, 0x11, 0x11, 0x11, 0x11,
                                          0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
                                          0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
                                          0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x80]);

    // let addr = MsgAddressInt::AddrNone;
    check_msg_adr(vec![int!(0)], None);

    // let addr = MsgAddressExt::with_extern(acc_addr.clone()).unwrap();
    check_msg_adr(vec![int!(1), acc_slice.clone()], None);

    // Standart without prefix
    // let addr = MsgAddressInt::with_standart(None, 1, acc_addr.clone()).unwrap();
    check_msg_adr(vec![int!(2), StackItem::None, int!(1), acc_slice.clone()], Some(acc_addr.clone()));

    // Standart with prefix
    // let addr = MsgAddressInt::with_standart(rewrite_pfx.clone(), 2, acc_addr.clone()).unwrap();
    check_msg_adr(vec![int!(2), prefix_slice.clone(), int!(2), acc_slice.clone()], Some(acc_prefix.clone()));

    // Variant with 256 bit addr and without prefix
    // let addr = MsgAddressInt::with_variant(None, 3, acc_addr.clone()).unwrap();
    check_msg_adr(vec![int!(3), StackItem::None, int!(3), acc_slice.clone()], Some(acc_addr));

    // Variant with 256 bit addr and prefix
    // let addr = MsgAddressInt::with_variant(rewrite_pfx.clone(), 4, acc_addr.clone()).unwrap();
    check_msg_adr(vec![int!(3), prefix_slice.clone(), int!(4), acc_slice], Some(acc_prefix));

    // Variant with short addr and without prefix
    // let addr = MsgAddressInt::with_variant(None, 5, short_addr.clone()).unwrap();
    check_msg_adr(vec![int!(3), StackItem::None, int!(5), short_slice.clone()], Some(short_addr));

    // Variant with addr shorter than prefix
    // let addr = MsgAddressInt::with_variant(rewrite_pfx.clone(), 6, short_addr.clone()).unwrap();
    check_msg_adr(vec![int!(3), prefix_slice.clone(), int!(6), short_slice], None);

    // Variant with long addr and without prefix
    // let addr = MsgAddressInt::with_variant(None, 7, long_addr.clone()).unwrap();
    check_msg_adr(vec![int!(3), StackItem::None, int!(7), long_slice.clone()], Some(long_addr));

    // Variant with addr longer than prefix
    // let addr = MsgAddressInt::with_variant(rewrite_pfx.clone(), 8, long_addr.clone()).unwrap();
    check_msg_adr(vec![int!(3), prefix_slice, int!(8), long_slice], Some(long_prefix));
}

#[test]
fn test_load_msg_addr_with_error() {
    test_case("
        PUSHSLICE xE_
        LDMSGADDR
    ")
    .expect_failure(ExceptionCode::CellUnderflow);

    test_case("
        LDMSGADDR
    ")
    .expect_failure(ExceptionCode::StackUnderflow);

    test_case("
        PUSHSLICE xE_
        LDMSGADDRQ
    ")
    .expect_stack(Stack::new()
        .push(create::slice([0xE0]))
        .push(boolean!(false))
    );
}

#[test]
fn test_parse_msg_addr_with_error() {
    test_case("
        PUSHSLICE xE_
        PARSEMSGADDR
    ")
    .expect_failure(ExceptionCode::CellUnderflow);

    test_case("
        PUSHSLICE xE_
        PARSEMSGADDRQ
    ")
    .expect_stack(Stack::new()
        .push(boolean!(false))
    );
}
