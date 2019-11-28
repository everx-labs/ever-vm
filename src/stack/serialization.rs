/*
* Copyright 2018-2019 TON DEV SOLUTIONS LTD.
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

use stack::BuilderData;

pub trait Deserializer<T> {
    /// Tries to deserialize a value from a bitstring
    /// Returns deserialized value if any and a remaining bitstring
    fn deserialize(&self, data: &[u8]) -> T;
}

pub trait Serializer<T> {
    type Error;
    fn try_serialize(&self, value: &T) -> Result<BuilderData, Self::Error>;
}
