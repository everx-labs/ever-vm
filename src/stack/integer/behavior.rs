/*
* Copyright (C) 2019-2021 TON Labs. All Rights Reserved.
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

use crate::{error::TvmError, types::{Exception, Status}};
use ton_types::{error, types::ExceptionCode};

pub trait OperationBehavior {
    fn quiet() -> bool;
    fn name_prefix() -> Option<&'static str>;
    fn on_nan_parameter(file: &'static str, line: u32) -> Status;
    fn on_integer_overflow(file: &'static str, line: u32) -> Status;
    fn on_range_check_error(file: &'static str, line: u32) -> Status;
}

pub struct Signaling {}
pub struct Quiet {}

#[macro_export]
macro_rules! on_integer_overflow {
    ($T: ident) => {{
        $T::on_integer_overflow(file!(), line!())
    }}
}

#[macro_export]
macro_rules! on_nan_parameter {
    ($T: ident) => {{
        $T::on_nan_parameter(file!(), line!())
    }}
}

#[macro_export]
macro_rules! on_range_check_error {
    ($T: ident) => {{
        $T::on_range_check_error(file!(), line!())
    }}
}

impl OperationBehavior for Signaling {
    fn quiet() -> bool {
        false
    }
    fn name_prefix() -> Option<&'static str> {
        None
    }
    fn on_integer_overflow(file: &'static str, line: u32) -> Status {
        err!(ExceptionCode::IntegerOverflow, file, line)
    }
    fn on_nan_parameter(file: &'static str, line: u32) -> Status {
        err!(ExceptionCode::IntegerOverflow, file, line)
    }
    fn on_range_check_error(file: &'static str, line: u32) -> Status {
        err!(ExceptionCode::RangeCheckError, file, line)
    }
}

impl OperationBehavior for Quiet {
    fn quiet() -> bool {
        true
    }
    fn name_prefix() -> Option<&'static str> {
        Some("Q")
    }
    fn on_integer_overflow(_: &'static str, _: u32) -> Status {
        Ok(())
    }
    fn on_nan_parameter(_: &'static str, _: u32) -> Status {
        Ok(())
    }
    fn on_range_check_error(_: &'static str, _: u32) -> Status {
        Ok(())
    }
}
