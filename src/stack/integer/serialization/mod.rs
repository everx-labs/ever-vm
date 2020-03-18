/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
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

use types::{ExceptionCode, Result, TvmError};
use stack::serialization::{Serializer, Deserializer};
use stack::{BuilderData, IntegerData, SliceData};

pub trait Encoding : Serializer<IntegerData> + Deserializer<IntegerData>
{
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

pub trait IntoSliceExt {
    fn into_slice<T: Encoding>(&self, bits: usize) -> Result<SliceData>;
    fn into_builder<T: Encoding>(&self, bits: usize) -> Result<BuilderData>;
}

impl IntoSliceExt for IntegerData {
    fn into_slice<T: Encoding>(&self, bits: usize) -> Result<SliceData> {
        self.into_builder::<T>(bits).map(|builder| builder.into())
    }

    fn into_builder<T: Encoding>(&self, bits: usize) -> Result<BuilderData> {
        if self.is_nan() {
            err!(ExceptionCode::RangeCheckError)
        } else {
            T::new(bits).try_serialize(self)
        }
    }

}

