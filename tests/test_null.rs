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

use ever_block::{SliceData, types::ExceptionCode};
use ever_vm::{
    boolean, int, stack::{Stack, StackItem, integer::IntegerData},
};

mod common;
use common::*;

mod null {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHNULL
            ISNULL",
        ).expect_item(boolean!(true));

        test_case(
           "NULL
            ISNULL",
        ).expect_item(boolean!(true));

        test_case(
           "ZERO
            ISNULL",
        ).expect_item(boolean!(false));
    }

    #[test]
    fn test_dup_swap() {
        test_case(
           "PUSHNULL
            DUP
            ISNULL
            SWAP
            ISNULL",
        ).expect_stack(Stack::new()
            .push(boolean!(true))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_unsuccessful_comparison() {
        test_case(
           "NULL
            PUSHNULL
            EQUAL",
        ).expect_failure(ExceptionCode::TypeCheckError);

        test_case(
           "NULL
            ZERO
            EQUAL",
        ).expect_failure(ExceptionCode::TypeCheckError);
    }
}

mod nullswapif {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            NULLSWAPIF
            SWAP
            ISNULL",
        ).expect_stack(Stack::new()
            .push(int!(100))
            .push(boolean!(true))
        );

        test_case(
           "ZERO
            NULLSWAPIF
            ISNULL",
        ).expect_item(boolean!(false));
    }


    #[test]
    fn test_exceptions() {
        expect_exception("NULLSWAPIF", ExceptionCode::StackUnderflow);
        expect_exception("PUSHSLICE x5 NULLSWAPIF", ExceptionCode::TypeCheckError);
    }
}

mod nullswapif2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            NULLSWAPIF2",
        ).expect_stack(Stack::new()
            .push(StackItem::None)
            .push(StackItem::None)
            .push(int!(100))
        );

        test_case(
           "ZERO
            NULLSWAPIF2",
        ).expect_item(int!(0));
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLSWAPIF2", ExceptionCode::StackUnderflow);
        expect_exception("PUSHSLICE x5 NULLSWAPIF2", ExceptionCode::TypeCheckError);
    }
}

mod nullswapifnot {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            NULLSWAPIFNOT
            ISNULL",
        ).expect_item(boolean!(false));

        test_case(
           "ZERO
            NULLSWAPIFNOT
            SWAP
            ISNULL",
        ).expect_stack(Stack::new()
            .push(int!(0))
            .push(boolean!(true))
        );
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLSWAPIFNOT", ExceptionCode::StackUnderflow);
        expect_exception("PUSHSLICE x5 NULLSWAPIFNOT", ExceptionCode::TypeCheckError);
    }
}

mod nullswapifnot2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            NULLSWAPIFNOT2",
        ).expect_item(int!(100));

        test_case(
           "ZERO
            NULLSWAPIFNOT2",
        ).expect_stack(Stack::new()
            .push(StackItem::None)
            .push(StackItem::None)
            .push(int!(0))
        );
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLSWAPIFNOT2", ExceptionCode::StackUnderflow);
        expect_exception("PUSHSLICE x5 NULLSWAPIFNOT2", ExceptionCode::TypeCheckError);
    }
}

mod nullrotrif {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "ZERO
            PUSHINT 100
            NULLROTRIF
            ROT
            ISNULL",
        ).expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(100))
            .push(boolean!(true))
        );

        test_case(
           "PUSHSLICE x5_
            PUSHINT 100
            NULLROTRIF
            ROT
            ISNULL",
        ).expect_stack(Stack::new()
            .push(StackItem::Slice(SliceData::new(vec![0x50])))
            .push(int!(100))
            .push(boolean!(true))
        );

        test_case(
           "PUSHINT 100
            ZERO
            NULLROTRIF",
        ).expect_stack(Stack::new()
            .push(int!(100))
            .push(int!(0))
        );
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLROTRIF", ExceptionCode::StackUnderflow);
        expect_exception("ZERO NULLROTRIF", ExceptionCode::StackUnderflow);
        expect_exception("ZERO PUSHSLICE x5 NULLROTRIF", ExceptionCode::TypeCheckError);
    }
}

mod nullrotrif2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "ZERO
            PUSHINT 100
            NULLROTRIF2",
        ).expect_stack(Stack::new()
            .push(StackItem::None)
            .push(StackItem::None)
            .push(int!(0))
            .push(int!(100))
        );

        test_case(
           "PUSHSLICE x5_
            PUSHINT 100
            NULLROTRIF2",
        ).expect_stack(Stack::new()
            .push(StackItem::None)
            .push(StackItem::None)
            .push(StackItem::Slice(SliceData::new(vec![0x50])))
            .push(int!(100))
        );

        test_case(
           "PUSHINT 100
            ZERO
            NULLROTRIF2",
        ).expect_stack(Stack::new()
            .push(int!(100))
            .push(int!(0))
        );
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLROTRIF2", ExceptionCode::StackUnderflow);
        expect_exception("ZERO NULLROTRIF2", ExceptionCode::StackUnderflow);
        expect_exception("ZERO PUSHSLICE x5 NULLROTRIF2", ExceptionCode::TypeCheckError);
    }
}

mod nullrotrifnot {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            ZERO
            NULLROTRIFNOT
            ROT
            ISNULL",
        ).expect_stack(Stack::new()
            .push(int!(100))
            .push(int!(0))
            .push(boolean!(true))
        );

        test_case(
           "PUSHSLICE x5_
            ZERO
            NULLROTRIFNOT
            ROT
            ISNULL",
        ).expect_stack(Stack::new()
            .push(StackItem::Slice(SliceData::new(vec![0x50])))
            .push(int!(0))
            .push(boolean!(true))
        );

        test_case(
           "ZERO
            PUSHINT 100
            NULLROTRIFNOT",
        ).expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(100))
        );
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLROTRIFNOT", ExceptionCode::StackUnderflow);
        expect_exception("ZERO NULLROTRIFNOT", ExceptionCode::StackUnderflow);
        expect_exception("ZERO PUSHSLICE x5 NULLROTRIFNOT", ExceptionCode::TypeCheckError);
    }
}

mod nullrotrifnot2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            ZERO
            NULLROTRIFNOT2",
        ).expect_stack(Stack::new()
            .push(StackItem::None)
            .push(StackItem::None)
            .push(int!(100))
            .push(int!(0))
        );

        test_case(
           "PUSHSLICE x5_
            ZERO
            NULLROTRIFNOT2",
        ).expect_stack(Stack::new()
            .push(StackItem::None)
            .push(StackItem::None)
            .push(StackItem::Slice(SliceData::new(vec![0x50])))
            .push(int!(0))
        );

        test_case(
           "ZERO
            PUSHINT 100
            NULLROTRIFNOT2",
        ).expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(100))
        );
    }

    #[test]
    fn test_exceptions() {
        expect_exception("NULLROTRIFNOT2", ExceptionCode::StackUnderflow);
        expect_exception("ZERO NULLROTRIFNOT2", ExceptionCode::StackUnderflow);
        expect_exception("ZERO PUSHSLICE x5 NULLROTRIFNOT2", ExceptionCode::TypeCheckError);
    }
}