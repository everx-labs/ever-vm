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
    StackItem,
    integer::IntegerData,
};
use sha2::{Sha256, Digest};
use std::sync::Arc;
use ton_types::{Cell, HashmapE, HashmapType, SliceData, types::UInt256};


/*
The smart-contract information
structure SmartContractInfo, passed in the first reference of the cell contained
in control register c5, is serialized as follows:

smc_info#076ef1ea actions:uint16 msgs_sent:uint16
unixtime:uint32 block_lt:uint64 trans_lt:uint64
rand_seed:uint256 balance_remaining:CurrencyCollection
myself:MsgAddress = SmartContractInfo;
*/
#[derive(Clone,Debug,PartialEq)]
pub struct SmartContractInfo{
    actions: u16,
    msgs_sent: u16,
    unix_time: u32,
    block_lt: u64,
    trans_lt: u64,
    rand_seed: IntegerData,
    balance_remaining_grams: u128,
    balance_remaining_other: HashmapE,
    myself: SliceData,
    config_params: Option<Cell>, // config params from masterchain
    mycode: Cell,
    init_code_hash: UInt256,
}

impl SmartContractInfo{

    pub fn default() -> Self {
        SmartContractInfo {
            actions: 0,
            msgs_sent: 0,
            unix_time: 0,
            block_lt: 0,
            trans_lt: 0,
            rand_seed: IntegerData::zero(),
            balance_remaining_grams: 0,
            balance_remaining_other: HashmapE::with_bit_len(32),
            myself: SliceData::default(),
            config_params: None,
            mycode: Cell::default(),
            init_code_hash: UInt256::default(),
        }
    }

    pub fn with_myself(address: SliceData) -> Self {
        Self {
            myself: address,
            ..Self::default()
        }
    }

    pub fn set_actions(&mut self, actions: u16) {
        self.actions = actions;
    }

    pub fn set_msgs_sent(&mut self, msgs_sent: u16) {
        self.msgs_sent = msgs_sent;
    }

    pub fn block_lt(&self) -> u64 {
        self.block_lt
    }

    pub fn block_lt_mut(&mut self) -> &mut u64 {
        &mut self.block_lt
    }

    pub fn unix_time(&self) -> u32 {
        self.unix_time
    }

    pub fn unix_time_mut(&mut self) -> &mut u32 {
        &mut self.unix_time
    }

    pub fn trans_lt(&self) -> u64 {
        self.trans_lt
    }

    pub fn trans_lt_mut(&mut self) -> &mut u64 {
        &mut self.trans_lt
    }

    pub fn set_config_params(&mut self, params: Cell) {
        self.config_params = Some(params);
    }

    pub fn set_mycode(&mut self, code: Cell) {
        self.mycode = code;
    }
    /*
            The rand_seed field here is initialized deterministically starting from the
        rand_seed of the block, and the account address.
    */
    pub fn calc_rand_seed(&mut self, rand_seed_block: UInt256, account_address_anycast: &Vec<u8>) {
        // combine all parameters to vec and calculate hash of them
        if !rand_seed_block.is_zero() {
            let mut hasher = Sha256::new();
            hasher.input(rand_seed_block.as_slice());
            hasher.input(&account_address_anycast);

            let sha256 = hasher.result();
            self.rand_seed = IntegerData::from_unsigned_bytes_be(&sha256);
        } else {
            // if the user forgot to set the rand_seed_block value, then this 0 will be clearly visible on tests
            log::warn!(target: "tvm", "Not set rand_seed_block");
            self.rand_seed = 0.into();
        }
    }

    pub fn balance_remaining_grams(&self) -> &u128 {
        &self.balance_remaining_grams
    }

    pub fn balance_remaining_grams_mut(&mut self) -> &mut u128 {
        &mut self.balance_remaining_grams
    }

    pub fn balance_remaining_other(&self) -> &HashmapE {
        &self.balance_remaining_other
    }

    pub fn balance_remaining_other_mut(&mut self) -> &mut HashmapE {
        &mut self.balance_remaining_other
    }

    pub fn myself_mut(&mut self) -> &mut SliceData {
        &mut self.myself
    }

    pub fn set_init_code_hash(&mut self, init_code_hash: UInt256) {
        self.init_code_hash = init_code_hash;
    }

    pub fn into_temp_data(&self) -> StackItem {
        let mut params = vec![
            int!(0x076ef1ea),      // magic - should be changed because of structure change
            int!(self.actions),    // actions
            int!(self.msgs_sent),  // msgs
            int!(self.unix_time),  // unix time
            int!(self.block_lt),   // logical time
            int!(self.trans_lt),   // transaction time
            StackItem::int(self.rand_seed.clone()),
            StackItem::tuple(vec![
                int!(self.balance_remaining_grams),
                self.balance_remaining_other.data()
                .map(|dict| StackItem::Cell(dict.clone()))
                .unwrap_or_else(|| StackItem::default())
                ]),
            StackItem::Slice(self.myself.clone()),
            self.config_params.as_ref()
                .map(|params| StackItem::Cell(params.clone()))
                .unwrap_or_else(|| StackItem::default()),
        ];
        params.push(StackItem::cell(self.mycode.clone()));
        params.push(StackItem::int(IntegerData::from_unsigned_bytes_be(self.init_code_hash.as_slice())));
        StackItem::tuple(vec![StackItem::tuple(params)])
    }
}
