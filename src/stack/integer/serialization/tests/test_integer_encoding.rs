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

use crate::stack::{
    BuilderData, SliceData,
    integer::{IntegerData, serialization::{Encoding, SignedIntegerBigEndianEncoding}},
    serialization::{Serializer, Deserializer}
};

#[test]
fn encoding_one_positive_byte() {
    let src = IntegerData::from_u32(99);

    let encoding = SignedIntegerBigEndianEncoding::new(8);
    let a = encoding.try_serialize(&src).unwrap();
    let b = BuilderData::with_raw(vec![0b01100011], 8).unwrap();
    assert_eq!(a, b);

    let mut a = SliceData::load_builder(a).unwrap();
    let value = encoding.deserialize(&a.get_next_bits(8).unwrap());
    assert_eq!(src, value);
}

#[test]
fn encoding_one_negative_byte() {
    let src = IntegerData::from_i32(-99);

    let encoding = SignedIntegerBigEndianEncoding::new(8);
    let a = encoding.try_serialize(&src).unwrap();
    let b = BuilderData::with_raw(vec![0b10011101], 8).unwrap();
    assert_eq!(a, b);

    let mut a = SliceData::load_builder(a).unwrap();
    let value = encoding.deserialize(&a.get_next_bits(8).unwrap());
    assert_eq!(src, value);
}

#[test]
fn encoding_two_positive_bytes() {
    let src = IntegerData::from_u32(99);

    let encoding = SignedIntegerBigEndianEncoding::new(16);
    let a = encoding.try_serialize(&src).unwrap();
    let b = BuilderData::with_raw(vec![0b00000000, 0b01100011], 16).unwrap();
    assert_eq!(a, b);

    let mut a = SliceData::load_builder(a).unwrap();
    let value = encoding.deserialize(&a.get_next_bits(16).unwrap());
    assert_eq!(src, value);
}

#[test]
fn encoding_two_negative_bytes() {
    let src = IntegerData::from_i32(-99);

    let encoding = SignedIntegerBigEndianEncoding::new(16);
    let a = encoding.try_serialize(&src).unwrap();
    let b = BuilderData::with_raw(vec![0b11111111, 0b10011101], 16).unwrap();
    assert_eq!(a, b);

    let mut a = SliceData::load_builder(a).unwrap();
    let value = encoding.deserialize(&a.get_next_bits(16).unwrap());
    assert_eq!(src, value);
}
