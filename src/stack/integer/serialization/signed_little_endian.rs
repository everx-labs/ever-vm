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

use super::{
    IntegerData,
    common::*,
    Encoding
};
use types::{
    ExceptionCode,
    Result,
    TvmError,
};
use stack::BuilderData;
use stack::serialization::{
    Deserializer,
    Serializer,
};
use num::{
    BigInt,
    bigint::{
        ToBigInt, 
    }
};
use num_traits::Signed;

pub struct SignedIntegerLittleEndianEncoding {
    length_in_bits: usize
}

impl Encoding for SignedIntegerLittleEndianEncoding {
    fn new(length_in_bits: usize) -> SignedIntegerLittleEndianEncoding {
        SignedIntegerLittleEndianEncoding { length_in_bits }
    }
}

impl Serializer<IntegerData> for SignedIntegerLittleEndianEncoding {
    fn try_serialize(&self, value: &IntegerData) -> Result<BuilderData> {
        if !value.fits_in(self.length_in_bits) {
            // Spec. 3.2.7
            // * If the integer x to be serialized is not in the range
            //   −2^(n−1) <= x < 2^(n−1) (for signed integer serialization)
            //   or 0 <= x < 2^n (for unsigned integer serialization),
            //   a range check exception is usually generated
            return err!(ExceptionCode::RangeCheckError);
        }

        let value = value.take_value_of(|x| x.to_bigint())?;
        let mut bytes = value.to_signed_bytes_le();
        bytes = extend_buffer_le(bytes, self.length_in_bits, value.is_negative());

        BuilderData::with_raw(bytes, self.length_in_bits)
            .map_err(|err| err.into())
    }
}

impl Deserializer<IntegerData> for SignedIntegerLittleEndianEncoding {
    fn deserialize(&self, data: &[u8]) -> IntegerData {
        debug_assert!(data.len() * 8 >= self.length_in_bits);

        let value = BigInt::from_signed_bytes_le(data);
        IntegerData::from(value).expect("Should always fit")
    }
}
