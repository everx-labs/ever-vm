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

use crate::{error::TvmError, types::Exception};
use std::cmp::{max, min};
use ton_types::{error, Result, types::ExceptionCode};

// Gas state
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gas {
    gas_limit_max: i64,
    gas_limit: i64,
    gas_credit: i64,
    gas_remaining: i64,
    gas_price: i64,
    gas_base: i64,
}

const CELL_LOAD_GAS_PRICE: i64 = 100;
const CELL_RELOAD_GAS_PRICE: i64 = 25;
const CELL_CREATE_GAS_PRICE: i64 = 500;
const EXCEPTION_GAS_PRICE: i64 = 50;
const TUPLE_ENTRY_GAS_PRICE: i64 = 1;
const IMPLICIT_JMPREF_GAS_PRICE: i64 = 10;
const IMPLICIT_RET_GAS_PRICE: i64 = 5;
const FREE_STACK_DEPTH: usize = 32;
const STACK_ENTRY_GAS_PRICE: i64 = 1;
#[cfg(feature = "gosh")]
const DIFF_DURATION_FOR_LINE: i64 = 60;
#[cfg(feature = "gosh")]
const DIFF_DURATION_FOR_COUNT_PATCHES: usize = 80;
#[cfg(feature = "gosh")]
const DIFF_PATCH_DURATION_FOR_LINE: i64 = 40;
#[cfg(feature = "gosh")]
const DIFF_PATCH_DURATION_FOR_BYTE: i64 = 1;
#[cfg(feature = "gosh")]
const DIFF_PATCH_DURATION_FOR_COUNT_PATCHES: i64 = 1200;
#[cfg(feature = "gosh")]
const DURATION_TO_GAS_COEFFICIENT: i64 = 30;
#[cfg(feature = "gosh")]
const ZIP_DURATION_FOR_BYTE: i64 = 4;
#[cfg(feature = "gosh")]
const UNZIP_DURATION_FOR_BYTE: i64 = 1;
// const MAX_DATA_DEPTH: usize = 512;

impl Gas {
    /// Instance for constructors. Empty fields
    pub const fn empty() -> Gas {
        Gas {
            gas_limit_max: 0,
            gas_limit: 0,
            gas_credit: 0,
            gas_remaining: 0,
            gas_price: 0,
            gas_base: 0,
        }
    }
    /// Instance for debug and test. Cheat fields
    pub const fn test() -> Gas {
        Gas {
            gas_price: 10,
            gas_limit: 1000000000,
            gas_limit_max: 1000000000,
            gas_remaining: 1000000000,
            gas_credit: 0,
            gas_base: 1000000000,
        }
    }
    /// Instance for release
    pub fn test_with_limit(gas_limit: i64) -> Gas {
        let mut gas = Gas::test();
        gas.new_gas_limit(gas_limit);
        gas
    }
    /// Instance for release
    pub fn test_with_credit(gas_credit: i64) -> Gas {
        Gas::new(0, gas_credit, 1000000000, 10)
    }
    /// Instance for release
    pub const fn new(gas_limit: i64, gas_credit: i64, gas_limit_max: i64, gas_price: i64) -> Gas {
        let remaining = gas_limit + gas_credit;
        Gas {
            gas_price,
            gas_limit,
            gas_limit_max,
            gas_remaining: remaining,
            gas_credit,
            gas_base: remaining,
        }
    }
    /// Compute instruction cost
    pub const fn basic_gas_price(instruction_length: usize, _instruction_references_count: usize) -> i64 {
        // old formula from spec: (10 + instruction_length + 5 * instruction_references_count) as i64
        (10 + instruction_length) as i64
    }
    pub fn consume_basic(&mut self, instruction_length: usize, _instruction_references_count: usize) -> i64 {
        // old formula from spec: (10 + instruction_length + 5 * instruction_references_count) as i64
        self.use_gas((10 + instruction_length) as i64)
    }

    /// Compute exception cost
    pub const fn exception_price() -> i64 {
        EXCEPTION_GAS_PRICE
    }
    pub fn consume_exception(&mut self) -> i64 {
        self.use_gas(EXCEPTION_GAS_PRICE)
    }

    /// Compute exception cost
    pub const fn finalize_price() -> i64 {
        CELL_CREATE_GAS_PRICE
    }
    pub fn consume_finalize(&mut self) -> i64 {
        self.use_gas(CELL_CREATE_GAS_PRICE)
    }

    /// Implicit JMP cost
    pub const fn implicit_jmp_price() -> i64 {
        IMPLICIT_JMPREF_GAS_PRICE
    }
    pub fn consume_implicit_jmp(&mut self) -> i64 {
        self.use_gas(IMPLICIT_JMPREF_GAS_PRICE)
    }

    /// Implicit RET cost
    pub const fn implicit_ret_price() -> i64 {
        IMPLICIT_RET_GAS_PRICE
    }
    pub fn consume_implicit_ret(&mut self) -> i64 {
        self.use_gas(IMPLICIT_RET_GAS_PRICE)
    }

    /// Compute exception cost
    pub const fn load_cell_price(first: bool) -> i64 {
        if first {CELL_LOAD_GAS_PRICE} else {CELL_RELOAD_GAS_PRICE}
    }
    pub fn consume_load_cell(&mut self, first: bool) -> i64 {
        self.use_gas(if first {CELL_LOAD_GAS_PRICE} else {CELL_RELOAD_GAS_PRICE})
    }

