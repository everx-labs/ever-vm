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
    IntegerValue,
};
use std::fmt;

impl IntegerData {
    /// Converts value into String with given radix.
    pub fn to_str_radix(&self, radix: u32) -> String {
        match self.value {
            IntegerValue::NaN => "NaN".to_string(),
            IntegerValue::Value(ref value) => value.to_str_radix(radix),
        }
    }

    /// Converts value into String.
    pub fn to_str(&self) -> String {
        self.to_str_radix(10)
    }
}

impl fmt::Display for IntegerData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl fmt::LowerHex for IntegerData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str_radix(16))
    }
}

impl fmt::UpperHex for IntegerData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str_radix(16).to_uppercase())
    }
}

impl fmt::Binary for IntegerData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str_radix(2))
    }
}
