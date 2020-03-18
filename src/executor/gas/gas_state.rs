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

use std::cmp::{max, min};
use types::{ExceptionCode, Result, TvmError};

// TODO: it seems everything should be unsigned
// Application-specific primitives - A.10; Gas-related primitives - A.10.2
// Specification limit value - pow(2,63)-1
pub static SPEC_LIMIT: i64 = 9223372036854775807;

pub trait GasCallback : std::fmt::Debug {
    //Get gas price from masterchain in nanograms  
    fn get_gas_price(&self) -> i64;
    //Get max gas limit from masterchain
    fn get_gas_limit_max(&self) -> i64;
    //Get max gas credit from masterchain
    fn gas_credit(&self) -> i64;
}

//For getting configuration params from masterchain
#[derive(Debug)]
pub struct TestCallback {
    gas_price: i64, 
    gas_limit_max: i64, 
    gas_credit: i64,
}

impl TestCallback {
    pub fn new(gas_price: i64, 
        gas_limit_max: i64, 
        gas_credit: i64) -> TestCallback {
            
            TestCallback {
                gas_price,
                gas_limit_max,
                gas_credit,
            }
        }
}

impl GasCallback for TestCallback {
    fn get_gas_price(&self) -> i64 {
        self.gas_price.clone()
    }
    
    fn get_gas_limit_max(&self) -> i64 {
        self.gas_limit_max.clone()
    }
  
    fn gas_credit(&self) -> i64 {
        self.gas_credit.clone()
    }
}

// Gas state
#[derive(Clone, Debug, PartialEq)]
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
// const MAX_DATA_DEPTH: usize = 512;

impl Gas {
    /// Instanse for constructors. Empty fields
    pub fn empty() -> Gas {
        Gas {
            gas_limit_max: 0,
            gas_limit: 0,
            gas_credit: 0,
            gas_remaining: 0,
            gas_price: 0,
            gas_base: 0,
        }
    }    
    /// Instanse for debug and test. Cheat fields
    pub fn test() -> Gas {
        Gas {
            gas_price: 10,
            gas_limit: 1000000000,
            gas_limit_max: 1000000000,
            gas_remaining: 1000000000,
            gas_credit: 0,
            gas_base: 1000000000,
        }
    }
    /// Instanse for release.
    pub fn test_with_limit(gas_limit: i64) -> Gas {
        let mut gas = Gas::test();
        gas.gas_limit = gas_limit;
        gas
    }
    /// Instanse for release.
    pub fn new(gas_limit: i64, gas_credit: i64, gas_limit_max: i64, gas_price: i64) -> Gas {
        let remaining = gas_limit + gas_credit;
        Gas {
            gas_price: gas_price,
            gas_limit: gas_limit,
            gas_limit_max: gas_limit_max,
            gas_remaining: remaining,
            gas_credit: gas_credit,
            gas_base: remaining,
        }
    }
    /// Compute instruction cost
    pub fn basic_gas_price(instruction_length: usize, _instruction_references_count: usize) -> i64 {
        // old formula from spec: (10 + instruction_length + 5 * instruction_references_count) as i64
        (10 + instruction_length) as i64
    }

    /// Compute exception cost
    pub fn exception_price(_code: ExceptionCode) -> i64 {
        EXCEPTION_GAS_PRICE
    }

    /// Compute exception cost
    pub fn finalize_price() -> i64 {
        CELL_CREATE_GAS_PRICE
    }

    /// Implicit JMP cost
    pub fn implicit_jmp_price() -> i64 {
        IMPLICIT_JMPREF_GAS_PRICE
    }

    /// Implicit RET cost
    pub fn implicit_ret_price() -> i64 {
        IMPLICIT_RET_GAS_PRICE
    }

    /// Compute exception cost
    pub fn load_cell_price(first: bool) -> i64 {
        if first {CELL_LOAD_GAS_PRICE} else {CELL_RELOAD_GAS_PRICE}
    }

    /// Stack cost
    pub fn stack_price(stack_depth: usize) -> i64 {
        STACK_ENTRY_GAS_PRICE * (max(stack_depth, FREE_STACK_DEPTH) - FREE_STACK_DEPTH) as i64
    }

    /// Compute tuple using cost
    pub fn tuple_gas_price(tuple_length: usize) -> i64 {
        TUPLE_ENTRY_GAS_PRICE * tuple_length as i64
    }

    /// Set input gas to gas limit
    pub fn new_gas_limit(&mut self, gas_limit: i64) {
        self.gas_limit = max(0, min(gas_limit, self.gas_limit_max));
        self.gas_credit = 0;
        self.gas_remaining += self.gas_limit - self.gas_base;
        self.gas_base = self.gas_limit;
    }
    /// Update remaining gas limit.
    pub fn use_gas(&mut self, gas: i64) -> i64 {
        self.gas_remaining -= gas;
        self.gas_remaining
    }
    pub fn try_use_gas(&mut self, gas: i64) -> Result<i64> {
        if self.gas_remaining >= gas {
            self.gas_remaining -= gas;
            Ok(self.gas_remaining)
        } else {
            err!(ExceptionCode::OutOfGas)
        }
    }
    // *** Getters ***
    pub fn get_gas_price(&self) -> i64 {
        self.gas_price
    }
    
    pub fn get_gas_limit(&self) -> i64 {
        self.gas_limit
    }
    
    pub fn get_gas_limit_max(&self) -> i64 {
        self.gas_limit_max
    }
    
    pub fn get_gas_remaining(&self) -> i64 {
        self.gas_remaining
    }
    
    pub fn get_gas_credit(&self) -> i64 {
        self.gas_credit
    }
    
    pub fn get_gas_used(&self) -> i64 {
        self.gas_base - self.gas_remaining
    }
}
