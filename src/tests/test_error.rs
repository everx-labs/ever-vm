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

use super::*;
use ever_block::{error, fail};

#[test]
fn test_tvm_exception_code() {
    let err = exception!(ExceptionCode::RangeCheckError);
    assert_eq!(tvm_exception_code(&err).unwrap(), ExceptionCode::RangeCheckError);
    let err = exception!(ExceptionCode::RangeCheckError, "just a text");
    assert_eq!(tvm_exception_code(&err).unwrap(), ExceptionCode::RangeCheckError);
    let err = exception!(ExceptionCode::RangeCheckError, "text with format {} - {}", 123, 456);
    assert_eq!(tvm_exception_code(&err).unwrap(), ExceptionCode::RangeCheckError);

    let err = || -> Result<()> { fail!(ExceptionCode::RangeCheckError) }().unwrap_err();
    assert_eq!(tvm_exception_code(&err).unwrap(), ExceptionCode::RangeCheckError);
    let err = || -> Result<()> { fail!("just a text") }().unwrap_err();
    assert_eq!(tvm_exception_code(&err), None);
}

#[test]
fn test_update_error() {
    let err = exception!(ExceptionCode::RangeCheckError, "description {}", 42);
    println!("{:?}", err);
    let err = update_error_description(err, |d| format!("additional: {}", d));
    println!("{:?}", err);
    assert_eq!(tvm_exception_code(&err).unwrap(), ExceptionCode::RangeCheckError);
    assert!(err.to_string().contains("additional: "));

    // TODO: make fail! more informative
    // let err = || -> Result<()> { fail!(ExceptionCode::RangeCheckError, "lost description {}", 0) }().unwrap_err();
    // println!("{:?}", err);
    // let err = update_error_description(err, |d| format!("additional: {}", d));
    // println!("{:?}", err);
    // assert_eq!(tvm_exception_code(&err).unwrap(), ExceptionCode::RangeCheckError);

    let err = || -> Result<()> { custom_err!(112, "text with format {} - {}", 123, 456) }().unwrap_err();
    println!("{:?}", err);
    let err = update_error_description(err, |d| format!("additional: {}", d));
    println!("{:?}", err);
    assert_eq!(tvm_exception_code(&err), None);
    assert_eq!(tvm_exception_or_custom_code(&err), 112);
}
