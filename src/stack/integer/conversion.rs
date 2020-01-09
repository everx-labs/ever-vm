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

use super::*;
use types::{
    ExceptionCode,
    Result,
};
use num_traits::{
    Num,
};
use super::serialization::common::bits_to_bytes;

impl IntegerData {
    /// Constructs new IntegerData from u32 in a fastest way.
    #[inline]
    pub fn from_u32(value: u32) -> IntegerData {
        if value == 0 {
            return Self::zero();
        }
        IntegerData {
            value: IntegerValue::Value(
                Int::new(Sign::Plus, vec![value])
            )
        }
    }

    /// Constructs new IntegerData from i32 in a fastest way.
    #[inline]
    pub fn from_i32(value: i32) -> IntegerData {
        if value == 0 {
            return Self::zero();
        }
        IntegerData {
            value: IntegerValue::Value(
                Int::new(
                    if value < 0 {
                        Sign::Minus
                    } else {
                        Sign::Plus
                    }, vec![value.abs() as u32])
            )
        }
    }

    /// Constructs new IntegerData from u64 in a fastest way.
    #[inline]
    pub fn from_u64(value: u64) -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(
                Int::from(value)
            )
        }
    }

    /// Constructs new IntegerData from i64 in a fastest way.
    #[inline]
    pub fn from_i64(value: i64) -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(
                Int::from(value)
            )
        }
    }

    /// Constructs new IntegerData from u128 in a fastest way.
    #[inline]
    pub fn from_u128(value: u128) -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(
                Int::from(value)
            )
        }
    }

    /// Constructs new IntegerData from i128 in a fastest way.
    #[inline]
    pub fn from_i128(value: i128) -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(
                Int::from(value)
            )
        }
    }

    /// Constructs new IntegerData value from the given one of another supported type.
    #[inline]
    pub fn from<T: Into<Int>>(value: T) -> Result<IntegerData> {
        let bigint: Int = value.into();
        if !check_overflow(&bigint) {
            return err!(ExceptionCode::IntegerOverflow);
        }
        Ok(IntegerData {
            value: IntegerValue::Value(bigint)
        })
    }

    /// Constructs new IntegerData value from the little-endian slice of u32
    /// without overflow checking.
    #[inline]
    pub fn from_vec_le_unchecked(sign: Sign, digits: Vec<u32>) -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(Int::new(sign, digits))
        }
    }

    /// Constructs new IntegerData value from the little-endian slice of u32
    /// with overflow checking.
    #[inline]
    pub fn from_vec_le(sign: Sign, digits: Vec<u32>) -> Result<IntegerData> {
        let bigint = Int::new(sign, digits);
        if !check_overflow(&bigint) {
            return err!(ExceptionCode::IntegerOverflow);
        }
        Ok(IntegerData {
            value: IntegerValue::Value(bigint)
        })
    }

    /// Parses string literal using radix 10 and constructs new IntegerData.
    pub fn from_str(literal: &str) -> Result<IntegerData> {
        Self::from_str_radix(literal, 10)
    }

    /// Parses string literal with given radix and constructs new IntegerData.
    pub fn from_str_radix(literal: &str, radix: u32) -> Result<IntegerData> {
        match Int::from_str_radix(literal, radix) {
            Ok(value) => Self::from(value),
            Err(_) => err!(ExceptionCode::TypeCheckError),
        }
    }

    /// Returns value converted into given type with range checking.
    pub fn into<T>(&self, range: std::ops::RangeInclusive<T>) -> Result<T>
    where
        T: PartialOrd + std::fmt::Display + FromInt
    {
        match self.value {
            IntegerValue::NaN => err!(ExceptionCode::RangeCheckError),
            IntegerValue::Value(ref value) => T::from_int(value)
                .and_then(|ret| {
                    if *range.start() > ret || *range.end() < ret {
                        return err!(ExceptionCode::RangeCheckError);
                    }
                    Ok(ret)
                })
        }
    }

    /// Extracts internal value using conversion function. Returns IntegerOverflow exception on NaN.
    #[inline]
    pub fn take_value_of<T>(&self, convert: impl Fn(&Int) -> Option<T>) -> Result<T> {
        match self.value {
            IntegerValue::NaN => err!(ExceptionCode::IntegerOverflow),
            IntegerValue::Value(ref value) => {
                if let Some(value) = convert(value) {
                    return Ok(value);
                } else {
                    return err!(ExceptionCode::RangeCheckError);
                }
            }
        }
    }
}

