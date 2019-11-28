/*
* Copyright 2018-2019 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.  You may obtain a copy of the
* License at: https://ton.dev/licenses
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use ::int;
use stack::{IntegerData, StackItem};
use std::fmt;
use std::result;
use std::str;
use std::error::Error;
use std::sync::Arc;
pub use ton_types::types::*;

pub const ACTION_SEND_MSG: u32 = 0x0ec3c86d;
pub const ACTION_SET_CODE: u32 = 0xad4de08e;
pub const ACTION_RESERVE:  u32 = 0x36e6b809;

#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct UInt256([u8; 32]);

impl fmt::Debug for UInt256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

impl fmt::LowerHex for UInt256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

impl fmt::UpperHex for UInt256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode_upper(&self.0))
    }
}

// Exceptions *****************************************************************
#[derive(PartialEq)]
pub struct Exception {
    pub code: ExceptionCode,
    pub number: usize,
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
            "{}, file {}, line {}",
            exception_message(self.number, &self.value),
            self.file,
            self.line
        )
    }
    pub fn from_code(code: ExceptionCode, file: &'static str, line: u32) -> Exception {
        Exception {
            code,
            number: code as usize,
            value: int!(0),
            file,
            line,
        }
    }
    pub fn from_number_and_value(
        number: usize, 
        value: StackItem,
        file: &'static str, 
        line: u32
    ) -> Exception {
        Exception {
            code: ExceptionCode::from_usize(number).unwrap_or(ExceptionCode::UnknownError),
            number: number,
            value: value,
            file,
            line,
        }
    }
}

pub fn exception_message(number: usize, value: &StackItem) -> String {
    match ExceptionCode::from_usize(number) {
        Some(code) => code.message().to_owned(),
        _ => format!("unknown exception (number {}, value {})", number, value),
    }
}

#[macro_export]
macro_rules! exception {
    ($code:expr) => {
        $crate::types::Exception::from_code($code, file!(), line!())
    };
    ($code:expr, $file:expr, $line:expr) => {
        $crate::types::Exception::from_code($code, $file, $line)
    };
}

#[macro_export]
macro_rules! err {
    ($code:expr) => {
        Err(exception!($code, file!(), line!()))
    };
    ($code:expr, $file:expr, $line:expr) => {
        Err(exception!($code, $file, $line))
    };
}

#[macro_export]
macro_rules! err_opt {
    ($code:expr) => {
        Some(exception!($code))
    };

    ($number:expr, $value:expr) => {
        Some(exception!($number, $value))
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
        Exception::fmt(self, f)
    }
}

impl fmt::Debug for Exception {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Exception::fmt(self, f)
    }
}

impl Error for Exception {
    fn description(&self) -> &str {
        self.code.message()
    }

    fn cause(&self) -> Option<&dyn Error> {
        None
    }

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }    
}

pub(crate) type Failure = Option<Exception>;
pub(crate) type Result<T> = result::Result<T, Exception>;
pub(crate) type ResultMut<'a, T> = Result<&'a mut T>;
pub(crate) type ResultOpt<T> = Result<Option<T>>;
pub(crate) type ResultRef<'a, T> = Result<&'a T>;
pub(crate) type ResultVec<T> = Result<Vec<T>>;
pub(crate) type Status = Result<()>;
