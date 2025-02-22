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

use ever_block::GlobalCapabilities;
use ever_block::{BuilderData, SliceData, ExceptionCode};
use ever_vm::{
    int, stack::{Stack, StackItem, integer::IntegerData},
};

mod common;
use common::*;

fn test_case_with_c7(
    code: &str, 
    capability: GlobalCapabilities,
    check_fift: bool
) -> TestCaseInputs {
    let prefix = "
        PUSHINT 0
        PUSHINT 1
        PUSHINT 2
        PUSHINT 3
        PUSHINT 4
        PUSHINT 5
        PUSHINT 6
        ; balance 1000 grams and no others
        PUSHINT 1000
        NULL
        PAIR
        ; prepare my address addr_var $11_0_000001000_0:32_0101_0101 => $1100_0000_1000_0:32_0101_0101
        PUSHSLICE xC080000000055
        ; prepare config_param dictionary [-1]=x12345, [2000]=x67890
        NEWC
        STSLICECONST x12345
        ENDC
        PUSHINT -1
        NULL
        PUSHINT 32
        DICTISETREF
        NEWC
        STSLICECONST x67890
        ENDC
        PUSHINT 2000
        ROT
        PUSHINT 32
        DICTISETREF
        NEWC
        STSLICECONST xABCDEF
        ENDC
        DUP
        HASHCU
        PUSHINT 9112
        PUSHINT 12300 ; seq_no
        TUPLE 14
        SINGLE
        POP c7";
    test_case(format!("{} {}", prefix, code))
    .with_capability(capability)
    .skip_fift_check(!check_fift)
}

