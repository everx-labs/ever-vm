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

use crate::stack::{
    StackItem, 
    integer::{IntegerData, serialization::{Encoding, UnsignedIntegerBigEndianEncoding}},
    serialization::Deserializer
};
use sha2::Digest;
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
    config_params: Option<Cell> // config params from masterchain
}

impl Default for SmartContractInfo {
    fn default() -> Self{
        SmartContractInfo{
            actions: 0,
            msgs_sent: 0,
            unix_time: 0,
            block_lt: 0,
            trans_lt: 0,
            rand_seed: IntegerData::zero(),
            balance_remaining_grams: 0,
            balance_remaining_other: HashmapE::with_bit_len(32),
            myself: SliceData::new(vec!(0x20)),
            config_params: None
        }
    }
}

impl SmartContractInfo{

    pub fn with_myself(address: SliceData) -> Self{
        let mut sci = SmartContractInfo::default();
        sci.myself = address;
        sci
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
        self.config_params = Some(params)
    }
    /*
            The rand_seed field here is initialized deterministically starting from the
        rand_seed of the block, the account address, the hash of the inbound message
        being processed (if any), and the transaction logical time trans_lt.
    */
    pub fn calc_rand_seed(&mut self, rand_seed_block: UInt256, in_msg_hash: UInt256) {
        // combine all parameters to vec and calculate hash of them
        let v = self.trans_lt.to_be_bytes();
        let mut hasher = sha2::Sha256::new();
        hasher.input(rand_seed_block.as_slice());
        hasher.input(self.myself.cell().repr_hash().as_slice());
        hasher.input(in_msg_hash.as_slice());
        hasher.input(&v);

        let sha256 = hasher.result();
        self.rand_seed = UnsignedIntegerBigEndianEncoding::new(256)
            .deserialize(&sha256);
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

    pub fn into_temp_data(&self) -> StackItem {
        StackItem::Tuple(vec![
            StackItem::Tuple(vec![
                int!(0x076ef1ea),      // magic
                int!(self.actions),    // actions
                int!(self.msgs_sent),  // msgs
                int!(self.unix_time),  // unix time
                int!(self.block_lt),   // logical time
                int!(self.trans_lt),   // transaction time
                StackItem::Integer(Arc::new(self.rand_seed.clone())),
                StackItem::Tuple(vec![
                    int!(self.balance_remaining_grams),
                    self.balance_remaining_other.data()
                    .map(|dict| StackItem::Cell(dict.clone()))
                    .unwrap_or_default()
                    ]),
                StackItem::Slice(self.myself.clone()),
                self.config_params.as_ref()
                    .map(|params| StackItem::Cell(params.clone()))
                    .unwrap_or_default()
            ])
        ])
    }
}
