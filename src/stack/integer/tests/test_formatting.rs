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

#[test]
fn test_formatting() {
    let value = IntegerData::from_u32(180149778);

    assert_eq!("180149778", value.to_str());
    assert_eq!("abcde12", value.to_str_radix(16));
    assert_eq!("1010101111001101111000010010", value.to_str_radix(2));

    assert_eq!(value.to_str(), format!("{}", value));
    assert_eq!(value.to_str_radix(16), format!("{:x}", value));
    assert_eq!(value.to_str_radix(16).to_uppercase(), format!("{:X}", value));
    assert_eq!(value.to_str_radix(2), format!("{:b}", value));
}