impl IntegerData {
    /// Decodes value from big endian octet string for PUSHINT primitive using the format
    /// from TVM Spec A.3.1:
    ///  "82lxxx — PUSHINT xxx, where 5-bit 0 ≤ l ≤ 30 determines the length n = 8l + 19
    ///  of signed big-endian integer xxx. The total length of this instruction
    ///  is l + 4 bytes or n + 13 = 8l + 32 bits."
    pub fn from_big_endian_octet_stream<F>(mut get_next_byte: F) -> Result<IntegerData>
    where
        F: FnMut() -> Result<u8>
    {
        let first_byte = get_next_byte()?;
        let byte_len = ((first_byte & 0b11111000u8) as usize >> 3) + 3;
        let greatest3bits = (first_byte & 0b111) as u32;
        let digit_count = (byte_len + 3) >> 2;
        let mut digits: Vec<u32> = vec![0; digit_count];
        let (sign, mut value) = if greatest3bits & 0b100 == 0 {
            (Sign::Plus, greatest3bits)
        } else {
            (Sign::Minus, 0xFFFF_FFF8u32 | greatest3bits)
        };

        let mut upper = byte_len & 0b11;
        if upper == 0 {
            upper = 4;
        }
        for _ in 1..upper {
            value <<= 8;
            value |= get_next_byte()? as u32;
        }
        let last_index = digit_count - 1;
        digits[last_index] = value;

        for i in (0..last_index).rev() {
            let mut value = (get_next_byte()? as u32) << 24;
            value |= (get_next_byte()? as u32) << 16;
            value |= (get_next_byte()? as u32) << 8;
            value |= get_next_byte()? as u32;

            digits[i] = value;
        }

        if sign == Sign::Minus {
            twos_complement(&mut digits);
        }
        Ok(IntegerData::from_vec_le_unchecked(sign, digits))
    }

    /// Encodes value as big endian octet string for PUSHINT primitive using the format
    /// from TVM Spec A.3.1:
    ///  "82lxxx — PUSHINT xxx, where 5-bit 0 ≤ l ≤ 30 determines the length n = 8l + 19
    ///  of signed big-endian integer xxx. The total length of this instruction
    ///  is l + 4 bytes or n + 13 = 8l + 32 bits."
    pub fn to_big_endian_octet_string(&self) -> Vec<u8> {
        process_value(self, |value| -> Vec<u8> {
            let mut n = self.bitsize();
            if n < 19 {
                n = 19;
            } else {
                let excessive = n & 0b111;
                if excessive == 0 || excessive > 3 {
                    // Rounding to full octet and adding 3.
                    n = (((n + 7) as isize & -8) + 3) as usize;
                } else {
                    n += 3 - excessive;
                }
            };

            let bytelen = bits_to_bytes(n);
            let mut serialized_val = value.to_signed_bytes_be();
            let prefixlen = bytelen - serialized_val.len();
            let mut ret: Vec<u8> = Vec::with_capacity(bytelen);
            let is_negative = value.is_negative();
            let mut prefix: Vec<u8> = if prefixlen == 0 {
                let new_serialized_val = serialized_val.split_off(1);
                let first_element = serialized_val;
                serialized_val = new_serialized_val;
                first_element
            } else if is_negative {
                vec![0xFF; prefixlen]
            } else {
                vec![0x00; prefixlen]
            };
            debug_assert_eq!((n - 19) & 0b111, 0);
            prefix[0] = (n - 19) as u8 | (prefix[0] & 0b111);

            ret.append(&mut prefix);
            ret.append(&mut serialized_val);
            ret
        })
    }
}

impl From<u32> for IntegerData {
    fn from(value: u32) -> Self {
        IntegerData::from_u32(value)
    }
}

impl From<i32> for IntegerData {
    fn from(value: i32) -> Self {
        IntegerData::from_i32(value)
    }
}

impl From<u64> for IntegerData {
    fn from(value: u64) -> Self {
        IntegerData::from_u64(value)
    }
}

impl From<i64> for IntegerData {
    fn from(value: i64) -> Self {
        IntegerData::from_i64(value)
    }
}

impl From<u128> for IntegerData {
    fn from(value: u128) -> Self {
        IntegerData::from_u128(value)
    }
}

impl From<i128> for IntegerData {
    fn from(value: i128) -> Self {
        IntegerData::from_i128(value)
    }
}

pub trait FromInt {
    fn from_int(value: &Int) -> Result<Self>
    where
        Self: std::marker::Sized;
}

macro_rules! auto_from_int {
    ($($to:ty : $f:tt),+) => {
        $(
            impl FromInt for $to {
                fn from_int(value: &Int) -> Result<$to> {
                    if let Some(x) = <dyn num::ToPrimitive>::$f(value) {
                        Ok(x)
                    } else {
                        err!($crate::types::ExceptionCode::RangeCheckError)
                    }
                }
            }
        )*
    }
}

auto_from_int!{
    u8: to_u8,
    i8: to_i8,
    u16: to_u16,
    i16: to_i16,
    u32: to_u32,
    i32: to_i32,
    u64: to_u64,
    i64: to_i64,
    u128: to_u128,
    i128: to_i128,
    usize: to_usize,
    isize: to_isize
}
