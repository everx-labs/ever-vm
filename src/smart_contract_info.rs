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
    StackItem,
    integer::IntegerData,
};
use sha2::{Sha256, Digest};
use ton_block::{GlobalCapabilities, CurrencyCollection};
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmartContractInfo {
    pub actions: u16,
    pub msgs_sent: u16,
    pub unix_time: u32,
    pub block_lt: u64,
    pub trans_lt: u64,
    pub seq_no: u32,
    pub rand_seed: IntegerData,
    pub balance: CurrencyCollection,
    pub balance_remaining_grams: u128,
    pub balance_remaining_other: HashmapE,
    pub myself: SliceData,
    pub config_params: Option<Cell>, // config params from masterchain
    pub mycode: Cell,
    pub init_code_hash: UInt256,
    pub storage_fee_collected: u128,
    pub capabilities: u64,
}

impl SmartContractInfo{
    pub fn with_myself(myself: SliceData) -> Self {
        Self {
            myself,
            ..Self::default()
        }
    }

    // for compatibility with old 
    pub fn old_default(mycode: Cell) -> Self {
        Self {
            capabilities: GlobalCapabilities::CapInitCodeHash as u64 | GlobalCapabilities::CapMycode as u64,
            mycode,
            ..Default::default()
        }
    }

    #[deprecated]
    pub fn set_actions(&mut self, actions: u16) {
        self.actions = actions;
    }

    #[deprecated]
    pub fn set_msgs_sent(&mut self, msgs_sent: u16) {
        self.msgs_sent = msgs_sent;
    }

    #[deprecated]
    pub fn block_lt(&self) -> u64 {
        self.block_lt
    }

    #[deprecated]
    pub fn block_lt_mut(&mut self) -> &mut u64 {
        &mut self.block_lt
    }

    pub fn unix_time(&self) -> u32 {
        self.unix_time
    }

    #[deprecated]
    pub fn unix_time_mut(&mut self) -> &mut u32 {
        &mut self.unix_time
    }

    #[deprecated]
    pub fn trans_lt(&self) -> u64 {
        self.trans_lt
    }

    #[deprecated]
    pub fn trans_lt_mut(&mut self) -> &mut u64 {
        &mut self.trans_lt
    }

    #[deprecated]
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
    pub fn calc_rand_seed(&mut self, rand_seed_block: UInt256, account_address_anycast: &[u8]) {
        // combine all parameters to vec and calculate hash of them
        self.rand_seed = if !rand_seed_block.is_zero() {
            let mut hasher = Sha256::new();
            hasher.update(&rand_seed_block);
            hasher.update(&account_address_anycast);

            let sha256 = hasher.finalize();
            IntegerData::from_unsigned_bytes_be(&sha256)
        } else {
            // if the user forgot to set the rand_seed_block value, then this 0 will be clearly visible on tests
            log::warn!(target: "tvm", "Not set rand_seed_block");
            IntegerData::zero()
        };
    }

    #[deprecated]
    pub fn balance_remaining_grams_mut(&mut self) -> &mut u128 {
        &mut self.balance_remaining_grams
    }

    #[deprecated]
    pub fn balance_remaining_other_mut(&mut self) -> &mut HashmapE {
        &mut self.balance_remaining_other
    }

    pub fn set_init_code_hash(&mut self, init_code_hash: UInt256) {
        self.init_code_hash = init_code_hash;
    }

    pub fn set_storage_fee(&mut self, storage_fee: u128) {
        self.storage_fee_collected = storage_fee;
    }

    pub fn into_temp_data_item(self) -> StackItem {
        debug_assert_eq!(self.balance_remaining_grams, 0, "use balance instead old");
        debug_assert!(self.balance_remaining_other.data().is_none(), "use balance instead old");

        let balance = std::cmp::max(self.balance_remaining_grams, self.balance.grams.as_u128());
        let balance_other = self.balance_remaining_other.data().cloned()
            .or_else(|| self.balance.other_as_hashmap().data().cloned());

        let mut params = vec![
            int!(0x076ef1ea),      // magic - should be changed because of structure change
            int!(self.actions),    // actions
            int!(self.msgs_sent),  // msgs
            int!(self.unix_time),  // unix time
            int!(self.block_lt),   // logical time
            int!(self.trans_lt),   // transaction time
            StackItem::int(self.rand_seed),
            StackItem::tuple(vec![
                int!(balance),
                balance_other.map_or(StackItem::None, StackItem::Cell)
            ]),
            StackItem::Slice(self.myself),
            self.config_params.map_or(StackItem::None, StackItem::Cell),
        ];
        let mut additional_params = vec![
            (GlobalCapabilities::CapMycode, StackItem::cell(self.mycode.clone())),
            (GlobalCapabilities::CapInitCodeHash, StackItem::int(IntegerData::from_unsigned_bytes_be(self.init_code_hash.as_slice()))),
            (GlobalCapabilities::CapStorageFeeToTvm, StackItem::int(self.storage_fee_collected)),
            (GlobalCapabilities::CapDelections, StackItem::int(self.seq_no)),
        ];
        let add_params = &mut Vec::new();
        for (i, (caps, f)) in additional_params.drain(..).enumerate() {
            if (self.capabilities & caps as u64) != 0 {
                for _ in add_params.len()..i {
                    add_params.push(StackItem::default());
                }
                add_params.push(f);
            }
        }
        params.append(add_params);
        debug_assert!(params.len() <= 14, "{:?} caps: {:X}", params, self.capabilities);
        StackItem::tuple(vec![StackItem::tuple(params)])
    }

    #[deprecated]
    pub fn into_temp_data_with_init_code_hash(mut self, is_init_code_hash: bool, with_mycode: bool) -> StackItem {
        if is_init_code_hash { self.capabilities |= GlobalCapabilities::CapInitCodeHash as u64 }
        if with_mycode { self.capabilities |= GlobalCapabilities::CapMycode as u64 }
        self.into_temp_data_item()
    }

    #[deprecated]
    pub fn into_temp_data(mut self) -> StackItem {
        self.capabilities |= GlobalCapabilities::CapInitCodeHash as u64 | GlobalCapabilities::CapMycode as u64;
        self.into_temp_data_item()
    }

    #[deprecated]
    pub fn into_temp_data_with_capabilities(mut self, capabilities: u64) -> StackItem {
        self.capabilities = capabilities;
        self.into_temp_data_item()

    }
}

