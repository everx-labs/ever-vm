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

// PUSHINT 10 -> 0x7A
// PUSHINT 12 -> 0x7C
const CREATE_DICTU_INSTRUCTIONS: &str = "
    PUSHSLICE x7A8_
    PUSHINT 1
    NEWDICT
    PUSHINT 8
    DICTUSET
    PUSHSLICE x800C8_
    SWAP
    PUSHINT 2
    SWAP
    PUSHINT 8
    DICTUSET
";

const CREATE_DICTI_INSTRUCTIONS: &str = "
    PUSHSLICE x7A8_
    PUSHINT -1
    NEWDICT
    PUSHINT 8
    DICTISET
    PUSHSLICE x800C8_
    SWAP
    PUSHINT -2
    SWAP
    PUSHINT 8
    DICTISET
";

mod dictugetjmp {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(format!("
            PUSHINT 1
            {}
            PUSHINT 8
            DICTUGETJMP ",
            CREATE_DICTU_INSTRUCTIONS
        ))
         .expect_item(int!(10));

        test_case(format!("
            PUSHINT 2
            {}
            PUSHINT 8
            DICTUGETJMP ",
            CREATE_DICTU_INSTRUCTIONS
        ))
         .expect_item(int!(12));
    }
}

mod dictugetexec {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(format!("
            PUSHINT 1
            {}
            PUSHINT 8
            DICTUGETEXEC",
            CREATE_DICTU_INSTRUCTIONS
        ))
        .expect_item(int!(10));

        test_case(format!("
            PUSHINT 2
            {}
            PUSHINT 8
            DICTUGETEXEC ",
            CREATE_DICTU_INSTRUCTIONS
        ))
         .expect_item(int!(12));
    }
}

mod dictigetjmp {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(format!("
            PUSHINT -1
            {}
            PUSHINT 8
            DICTIGETJMP ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(10));

        test_case(format!("
            PUSHINT -2
            {}
            PUSHINT 8
            DICTIGETJMP ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(12));
    }

    #[test]
    fn test_failure_flow() {
        test_case(format!("
            PUSHINT 666
            PUSHINT -3
            {}
            PUSHINT 8
            DICTIGETJMP",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(666));
    }
}

mod dictigetexec {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(format!("
            PUSHINT -1
            {}
            PUSHINT 8
            DICTIGETEXEC",
            CREATE_DICTI_INSTRUCTIONS
        ))
        .expect_item(int!(10));

        test_case(format!("
            PUSHINT -2
            {}
            PUSHINT 8
            DICTIGETEXEC ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(12));
    }

    #[test]
    fn test_failure_flow() {
        test_case(format!("
            PUSHINT 666
            PUSHINT -3
            {}
            PUSHINT 8
            DICTIGETEXEC",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(666));
    }
}

mod dictigetjmpz {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(format!("
            PUSHINT -1
            {}
            PUSHINT 8
            DICTIGETJMPZ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(10));

        test_case(format!("
            PUSHINT -2
            {}
            PUSHINT 8
            DICTIGETJMPZ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(12));
    }

    #[test]
    fn test_failure_flow() {
        test_case(format!("
            PUSHINT -3
            {}
            PUSHINT 8
            DICTIGETJMPZ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(-3));
    }
}

mod dictigetexecz {
    use super::*;

    #[test]
    fn test_normal_flow() {
        test_case(format!("
            PUSHINT -1
            {}
            PUSHINT 8
            DICTIGETEXECZ",
            CREATE_DICTI_INSTRUCTIONS
        ))
        .expect_item(int!(10));

        test_case(format!("
            PUSHINT -2
            {}
            PUSHINT 8
            DICTIGETEXECZ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(12));
    }

    #[test]
    fn test_failure_flow() {
        test_case(format!("
            PUSHINT -3
            {}
            PUSHINT 8
            DICTIGETEXECZ",
            CREATE_DICTI_INSTRUCTIONS
        ))
         .expect_item(int!(-3));
    }
}
