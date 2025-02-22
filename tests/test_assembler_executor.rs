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

use std::{fs::File, io::Read};
mod common;
use common::*;
use ever_assembler::CompileError;
use ever_block::types::ExceptionCode;
use ever_vm::{
    int,
    stack::{Stack, StackItem, integer::IntegerData},
};

mod test_assembler_output {
    use super::*;

    #[test]
    fn test_commands_written_in_a_line() {
        test_case("PUSHINT 1 PUSHINT 2").expect_int_stack(&[1, 2]);
    }

    #[test]
    fn test_command_typo_in_a_line() {
        test_case("PUSHINT 1 PSHINT 2").expect_compilation_failure(
            CompileError::unknown(1, 11, "PSHINT")
        );
    }
}

fn test() {
    let mut file = match File::open("tests/test.in") {
        Ok(r) => r,
        Err(e) => {
            panic!("Execution error {}", e);
        }
    };

    println!("\nCompiling...");

    let mut source = String::new();
    match file.read_to_string(&mut source) {
        Ok(_r) => (),
        Err(e) => {
            panic!("Execution error {}", e);
        }
    }

    test_case(&source)
    .expect_stack(
        Stack::new()
            .push(int!(-2))
            .push(int!(1))
            .push(int!(2))
            .push(int!(1))
            .push(int!(2))
            .push(int!(2))
            .push(int!(18))
            .push(int!(1)),
    );
}

#[test]
fn test_assembler_executor() {
    test()
}

mod test_setcp {
    use super::*;

    #[test]
    fn test_setcp_out_of_range_failure() {
        test_case("SETCP 240")
        .expect_compilation_failure(CompileError::out_of_range(1, 1, "SETCP", "arg 0"));

        test_case("SETCP -16")
        .expect_compilation_failure(CompileError::out_of_range(1, 1, "SETCP", "arg 0"));
    }

    #[test]
    fn test_setcp_stack_failure() {
        test_case("SETCPX").expect_failure(ExceptionCode::StackUnderflow);

        test_case("PUSHCONT {} SETCPX").expect_failure(ExceptionCode::TypeCheckError);

        // awaiting fix in numbers
        // test_case("PUSHNAN SETCPX").expect_failure(ExceptionCode::RangeCheckError);

        test_case("PUSHINT 32768 SETCPX").expect_failure(ExceptionCode::RangeCheckError);

        test_case("PUSHINT -32769 SETCPX").expect_failure(ExceptionCode::RangeCheckError);

    }

    #[test]
    fn test_setcp_success() {
        test_case("SETCP0").expect_bytecode(vec![0xFF, 0x00, 0x80]);

        test_case("SETCP 239").expect_bytecode(vec![0xFF, 0xEF, 0x80]);
        test_case("SETCP -15").expect_bytecode(vec![0xFF, 0xF1, 0x80]);
        test_case("SETCP -1").expect_bytecode(vec![0xFF, 0xFF, 0x80]);

        test_case("SETCPX").expect_bytecode(vec![0xFF, 0xF0, 0x80]);
    }
}