mod getparam {
    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "GETPARAM 1", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(1));
        test_case_with_c7(
            "GETPARAM 2", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(2));
        test_case_with_c7(
            "GETPARAM 6", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(6));
        test_case_with_c7(
            "GETPARAM 7", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(create::tuple(&[int!(1000), StackItem::None]));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            GETPARAM 1
        ", ExceptionCode::RangeCheckError);

        expect_exception("
            NIL
            SINGLE
            POP c7
            GETPARAM 2
        ", ExceptionCode::RangeCheckError);

        expect_exception("
            NIL
            SINGLE
            POP c7
            GETPARAM 3
        ", ExceptionCode::RangeCheckError);
    }
}

mod root {

    use super::*;
    use ever_block::{HashmapE, HashmapType};

    #[test]
    fn normal_flow() {
        let mut params = HashmapE::with_bit_len(32);
        params.setref(
            SliceData::from_raw(2000i32.to_be_bytes().to_vec(), 32), 
            &SliceData::new(vec![0x67, 0x89, 0x08]).into_cell()
        ).unwrap();
        params.setref(
            SliceData::from_raw((-1i32).to_be_bytes().to_vec(), 32), 
            &SliceData::new(vec![0x12, 0x34, 0x58]).into_cell()
        ).unwrap();
        test_case_with_c7(
            "CONFIGROOT", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(StackItem::Cell(params.data().unwrap().clone()));

        test_case("
            ZERO
            DUP
            DUP
            DUP2
            DUP2
            DUP2
            NULL
            TUPLE 10
            SINGLE
            POP c7
            CONFIGROOT
        ").expect_item(StackItem::None);
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            CONFIGROOT
        ", ExceptionCode::RangeCheckError);
    }
}

mod dict {

    use super::*;
    use ever_block::{HashmapE, HashmapType};

    #[test]
    fn normal_flow() {
        let mut params = HashmapE::with_bit_len(32);
        params.setref(
            SliceData::from_raw(2000i32.to_be_bytes().to_vec(), 32), 
            &SliceData::new(vec![0x67, 0x89, 0x08]).into_cell()
        ).unwrap();
        params.setref(
            SliceData::from_raw((-1i32).to_be_bytes().to_vec(), 32), 
            &SliceData::new(vec![0x12, 0x34, 0x58]).into_cell()
        ).unwrap();
        test_case_with_c7(
            "CONFIGDICT", 
            GlobalCapabilities::CapNone,
            true
        )
        .expect_stack(Stack::new()
        .push(StackItem::Cell(params.data().unwrap().clone()))
        .push(int!(32)));

        test_case("
            ZERO
            DUP
            DUP
            DUP2
            DUP2
            DUP2
            NULL
            TUPLE 10
            SINGLE
            POP c7
            CONFIGDICT
        ").expect_stack(Stack::new()
            .push(StackItem::None)
            .push(int!(32)));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            CONFIGDICT
        ", ExceptionCode::RangeCheckError);
    }
}

mod param_ref {

    use super::*;

    #[test]
    fn normal_flow() {

        test_case_with_c7(
            "PUSHINT -1 CONFIGPARAM", 
            GlobalCapabilities::CapNone,
            true
        )
        .expect_stack(Stack::new()
        .push(create::cell([0x12, 0x34, 0x58]))
        .push(int!(-1)));

        test_case_with_c7(
            "PUSHINT 2000 CONFIGPARAM", 
            GlobalCapabilities::CapNone,
            true
        )
        .expect_stack(Stack::new()
        .push(create::cell([0x67, 0x89, 0x08]))
        .push(int!(-1)));

        test_case_with_c7(
            "PUSHINT 0 CONFIGPARAM", 
            GlobalCapabilities::CapNone,
            true
        )
        .expect_stack(Stack::new()
        .push(int!(0)));

        test_case("
            ZERO
            DUP
            DUP
            DUP2
            DUP2
            DUP2
            NULL
            TUPLE 10
            SINGLE
            POP c7
            ZERO
            CONFIGPARAM
        ").expect_item(int!(0));

    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            ZERO
            CONFIGPARAM
        ", ExceptionCode::RangeCheckError);
    }

    #[test]
    fn type_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            NULL
            CONFIGPARAM
        ", ExceptionCode::TypeCheckError);
    }
}

mod param_opt {

    use super::*;

    #[test]
    fn normal_flow() {

        test_case_with_c7(
            "PUSHINT -1 CONFIGOPTPARAM", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(create::cell([0x12, 0x34, 0x58]));
        test_case_with_c7(
            "PUSHINT 2000 CONFIGOPTPARAM", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(create::cell([0x67, 0x89, 0x08]));
        test_case_with_c7(
            "PUSHINT 0 CONFIGOPTPARAM", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(StackItem::None);

        test_case("
            ZERO
            DUP
            DUP
            DUP2
            DUP2
            DUP2
            NULL
            TUPLE 10
            SINGLE
            POP c7
            ZERO
            CONFIGOPTPARAM
        ").expect_item(StackItem::None);

    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            ZERO
            CONFIGOPTPARAM
        ", ExceptionCode::RangeCheckError);
    }

    #[test]
    fn type_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            NULL
            CONFIGOPTPARAM
        ", ExceptionCode::TypeCheckError);
    }
}

mod balance {
    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "BALANCE UNPAIR", 
            GlobalCapabilities::CapNone,
            true
        ).expect_stack(Stack::new().push(int!(1000)).push(StackItem::None));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            RANDSEED
        ", ExceptionCode::RangeCheckError);
    }
}

mod myaddr {
    use super::*;

    #[test]
    fn normal_flow() {
        let slice = SliceData::new(vec![0xc0, 0x80, 0x00, 0x00, 0x00, 0x05, 0x58]);
        test_case_with_c7(
            "MYADDR", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(StackItem::Slice(slice));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            MYADDR
        ", ExceptionCode::RangeCheckError);
    }
}

mod mycode {

    use super::*;

    #[test]
    fn normal_flow() {
        let cell = BuilderData::with_raw(vec![0xAB, 0xCD, 0xEF], 8 * 3).unwrap().into_cell().unwrap();
        test_case_with_c7(
            "MYCODE", 
            GlobalCapabilities::CapMycode,
            false
        ).expect_item(StackItem::cell(cell));
    }

    #[test]
    fn bad_cap() {
        if cfg!(feature = "fift_check") {
            test_case("MYCODE")
                .skip_fift_check(true)  // disable fift check
                .expect_failure(ExceptionCode::InvalidOpcode);
        } else {
            expect_exception("MYCODE", ExceptionCode::InvalidOpcode);
        }
    }

    #[test]
    fn range_check_error() {
        expect_exception_with_capability(
            "NIL
            SINGLE
            POP c7
            MYCODE",
            ExceptionCode::RangeCheckError,
            GlobalCapabilities::CapMycode,
            false
        );
    }

}

mod init_code_hash {

    use super::*;

    #[test]
    fn normal_flow() {
        let cell = BuilderData::with_raw(vec![0xAB, 0xCD, 0xEF], 8 * 3).unwrap().into_cell().unwrap();
        let hash = IntegerData::from_unsigned_bytes_be(cell.repr_hash().as_slice());
        test_case_with_c7(
            "INITCODEHASH", 
            GlobalCapabilities::CapInitCodeHash,
            false
        ).expect_item(StackItem::int(hash));
    }

    #[test]
    fn bad_cap() {
        expect_exception(
            "INITCODEHASH", 
            ExceptionCode::InvalidOpcode,
        );
    }

    #[test]
    fn range_check_error() {
        expect_exception_with_capability(
            "NIL
            SINGLE
            POP c7
            INITCODEHASH", 
            ExceptionCode::RangeCheckError,
            GlobalCapabilities::CapInitCodeHash,
            false
        );
    }

}

mod storage_fee {

    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "STORAGEFEE",
            GlobalCapabilities::CapStorageFeeToTvm,
            false
        ).expect_item(int!(9112));
    }

    #[test]
    fn bad_cap() {
        expect_exception(
            "STORAGEFEE",
            ExceptionCode::InvalidOpcode,
        );
    }

    #[test]
    fn range_check_error() {
        expect_exception_with_capability(
            "NIL
            SINGLE
            POP c7
            STORAGEFEE",
            ExceptionCode::RangeCheckError,
            GlobalCapabilities::CapStorageFeeToTvm,
            false
        );
    }

}

mod seq_no {

    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "SEQNO",
            GlobalCapabilities::CapDelections,
            false
        ).expect_item(int!(12300));
    }

    #[test]
    fn bad_cap() {
        expect_exception(
            "SEQNO",
            ExceptionCode::InvalidOpcode,
        );
    }

    #[test]
    fn range_check_error() {
        expect_exception_with_capability(
            "NIL
            SINGLE
            POP c7
            SEQNO",
            ExceptionCode::RangeCheckError,
            GlobalCapabilities::CapDelections,
            false
        );
    }

}

mod randseed {
    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "RANDSEED", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(6));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            SINGLE
            POP c7
            RANDSEED
        ", ExceptionCode::RangeCheckError);
    }
}

