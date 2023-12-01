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
    SliceData,
    integer::{
        IntegerData,
        serialization::{
            Encoding, SignedIntegerBigEndianEncoding, SignedIntegerLittleEndianEncoding,
            UnsignedIntegerBigEndianEncoding, UnsignedIntegerLittleEndianEncoding
        }
    }
};

#[test]
fn test_signed_big_endian_ser_deser() {
    test_ser_deser::<SignedIntegerBigEndianEncoding>();
}

#[test]
fn test_unsigned_big_endian_ser_deser() {
    test_ser_deser::<UnsignedIntegerBigEndianEncoding>();
}

#[test]
fn test_signed_little_endian_ser_deser() {
    test_ser_deser::<SignedIntegerLittleEndianEncoding>();
}

#[test]
fn test_unsigned_little_endian_ser_deser() {
    test_ser_deser::<UnsignedIntegerLittleEndianEncoding>();
}

fn test_ser_deser<T>()
where T: Encoding
{
    let initial = IntegerData::from_str_radix("18AB_C0435ACE", 16).unwrap();

    let encoding = T::new(46);
    let data = encoding.try_serialize(&initial).unwrap();
    let mut data = SliceData::load_builder(data).unwrap();
    let resulted = encoding.deserialize(&data.get_next_bits(46).unwrap());

    assert_eq!(initial, resulted);
}