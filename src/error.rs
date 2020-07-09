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

use ton_types::{error, fail, Result, ExceptionCode};
use crate::{types::Exception, stack::StackItem};

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
    #[fail(display = "VM Exception: {} {}", 0, 1)]
    TvmExceptionFull(Exception, String),
}

pub fn tvm_exception(err: failure::Error) -> Result<Exception> {
    match err.downcast::<TvmError>() {
        Ok(TvmError::TvmExceptionFull(err, _)) => Ok(err),
        Ok(TvmError::TvmException(err)) => Ok(Exception::from(err)),
        Ok(err) => fail!(err),
        Err(err) => if let Some(err) = err.downcast_ref::<ton_types::types::ExceptionCode>() {
            Ok(Exception::from(*err))
        } else {
            Err(err)
        }
    }
}

pub fn tvm_exception_code(err: &failure::Error) -> ExceptionCode {
    match err.downcast_ref::<TvmError>() {
        Some(TvmError::TvmExceptionFull(err, _)) => err.code,
        Some(TvmError::TvmException(err)) => *err,
        Some(_) => ExceptionCode::UnknownError,
        None => if let Some(err) = err.downcast_ref::<ton_types::types::ExceptionCode>() {
            *err
        } else {
            ExceptionCode::UnknownError
        }
    }
}

pub fn tvm_exception_code_and_value(err: &failure::Error) -> (i32, ExceptionCode, StackItem) {
    match err.downcast_ref::<TvmError>() {
        Some(TvmError::TvmExceptionFull(err, _)) => (err.number as i32, err.code, err.value.clone()),
        Some(TvmError::TvmException(err)) => (*err as i32, *err, StackItem::None),
        Some(_) => (-1, ExceptionCode::UnknownError, StackItem::None),
        None => if let Some(err) = err.downcast_ref::<ExceptionCode>() {
            (*err as i32, *err, StackItem::None)
        } else {
            (-1, ExceptionCode::UnknownError, StackItem::None)
        }
    }
}