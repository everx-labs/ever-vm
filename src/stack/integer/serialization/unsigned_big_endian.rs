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

use crate::stack::{
    BuilderData,
    integer::{
        IntegerData,
        serialization::{Encoding, common::{calc_excess_bits, extend_buffer_be}}
    },
    serialization::{Serializer, Deserializer}
};
use ton_types::{error, Result, types::ExceptionCode, fail};

pub struct UnsignedIntegerBigEndianEncoding {
    length_in_bits: usize
}

impl Encoding for UnsignedIntegerBigEndianEncoding {
    fn new(length_in_bits: usize) -> UnsignedIntegerBigEndianEncoding {
        UnsignedIntegerBigEndianEncoding { length_in_bits }
    }
}

impl Serializer<IntegerData> for UnsignedIntegerBigEndianEncoding {
    fn try_serialize(&self, value: &IntegerData) -> Result<BuilderData> {
        if value.is_neg() || !value.ufits_in(self.length_in_bits) {
            // Spec. 3.2.7
            // * If the integer x to be serialized is not in the range
            //   −2^(n−1) <= x < 2^(n−1) (for signed integer serialization)
            //   or 0 <= x < 2^n (for unsigned integer serialization),
            //   a range check exception is usually generated
            fail!(ExceptionCode::RangeCheckError)
        }

        let mut value = value.take_value_of(|x| x.to_biguint())?;

        let excess_bits = calc_excess_bits(self.length_in_bits);
        if excess_bits != 0 {
            value <<= 8 - excess_bits;
        }

        let mut buffer = value.to_bytes_be();
        buffer = extend_buffer_be(buffer, self.length_in_bits, false);

        BuilderData::with_raw(buffer, self.length_in_bits)
    }
}

impl Deserializer<IntegerData> for UnsignedIntegerBigEndianEncoding {
    fn deserialize(&self, data: &[u8]) -> IntegerData {
        debug_assert!(data.len() * 8 >= self.length_in_bits);

        let mut value = num::BigInt::from_bytes_be(num::bigint::Sign::Plus, data);
        let excess_bits = calc_excess_bits(self.length_in_bits);
        if excess_bits != 0 {
            value >>= 8 - excess_bits;
        }

        IntegerData::from(value).expect("Should always fit")
    }
}
