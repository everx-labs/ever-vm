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

#![cfg_attr(feature = "ci_run", deny(warnings))]
#![recursion_limit="128"] // needs for error_chain

// External
extern crate core;
extern crate crc;
extern crate ed25519_dalek;
#[macro_use]
extern crate log;
extern crate num;
extern crate num_traits;
extern crate sha2;

#[macro_use]
extern crate error_chain;
extern crate rand;

extern crate ton_types;

#[macro_use]
pub mod types;
#[macro_use]
pub mod stack;
#[macro_use]
pub mod executor;

pub mod assembler;
pub mod smart_contract_info;
pub use self::smart_contract_info::SmartContractInfo;
pub mod error;