    /// Stack cost
    pub const fn stack_price(stack_depth: usize) -> i64 {
        let depth = if stack_depth > FREE_STACK_DEPTH {
            stack_depth
        } else {
            FREE_STACK_DEPTH
        };
        STACK_ENTRY_GAS_PRICE * (depth - FREE_STACK_DEPTH) as i64
    }
    pub fn consume_stack(&mut self, stack_depth: usize) -> i64 {
        self.use_gas(
            STACK_ENTRY_GAS_PRICE * (max(stack_depth, FREE_STACK_DEPTH) - FREE_STACK_DEPTH) as i64
        )
    }

    /// Compute tuple usage cost
    pub const fn tuple_gas_price(tuple_length: usize) -> i64 {
        TUPLE_ENTRY_GAS_PRICE * tuple_length as i64
    }
    pub fn consume_tuple_gas(&mut self, tuple_length: usize) -> i64 {
        self.use_gas(TUPLE_ENTRY_GAS_PRICE * tuple_length as i64)
    }

    #[cfg(feature = "gosh")]
    /// line cost for diff
    pub fn diff_fee_for_line(lines_first_file: usize, lines_second_file: usize) -> i64 {
        let lines = std::cmp::max(lines_first_file, lines_second_file) as i64;
        let duration = DIFF_DURATION_FOR_LINE * lines;
        (duration * (duration as f64).log2() as i64) / DURATION_TO_GAS_COEFFICIENT
    }

    #[cfg(feature = "gosh")]
    /// patch cost for diff
    pub fn diff_fee_for_count_patches(count: usize) -> i64 {
        (
            (count * count * DIFF_DURATION_FOR_COUNT_PATCHES * DIFF_DURATION_FOR_COUNT_PATCHES) / 
            (DURATION_TO_GAS_COEFFICIENT as usize)
        ) as i64
    }

    #[cfg(feature = "gosh")]
    /// line cost for diff_patch
    pub fn diff_patch_fee_for_line(lines: i64) -> i64 {
        (DIFF_PATCH_DURATION_FOR_LINE * lines) / DURATION_TO_GAS_COEFFICIENT
    }

    #[cfg(feature = "gosh")]
    /// byte cost for diff_bytes_patch
    pub fn diff_bytes_patch_fee_for_byte(bytes: i64) -> i64 {
        (DIFF_PATCH_DURATION_FOR_BYTE * bytes) / DURATION_TO_GAS_COEFFICIENT
    }

    #[cfg(feature = "gosh")]
    /// patch cost for diff_patch
    pub fn diff_patch_fee_for_count_patches(count: i64) -> i64 {
        (DIFF_PATCH_DURATION_FOR_COUNT_PATCHES * count) / DURATION_TO_GAS_COEFFICIENT
    }

    #[cfg(feature = "gosh")]
    /// byte cost for zip
    pub fn zip_fee_for_byte(bytes: i64) -> i64 {
        (ZIP_DURATION_FOR_BYTE * bytes) / DURATION_TO_GAS_COEFFICIENT
    }

    #[cfg(feature = "gosh")]
    /// byte cost for unzip
    pub fn unzip_fee_for_byte(bytes: i64) -> i64 {
        (UNZIP_DURATION_FOR_BYTE * bytes) / DURATION_TO_GAS_COEFFICIENT
    }

    /// Set input gas to gas limit
    pub fn new_gas_limit(&mut self, gas_limit: i64) {
        self.gas_limit = max(0, min(gas_limit, self.gas_limit_max));
        self.gas_credit = 0;
        self.gas_remaining += self.gas_limit - self.gas_base;
        self.gas_base = self.gas_limit;
    }

    /// Update remaining gas limit
    pub fn use_gas(&mut self, gas: i64) -> i64 {
        self.gas_remaining -= gas;
        self.gas_remaining
    }

    /// Try to consume gas then raise exception out of gas if needed
    pub fn try_use_gas(&mut self, gas: i64) -> Result<Option<i32>> {
        self.gas_remaining -= gas;
        self.check_gas_remaining()
    }

    /// Raise out of gas exception
    pub fn check_gas_remaining(&self) -> Result<Option<i32>> {
        if self.gas_remaining >= 0 {
            Ok(None)
        } else {
            Err(exception!(ExceptionCode::OutOfGas, self.gas_base - self.gas_remaining, "check_gas_remaining"))
        }
    }

    // *** Getters ***
    pub const fn get_gas_price(&self) -> i64 {
        self.gas_price
    }

    pub const fn get_gas_limit(&self) -> i64 {
        self.gas_limit
    }

    pub const fn get_gas_limit_max(&self) -> i64 {
        self.gas_limit_max
    }

    pub const fn get_gas_remaining(&self) -> i64 {
        self.gas_remaining
    }

    pub const fn get_gas_credit(&self) -> i64 {
        self.gas_credit
    }

    pub const fn get_gas_used_full(&self) -> i64 {
        self.gas_base - self.gas_remaining
    }

    pub const fn get_gas_used(&self) -> i64 {
        if self.gas_remaining > 0 {
            self.gas_base - self.gas_remaining
        } else {
            self.gas_base
        }
    }
}
