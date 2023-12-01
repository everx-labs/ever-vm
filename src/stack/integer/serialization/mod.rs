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

use crate::stack::{
    integer::IntegerData,
    serialization::{Serializer, Deserializer}
};

pub trait Encoding : Serializer<IntegerData> + Deserializer<IntegerData> {
    fn new(length_in_bits: usize) -> Self;
}

pub mod common;
mod signed_big_endian;
mod unsigned_big_endian;
mod signed_little_endian;
mod unsigned_little_endian;

pub use self::unsigned_little_endian::UnsignedIntegerLittleEndianEncoding;
pub use self::unsigned_big_endian::UnsignedIntegerBigEndianEncoding;
pub use self::signed_big_endian::SignedIntegerBigEndianEncoding;
pub use self::signed_little_endian::SignedIntegerLittleEndianEncoding;

#[cfg(test)]
#[path = "tests/test_integer_encoding.rs"]
mod test_integer_encoding;

#[cfg(test)]
#[path = "tests/test_ser_deser.rs"]
mod test_ser_deser;
