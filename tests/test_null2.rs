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

mod zeroswapif {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
            "PUSHINT 100
            ZEROSWAPIF",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
                .push(int!(100))
            );

        test_case(
            "ZERO
            ZEROSWAPIF"
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
            );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROSWAPIF")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zeroswapif2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            ZEROSWAPIF2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(0))
            .push(int!(100))
        );

        test_case(
           "ZERO
            ZEROSWAPIF2",
        )
            .skip_fift_check(true)
            .expect_item(int!(0));
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROSWAPIF2")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zeroswapifnot {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case("
            PUSHINT 100
            ZEROSWAPIFNOT
        ")
            .skip_fift_check(true)
            .expect_item(int!(100));

        test_case("
            PUSHINT 0
            ZEROSWAPIFNOT
        ")
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
                .push(int!(0))
            );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROSWAPIFNOT")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zeroswapifnot2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            ZEROSWAPIFNOT2",
        )
            .skip_fift_check(true)
            .expect_item(int!(100));

        test_case(
           "ZERO
            ZEROSWAPIFNOT2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
                .push(int!(0))
                .push(int!(0))
            );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROSWAPIFNOT2")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zerorotrif {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "NULL
            PUSHINT 100
            ZEROROTRIF
            ROT
            ISZERO",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(StackItem::None)
                .push(int!(100))
                .push(boolean!(true))
            );

        test_case(
           "PUSHSLICE x5_
            PUSHINT 100
            ZEROROTRIF
            ROT
            ISZERO",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(StackItem::Slice(SliceData::new(vec![0x50])))
                .push(int!(100))
                .push(boolean!(true))
            );

        test_case(
           "PUSHINT 100
            ZERO
            ZEROROTRIF",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(100))
                .push(int!(0))
            );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROROTRIF")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
        test_case("ZERO ZEROROTRIF")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zerorotrif2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "NULL
            PUSHINT 100
            ZEROROTRIF2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(0))
            .push(StackItem::None)
            .push(int!(100))
        );

        test_case(
           "PUSHSLICE x5_
            PUSHINT 100
            ZEROROTRIF2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(0))
            .push(StackItem::Slice(SliceData::new(vec![0x50])))
            .push(int!(100))
        );

        test_case(
           "PUSHINT 100
            ZERO
            ZEROROTRIF2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(100))
                .push(int!(0))
            );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROROTRIF2")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
        test_case("ZERO ZEROROTRIF2")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zerorotrifnot {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            ZERO
            ZEROROTRIFNOT"
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
                .push(int!(100))
                .push(int!(0))
            );

        test_case(
           "PUSHSLICE x5_
            ZERO
            ZEROROTRIFNOT",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
                .push(StackItem::Slice(SliceData::new(vec![0x50])))
                .push(int!(0))
            );

        test_case(
           "ZERO
            PUSHINT 100
            ZEROROTRIFNOT",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
                .push(int!(0))
                .push(int!(100))
            );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROROTRIFNOT")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
        test_case("ZERO ZEROROTRIFNOT")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}

mod zerorotrifnot2 {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(
           "PUSHINT 100
            ZERO
            ZEROROTRIFNOT2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(0))
            .push(int!(100))
            .push(int!(0))
        );

        test_case(
           "PUSHSLICE x5_
            ZERO
            ZEROROTRIFNOT2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(0))
            .push(StackItem::Slice(SliceData::new(vec![0x50])))
            .push(int!(0))
        );

        test_case(
           "ZERO
            PUSHINT 100
            ZEROROTRIFNOT2",
        )
            .skip_fift_check(true)
            .expect_stack(Stack::new()
            .push(int!(0))
            .push(int!(100))
        );
    }

    #[test]
    fn test_exceptions() {
        test_case("ZEROROTRIFNOT2")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
        test_case("ZERO ZEROROTRIFNOT2")
            .skip_fift_check(true)
            .expect_failure(ExceptionCode::StackUnderflow);
    }
}
