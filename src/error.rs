/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use ton_types::types::ExceptionCode;
use crate::types::Exception;

#[derive(Debug, failure::Fail)]
pub enum TvmError {
    /// Fatal error.
    #[fail(display = "Fatal error: {}", 0)]
    FatalError(String),
    /// Invalid argument.
    #[fail(display = "Invalid argument: {}", 0)]
    InvalidArg(usize),
    /// Invalid data.
    #[fail(display = "Invalid data: {}", 0)]
    InvalidData(String),
    /// Invalid operation.
    #[fail(display = "Invalid operation: {}", 0)]
    InvalidOperation(String),
    /// TVM Exception
    #[fail(display = "VM Exception, code: {}", 0)]
    TvmException(ExceptionCode),
    /// TVM Exception description
    #[fail(display = "VM Exception: {}", 0)]
    TvmExceptionFull(Exception),
}

#[allow(dead_code)]
pub(crate) fn exception_code(err: failure::Error) -> Option<ExceptionCode> {
    if let Some(TvmError::TvmExceptionFull(err)) = err.downcast_ref() {
        Some(err.code)
    } else {
        None
    }
}