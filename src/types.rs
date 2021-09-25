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

use crate::stack::{StackItem, integer::IntegerData};
use std::fmt;
use ton_types::{Result, types::ExceptionCode};

pub const ACTION_SEND_MSG: u32 = 0x0ec3c86d;
pub const ACTION_SET_CODE: u32 = 0xad4de08e;
pub const ACTION_RESERVE:  u32 = 0x36e6b809;
pub const ACTION_CHANGE_LIB: u32 = 0x26fa1dd4;

#[derive(Clone, PartialEq)]
enum ExceptionType {
    System(ExceptionCode),
    Custom(i32)
}

impl ExceptionType {
    fn is_normal_termination(&self) -> Option<i32> {
        match self {
            ExceptionType::System(ExceptionCode::NormalTermination) | ExceptionType::Custom(0) => Some(0),
            ExceptionType::System(ExceptionCode::AlternativeTermination) | ExceptionType::Custom(1) => Some(1),
            _ => None
        }
    }
    fn exception_code(&self) -> Option<ExceptionCode> {
        if let ExceptionType::System(code) = self {
            Some(*code)
        } else {
            None
        }
    }
    fn custom_code(&self) -> Option<i32> {
        if let ExceptionType::Custom(code) = self {
            Some(*code)
        } else {
            None
        }
    }
    pub fn exception_or_custom_code(&self) -> i32 {
        match self {
            ExceptionType::System(code) => *code as i32,
            ExceptionType::Custom(code) => *code
        }
    }
    fn exception_message(&self) -> String {
        match self {
            ExceptionType::System(code) => format!("{}, code {}", code, *code as u8),
            ExceptionType::Custom(code) => format!("code {}", code)
        }
    }
}

// Exceptions *****************************************************************
#[derive(Clone, PartialEq)]
pub struct Exception {
    exception: ExceptionType,
    pub value: StackItem,
    pub file: &'static str,
    pub line: u32,
}

impl From<ExceptionCode> for Exception {
    fn from(code: ExceptionCode) -> Self {
        Exception::from_code(code, file!(), line!())
    }
}

impl Exception {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}, value {}, file {}:{}",
            self.exception.exception_message(),
            self.value,
            self.file,
            self.line
        )
    }
    pub fn from_code(code: ExceptionCode, file: &'static str, line: u32) -> Exception {
        Self::from_code_and_value(code, 0, file, line)
    }
    pub fn from_code_and_value(code: ExceptionCode, value: impl Into<IntegerData>, file: &'static str, line: u32) -> Exception {
        // panic!("{} {} {}:{}", code, IntegerData::from(value), file, line)
        Exception {
            exception: ExceptionType::System(code),
            value: StackItem::integer(value.into()),
            file,
            line,
        }
    }
    pub fn from_number_and_value(number: usize, value: StackItem, file: &'static str, line: u32) -> Exception {
        Exception {
            exception: ExceptionType::Custom(number as i32),
            value,
            file,
            line,
        }
    }
    pub fn exception_code(&self) -> Option<ExceptionCode> {
        self.exception.exception_code()
    }
    pub fn custom_code(&self) -> Option<i32> {
        self.exception.custom_code()
    }
    pub fn exception_or_custom_code(&self) -> i32 {
        self.exception.exception_or_custom_code()
    }
    pub fn is_normal_termination(&self) -> Option<i32> {
        self.exception.is_normal_termination()
    }
}

#[macro_export]
macro_rules! exception {
    ($code:expr) => {
        error!(TvmError::TvmExceptionFull(Exception::from_code($code, file!(), line!()), String::new()))
    };
    ($code:expr, $msg:literal, $($arg:tt)*) => {
        error!(TvmError::TvmExceptionFull(Exception::from_code($code, file!(), line!()), format!($msg, $($arg)*)))
    };
    ($code:expr, $value:expr, $msg:literal, $($arg:tt)*) => {
        error!(TvmError::TvmExceptionFull(Exception::from_code_and_value($code, $value, file!(), line!()), format!($msg, $($arg)*)))
    };
    ($code:expr, $value:expr, $msg:literal) => {
        error!(TvmError::TvmExceptionFull(Exception::from_code_and_value($code, $value, file!(), line!()), format!($msg)))
    };
    ($code:expr, $msg:literal) => {
        error!(TvmError::TvmExceptionFull(Exception::from_code($code, file!(), line!()), format!($msg)))
    };
    ($code:expr, $file:expr, $line:expr) => {
        error!(TvmError::TvmExceptionFull(Exception::from_code($code, $file, $line), String::new()))
    };
}

#[macro_export]
macro_rules! err {
    ($code:expr) => {
        Err(exception!($code))
    };
    ($code:expr, $msg:literal, $($arg:tt)*) => {{
        Err(exception!($code, $msg, $($arg)*))
    }};
    ($msg:literal, $($arg:tt)*) => {{
        Err(exception!(ExceptionCode::FatalError, $msg, $($arg)*))
    }};
    ($code:expr, $msg:literal) => {{
        Err(exception!($code, $msg))
    }};
    ($code:expr, $file:expr, $line:expr) => {
        Err(exception!($code, $file, $line))
    };
}

#[macro_export]
macro_rules! err_opt {
    ($code:expr) => {
        Some(exception!($code))
    };
}

#[macro_export]
macro_rules! opt {
    ($from:expr) => {
        match $from {
            Some(e) => return Some(e),
            None => (),
        }
    };
}

#[macro_export]
macro_rules! to_err {
    ($from:expr, $ok:expr) => {
        match $from {
            Some(e) => Err(e),
            None => Ok($ok),
        }
    };
}

#[macro_export]
macro_rules! to_opt {
    ($from:expr) => {
        match $from {
            Ok(_) => None,
            Err(e) => Some(e),
        }
    };
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, value: {}", self.exception.exception_message(), self.value)
    }
}

impl fmt::Debug for Exception {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Exception::fmt(self, f)
    }
}

// pub(crate) use ton_types::Result;
pub(crate) type ResultMut<'a, T> = Result<&'a mut T>;
pub(crate) type ResultOpt<T> = Result<Option<T>>;
pub(crate) type ResultRef<'a, T> = Result<&'a T>;
pub(crate) type ResultVec<T> = Result<Vec<T>>;
pub(crate) type Status = Result<()>;
