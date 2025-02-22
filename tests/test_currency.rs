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

mod ldgrams {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHSLICE x1568_
            LDGRAMS
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(86))
            .push(boolean!(true))
        );

        test_case(
           "PUSHSLICE x248568_
            LDVARUINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(boolean!(true))
        );

        test_case(
           "PUSHSLICE x24856212348_
            LDVARUINT16
            LDGRAMS
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(int!(4660))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_cell_underflow() {
        test_case(
           "PUSHSLICE x158_
            LDGRAMS",
        ).expect_failure(ExceptionCode::CellUnderflow);
    }
}

mod ldvarint16 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHSLICE x1568_
            LDVARINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(86))
            .push(boolean!(true))
        );

        test_case(
           "PUSHINT 100
            PUSHINT 2
            NEWC
            STU 4
            STI 16
            ENDC
            CTOS
            LDVARINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(100))
            .push(boolean!(true))
        );

        test_case(
           "PUSHSLICE x24856212348_
            LDVARINT16
            LDVARINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(int!(4660))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_cell_underflow() {
        test_case(
           "PUSHSLICE x158_
            LDVARINT16",
        ).expect_failure(ExceptionCode::CellUnderflow);
    }
}

mod stgrams {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "NEWC
            PUSHINT 86
            STGRAMS
            ENDC
            CTOS

            LDGRAMS
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(86))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STGRAMS
            ENDC
            CTOS

            LDVARUINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STGRAMS
            PUSHINT 4660
            STGRAMS
            ENDC
            CTOS

            LDVARUINT16
            LDGRAMS
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(int!(4660))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_range_check() {
        test_case(
           "NEWC
            PUSHINT -1
            STGRAMS",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 120
            LSHIFT
            STGRAMS
            ENDC",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 120
            LSHIFT
            DEC
            STGRAMS
            ENDC",
        ).expect_success();

        test_case(
           "NEWC
            PUSHINT 0
            STGRAMS",
        ).expect_success();
    }
}

mod stvarint16 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "NEWC
            PUSHINT 86
            STVARINT16
            ENDC
            CTOS

            LDVARINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(86))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STVARINT16
            ENDC
            CTOS

            LDVARINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STVARINT16
            PUSHINT 4660
            STVARINT16
            ENDC
            CTOS

            LDVARINT16
            LDVARINT16
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(int!(4660))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_range_check() {
        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 119
            LSHIFT
            STVARINT16
            ENDC",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 119
            LSHIFT
            INC
            NEGATE
            STVARINT16
            ENDC",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 119
            LSHIFT
            DEC
            STVARINT16
            ENDC",
        ).expect_success();

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 119
            LSHIFT
            NEGATE
            STVARINT16
            ENDC",
        ).expect_success();
    }
}

mod stvaruint32 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "NEWC
            PUSHINT 86
            STVARUINT32
            ENDC
            CTOS

            LDVARUINT32
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(86))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STVARUINT32
            ENDC
            CTOS

            LDVARUINT32
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STVARUINT32
            PUSHINT 4660
            STVARUINT32
            ENDC
            CTOS

            LDVARUINT32
            LDVARUINT32
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(int!(4660))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_range_check() {
        test_case(
           "NEWC
            PUSHINT -1
            STVARUINT32",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 248
            LSHIFT
            STVARUINT32
            ENDC",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 248
            LSHIFT
            DEC
            STVARUINT32
            ENDC",
        ).expect_success();

        test_case(
           "NEWC
            PUSHINT 0
            STVARUINT32",
        ).expect_success();
    }
}

mod stvarint32 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "NEWC
            PUSHINT 86
            STVARINT32
            ENDC
            CTOS

            LDVARINT32
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(86))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STVARINT32
            ENDC
            CTOS

            LDVARINT32
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(boolean!(true))
        );

        test_case(
           "NEWC
            PUSHINT 18518
            STVARINT32
            PUSHINT 4660
            STVARINT32
            ENDC
            CTOS

            LDVARINT32
            LDVARINT32
            SEMPTY",
        ).expect_stack(Stack::new()
            .push(int!(18518))
            .push(int!(4660))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_range_check() {
        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 247
            LSHIFT
            STVARINT32
            ENDC",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 247
            LSHIFT
            INC
            NEGATE
            STVARINT32
            ENDC",
        ).expect_failure(ExceptionCode::RangeCheckError);

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 247
            LSHIFT
            DEC
            STVARINT32
            ENDC",
        ).expect_success();

        test_case(
           "NEWC
            PUSHINT 1
            PUSHINT 247
            LSHIFT
            NEGATE
            STVARINT32
            ENDC",
        ).expect_success();

        test_case(
           "NEWC
            PUSHPOW2DEC 247
            STVARINT32
            ENDC",
        ).expect_success();

        test_case(
           "NEWC
            PUSHNEGPOW2 247
            STVARINT32
            ENDC",
        ).expect_success();
    }
}

#[test]
fn test_stvaruint32_nan() {
    test_case("
        NEWC
        PUSHNAN
        STVARUINT32
    ")
    .with_capability(ever_block::GlobalCapabilities::CapsTvmBugfixes2022)
    .expect_failure(ExceptionCode::RangeCheckError);
}
