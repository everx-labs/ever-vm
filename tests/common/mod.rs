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

pub mod test_framework;
pub use test_framework::*;

use ever_block::GlobalCapabilities;
use ever_assembler::CompileError;
use ever_vm::stack::StackItem;
use ever_block::{BuilderData, SliceData, ExceptionCode};

#[allow(dead_code)]
pub mod create {
    use super::*;

    pub fn cell<T: AsRef<[u8]>>(data:T) -> StackItem {
        let data = data.as_ref().to_vec();
        StackItem::Cell(BuilderData::with_bitstring(data).unwrap().into_cell().unwrap())
    }

    pub fn builder<T: AsRef<[u8]>>(data:T) -> StackItem {
        let builder = BuilderData::with_bitstring(data.as_ref().to_vec()).unwrap();
        StackItem::builder(builder)
    }

    pub fn slice<T: AsRef<[u8]>>(data:T) -> StackItem {
        let data = data.as_ref().to_vec();
        let slice = SliceData::new(data);
        StackItem::Slice(slice)
    }

    pub fn tuple<T: AsRef<[StackItem]>>(data: &T) -> StackItem {
        let data = data.as_ref().to_vec();
        StackItem::tuple(data)
    }
}

#[allow(dead_code)]
pub fn test_single_argument_fail(cmd: &str, argument: isize) {
    let code = format!("{} {}", cmd, argument);
    test_case(code)
    .expect_compilation_failure(CompileError::out_of_range(1, 1, cmd, "arg 0"));
}

#[allow(dead_code)]
pub fn expect_exception(code: &str, exc_code: ExceptionCode) {
    test_case(code).expect_failure(exc_code);
}

#[allow(dead_code)]
pub fn expect_exception_with_capability(
    code: &str, 
    exc_code: ExceptionCode,
    capability: GlobalCapabilities,
    check_fift: bool
) {
    test_case(
        code, 
    )
    .with_capability(capability)
    .skip_fift_check(!check_fift)
    .expect_failure(exc_code);
}