mod now {
    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "NOW", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(3));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            PUSHINT 1
            SINGLE
            SINGLE
            POP c7
            NOW
        ", ExceptionCode::RangeCheckError);
    }
}

mod blocklt {
    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "BLOCKLT", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(4));
    }                              

    #[test]
    fn range_check_error() {
        expect_exception("
            PUSHINT 1
            SINGLE
            SINGLE
            POP c7
            BLOCKLT
        ", ExceptionCode::RangeCheckError);
    }
}

mod ltime {
    use super::*;

    #[test]
    fn normal_flow() {
        test_case_with_c7(
            "LTIME", 
            GlobalCapabilities::CapNone,
            true
        ).expect_item(int!(5));
    }

    #[test]
    fn range_check_error() {
        expect_exception("
            NIL
            POP c7
            LTIME
        ", ExceptionCode::RangeCheckError);
    }
}

#[test]
fn test_setgetglobvar_normal(){
    test_case("
        ONE
        TWO
        TEN
        TRIPLE
        POP c7
        PUSHINT 9
        PUSHINT 3
        SETGLOBVAR
        PUSHINT 2
        GETGLOBVAR
        PUSHINT 3
        GETGLOBVAR
    ").expect_int_stack(&[10, 9]);

    test_case("
        ONE
        TWO
        TEN
        TRIPLE
        POP c7
        PUSHINT 5
        GETGLOBVAR
    ").expect_item(StackItem::None);
}

#[test]
fn test_setgetglob_normal(){
    test_case("
        ONE
        TWO
        TEN
        TRIPLE
        POP c7
        PUSHINT 9
        SETGLOB 31
        GETGLOB 2
        GETGLOB 31
    ").expect_int_stack(&[10, 9]);

    test_case("
        ONE
        TWO
        TEN
        TRIPLE
        POP c7
        GETGLOB 5
    ").expect_item(StackItem::None);
}

#[test]
fn test_setgetglobvar_range_error(){
    expect_exception("
        PUSHINT 255
        GETGLOBVAR
    ", ExceptionCode::RangeCheckError);

    expect_exception("
        PUSHINT -1
        GETGLOBVAR
    ", ExceptionCode::RangeCheckError);

    expect_exception("
        ZERO
        PUSHINT 255
        SETGLOBVAR
    ", ExceptionCode::RangeCheckError);

    expect_exception("
        ZERO
        PUSHINT -1
        SETGLOBVAR
    ", ExceptionCode::RangeCheckError);
}

#[test]
fn test_setgetglobvar_stack_underflow(){
    expect_exception("GETGLOBVAR", ExceptionCode::StackUnderflow);
    expect_exception("SETGLOBVAR", ExceptionCode::StackUnderflow);
    expect_exception("ZERO SETGLOBVAR", ExceptionCode::StackUnderflow);
}
