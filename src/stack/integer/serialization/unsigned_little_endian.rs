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

use crate::{
    error::TvmError,
    stack::{
        integer::{
            serialization::{
                common::bits_to_bytes,
                Encoding,
            },
            IntegerData,
        },
        serialization::{Deserializer, Serializer},
    },
    types::Exception,
};
use ton_types::{error, BuilderData, ExceptionCode, Result};

pub struct UnsignedIntegerLittleEndianEncoding {
    length_in_bits: usize
}

impl Encoding for UnsignedIntegerLittleEndianEncoding {
    fn new(length_in_bits: usize) -> UnsignedIntegerLittleEndianEncoding {
        UnsignedIntegerLittleEndianEncoding { length_in_bits }
    }
}

impl Serializer<IntegerData> for UnsignedIntegerLittleEndianEncoding {
    fn try_serialize(&self, value: &IntegerData) -> Result<BuilderData> {
        if value.is_neg() || !value.ufits_in(self.length_in_bits) {
            // Spec. 3.2.7
            // * If the integer x to be serialized is not in the range
            //   −2^(n−1) <= x < 2^(n−1) (for signed integer serialization)
            //   or 0 <= x < 2^n (for unsigned integer serialization),
            //   a range check exception is usually generated
            return err!(ExceptionCode::RangeCheckError, "{} is not fit in {}", value, self.length_in_bits)
        }

        let value = value.take_value_of(|x| x.to_biguint())?;
        let mut buffer = value.to_bytes_le();
        let expected_buffer_size = bits_to_bytes(self.length_in_bits);
        debug_assert!(expected_buffer_size >= buffer.len());
        buffer.resize(expected_buffer_size, 0);

        BuilderData::with_raw(buffer, self.length_in_bits)
    }
}

impl Deserializer<IntegerData> for UnsignedIntegerLittleEndianEncoding {
    fn deserialize(&self, data: &[u8]) -> IntegerData {
        debug_assert!(data.len() * 8 >= self.length_in_bits);

        let value = num::BigInt::from_bytes_le(num::bigint::Sign::Plus, data);
        IntegerData::from(value).unwrap_or_default()
    }
}

