/*
* Copyright (C) 2019-2022 TON Labs. All Rights Reserved.
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

use crate::{
    error::TvmError,
    executor::engine::Engine,
    stack::StackItem,
    types::{Exception, Status},
};
use num::{BigUint, ToPrimitive};
use std::{
    cmp::{min, Ordering},
    collections::HashMap,
    sync::Arc,
};
use ton_block::{
    Account, ConfigParam1, ConfigParam15, ConfigParam16, ConfigParam17, ConfigParam34,
    DelectorParams, Deserializable, GlobalCapabilities, Grams, MsgAddress, MsgAddressInt,
    Serializable, ShardAccount, SigPubKey, ValidatorDescr, ValidatorSet,
};
use ton_types::{
    error, fail, BuilderData, Cell, ExceptionCode, GasConsumer, HashmapE, IBitstring, Result,
    SliceData, UInt256,
};

use super::{
    engine::{storage::fetch_stack, IndexProvider},
    types::Instruction,
};

#[derive(Debug, Default)]
struct Staker {
    stake: u128,
}

#[derive(Debug, Default, Ord, Eq, PartialOrd, PartialEq)]
struct ValidatorKey {
    stake: Grams,
    time: u32,
    pub_key: UInt256,
}

#[derive(Debug, Default, Eq)]
struct Validator {
    key: ValidatorKey,
    true_stake: u128, // min(min_stake * max_factor, stake)
    max_factor: u32,  // fixed point real 16 bit
    addr: SimpleAddress,
    adnl_addr: UInt256,
    mc_seq_no_since: u32,
}

impl PartialEq for Validator {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Ord for Validator {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key.cmp(&self.key)
    }
}

impl PartialOrd for Validator {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
struct StakeAndFactor<'a> {
    stake: BigUint,
    validator: &'a Validator,
}

struct SortedList<'a> {
    list: Vec<StakeAndFactor<'a>>,
}

impl<'a> SortedList<'a> {
    const fn new() -> SortedList<'a> {
        SortedList { list: Vec::new() }
    }

    fn insert(&mut self, element: StakeAndFactor<'a>) {
        let index = self.list.partition_point(|a| element.stake >= a.stake);
        self.list.insert(index, element);
    }

    fn pop(&mut self) -> Option<StakeAndFactor<'a>> {
        self.list.pop()
    }

    fn last(&self) -> Option<&StakeAndFactor<'a>> {
        self.list.last()
    }
}

pub(crate) fn execute_try_elect(engine: &mut Engine) -> Status {
    engine.load_instruction(Instruction::new("TRYELECT"))?;
    engine.check_capability(GlobalCapabilities::CapDelections)?;
    fetch_stack(engine, 1)?;
    let credits = engine.cmd.var(0).as_dict()?.cloned();
    let cfg1: ConfigParam1 = engine.read_config_param(1)?;
    let slice = &mut engine.smci_param(8)?.as_slice()?.clone();
    let my_addr = MsgAddressInt::construct_from(slice)?;
    if !my_addr.is_masterchain() || cfg1.elector_addr != my_addr.address() {
        return err!(
            ExceptionCode::TypeCheckError,
            "it can be called only in elector contract {}",
            my_addr
        );
    }

    let cfg15: ConfigParam15 = engine.read_config_param(15)?;
    let cfg16: ConfigParam16 = engine.read_config_param(16)?;
    let cfg17: ConfigParam17 = engine.read_config_param(17)?;

    // TODO: we cannot determine total stake before procedure
    // if cur_elect.total_stake < min_total_stake {
    //     // insufficient total stake, postpone elections
    //     return postpone_elections();
    // }

    // all validators sorted in decresing stake order
    let validators = find_validators(engine, &cfg17)?;
    let result = calculate_elections(credits, validators, &cfg16, &cfg17, engine)?;
    let utime_since = engine.smci_param(3)?.as_integer()?.into(0..=u32::MAX)?;
    let utime_until = utime_since + cfg15.validators_elected_for;
    let main = cfg16.max_main_validators.as_u16();
    let vset = ValidatorSet::with_values_version_2(
        utime_since,
        utime_until,
        main,
        result.total_weight,
        result.list,
    )?;
    let cell = vset.serialize()?;
    engine.cc.stack.push(StackItem::cell(cell));
    engine.cc.stack.push(StackItem::dict(&result.frozen));
    engine.cc.stack.push(StackItem::dict(&result.credits));
    engine.cc.stack.push(StackItem::int(result.total_stake));
    engine.cc.stack.push(StackItem::int(result.total_weight));
    Ok(())
}

#[derive(Debug, Default, Eq, PartialEq)]
struct SimpleAddress {
    workchain_id: i32,
    address: UInt256,
}

impl SimpleAddress {
    fn with_params(workchain_id: i32, address: UInt256) -> Self {
        Self {
            workchain_id,
            address,
        }
    }
}

impl Serializable for SimpleAddress {
    fn write_to(&self, cell: &mut BuilderData) -> Result<()> {
        self.workchain_id.write_to(cell)?;
        self.address.write_to(cell)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ElectionResult {
    list: Vec<ValidatorDescr>,
    total_weight: u64,
    total_stake: u128,
    frozen: HashmapE, // pub_key: addr, weight, true_stake, banned = false
    credits: HashmapE,
}

impl ElectionResult {
    fn with_credits(credits: Option<Cell>) -> ElectionResult {
        ElectionResult {
            list: Vec::new(),
            total_weight: 0,
            total_stake: 0,
            frozen: HashmapE::with_bit_len(256), // pub_key => (workchain_id + address), weight u64, stake u128, banned bool
            credits: HashmapE::with_hashmap(32 + 256, credits), // (workchain_id + address) => (stake: u128)
        }
    }
}

fn calculate_elections(
    credits: Option<Cell>,
    mut validators: Vec<Validator>,
    cfg16: &ConfigParam16,
    cfg17: &ConfigParam17,
    gas_consumer: &mut dyn GasConsumer,
) -> Result<ElectionResult> {
    if validators.len() < cfg16.min_validators.as_usize() {
        return err!(
            "not enough good validators: {} < {}",
            validators.len(),
            cfg16.min_validators.as_u32()
        );
    }
    let n = min(cfg16.max_validators.as_usize(), validators.len());

    let mut result = ElectionResult::with_credits(credits);
    // TODO: first check overrun
    // > 3e38
    // u128::MAX == 340282366920938463463374607431768211455
    // 1e16 * 1e3 * 3 = 3e19
    // cfg17.max_stake * n * cfg17.max_stake_factor

    // TODO: no need to sort all, only first n
    validators.sort();

    let mut m_stake = 0; // minimal stake
    let mut whole_stake = Grams::zero();
    let mut best_stake = Grams::zero();

    const PRECISION: u128 = 1_000_000_000_000_000_000u128; // 1e18
    let mut wholes_stakes = SortedList::new();
    let mut cut_fact_sum = 0;
    let mut m = 0; // qty of usable stakes
    for (qty, validator) in validators.iter_mut().take(n).enumerate() {
        whole_stake += validator.key.stake;
        // it can overrun u128 so use BigUint
        let stake_big = BigUint::from(validator.key.stake.as_u128()) * PRECISION;
        let stake = (stake_big.clone() << 16u8) / validator.max_factor;
        // println!("{} stake {} time {} big stake: {}", qty, validator.key.stake, validator.time, stake);
        wholes_stakes.insert(StakeAndFactor { stake, validator });
        while let Some(e) = wholes_stakes.last() {
            if e.stake < stake_big {
                break;
            }
            whole_stake -= e.validator.key.stake;
            cut_fact_sum += e.validator.max_factor as u128;
            // println!("e: {}", e.stake);
            wholes_stakes.pop();
        }

        let stake_cut = (validator.key.stake * cut_fact_sum) >> 16;
        let total_stake = whole_stake + stake_cut;
        if best_stake < total_stake {
            best_stake = total_stake;
            m = qty + 1;
            m_stake = validator.key.stake.as_u128();
            // } else { println!("{}: total stake {} is not grown {}", qty, total_stake, best_stake);
        }
    }

    // println!("n: {} m: {} best_stake: {} max_stake: {}", n, m, best_stake, cfg17.max_stake);

    if (m == 0) || (best_stake < cfg17.min_total_stake) {
        return err!(
            "stake is not enough n: {} m: {} best_stake: {}",
            n,
            m,
            best_stake
        );
    }
    // we have to select first m validators from list l

    // precise calculation of best stake
    let round_best_stake = best_stake.as_u128();
    let mut best_stake = 0;
    for validator in validators.iter_mut().take(m) {
        validator.true_stake = min(
            validator.key.stake.as_u128(),
            (m_stake * validator.max_factor as u128) >> 16,
        );
        best_stake += validator.true_stake;
    }
    let abs = match round_best_stake.cmp(&best_stake) {
        Ordering::Equal => 0,
        Ordering::Less => best_stake - round_best_stake,
        Ordering::Greater => round_best_stake - best_stake,
    };
    // use it with rust 1.6
    // let abs = round_best_stake.abs_diff(best_stake);
    // println!("{} - {} abs score: {}", round_best_stake, best_stake, abs);
    if abs > 1_000_000_000 {
        return err!(
            "{} and {} differ more than by 1_000_000_000",
            best_stake,
            round_best_stake
        );
    }
    // create both the new validator set and the refund set
    for (i, validator) in validators.drain(..).enumerate() {
        let mut leftover_stake = validator.key.stake.as_u128() - validator.true_stake;
        if leftover_stake > 0 {
            // non-zero unused part of the stake, credit to the source address
            let key = SliceData::load_cell(validator.addr.serialize()?)?;
            if let Some(mut data) = result.credits.get_with_gas(key.clone(), gas_consumer)? {
                leftover_stake += data.get_next_u128()?
            }
            let leftover_stake = leftover_stake.write_to_new_cell()?;
            result
                .credits
                .set_builder_with_gas(key, &leftover_stake, gas_consumer)?;
        }
        if i < m {
            result.total_stake += validator.true_stake;
            let weight = (BigUint::from(validator.true_stake) << 60u8) / best_stake;
            let weight = match weight.to_u64() {
                Some(weight) => weight,
                None => return err!("weight {} does not fit u64", weight),
            };
            result.total_weight += weight;
            result.list.push(ValidatorDescr::with_params(
                SigPubKey::from_bytes(validator.key.pub_key.as_slice())?,
                weight,
                Some(validator.adnl_addr),
                None,
            ));
            let key = SliceData::from_raw(validator.key.pub_key.as_array().to_vec(), 256);
            let mut value = validator.addr.write_to_new_cell()?;
            value.append_u64(weight)?;
            value.append_u128(validator.true_stake)?;
            value.append_u8(0)?;
            result
                .frozen
                .set_builder_with_gas(key, &value, gas_consumer)?;
        }
    }
    // m_credits = credits;
    if result.total_stake != best_stake {
        return err!("{} != {}", result.total_stake, best_stake);
    }
    Ok(result)
}

type Stakers = HashMap<MsgAddressInt, Vec<Staker>>;

/// get common depool's parameters from is data (pub_key and stake)
fn process_depool(
    account: &Account,
    data: &mut SliceData,
    gas_consumer: &mut dyn GasConsumer,
) -> Result<(UInt256, u128)> {
    let pub_key = data.get_next_hash()?;
    *data = gas_consumer.load_cell(data.checked_drain_reference()?)?;
    data.move_by(256 + 16)?;
    MsgAddress::skip(data)?;
    let stake = data.get_next_u64()? as u128;
    let state = data.get_next_byte()?;
    if state > 2 {
        custom_err!(73, "state of depool is wrong: {}", state);
    }
    if state != 1 {
        return err!("state of depool is not active {}", state);
    }
    let balance = match account.balance() {
        Some(balance) => balance.grams.as_u128(),
        None => fail!("account has no balance"),
    };
    if stake > balance {
        return err!("account balance {} less than its stake {}", balance, stake);
    }
    *data = gas_consumer.load_cell(data.checked_drain_reference()?)?;
    data.move_by(64 + 32)?;
    data.get_next_dictionary()?;
    data.move_by(32)?;
    data.get_next_dictionary()?;
    data.get_next_dictionary()?;
    data.get_next_dictionary()?;
    Ok((pub_key, stake))
}

/// get validator's parameters from depool data
fn process_validator(
    shard_acc: &ShardAccount,
    stakers: &mut Stakers,
    min_stake: &u128,
    max_stake: &u128,
    gas_consumer: &mut dyn GasConsumer,
) -> Result<Validator> {
    let account = shard_acc.read_account()?;
    let mut data = match account.get_data() {
        Some(data) => gas_consumer.load_cell(data)?,
        None => fail!("account has no data"),
    };
    let (pub_key, mut stake) = process_depool(&account, &mut data, gas_consumer)?;
    let max_factor = data.get_next_u32()?;
    let adnl_addr = data.get_next_hash()?;
    // time now is not implemented in validator contract but left for testing purposes now
    let time = !data.get_next_u32().unwrap_or(0);
    let addr = match account.get_addr() {
        Some(addr) => {
            if let Some(stakers) = stakers.remove(addr) {
                stakers.iter().for_each(|staker| stake += staker.stake);
            }
            SimpleAddress::with_params(addr.workchain_id(), addr.address().get_next_hash()?)
        }
        None => fail!("wrong address of validator"),
    };
    if &stake > max_stake {
        stake = *max_stake;
    }
    if &stake < min_stake {
        return err!("stake {} is less than min_stake {}", stake, min_stake);
    }
    let key = ValidatorKey {
        stake: Grams::new(stake)?,
        time,
        pub_key,
    };
    Ok(Validator {
        key,
        true_stake: 0,
        max_factor,
        addr,
        adnl_addr,
        mc_seq_no_since: 0,
    })
}

/// determine if staker is valid and return its address and parameters
fn process_staker(shard_acc: &ShardAccount, gas_consumer: &mut dyn GasConsumer) -> Result<(MsgAddressInt, Staker)> {
    let account = shard_acc.read_account()?;
    let mut data = match account.get_data() {
        Some(data) => SliceData::load_cell(data)?,
        None => fail!("account has no data"),
    };
    let (_, stake) = process_depool(&account, &mut data, gas_consumer)?;
    match MsgAddress::construct_maybe_from(&mut data)?.and_then(|addr| addr.to_msg_addr_int()) {
        Some(addr) => {
            let staker = Staker { stake };
            Ok((addr, staker))
        }
        None => err!(
            "m_validatorDePool of staker {:?} has wrong address type",
            account.get_addr()
        ),
    }
}

/// get all valid stakers and put them to hashmap
fn process_stakers(stakers: Vec<ShardAccount>, gas_consumer: &mut dyn GasConsumer) -> Stakers {
    let mut contracts = Stakers::new();
    for shard_acc in &stakers {
        match process_staker(shard_acc, gas_consumer) {
            Ok((addr, staker)) => {
                contracts.entry(addr).or_default().push(staker);
            }
            Err(err) => log::trace!(target: "tvm", "staker was not used due to: {}", err),
        }
    }
    contracts
}

/// get all valid validators with their stakers
fn find_validators(engine: &mut Engine, cfg17: &ConfigParam17) -> Result<Vec<Validator>> {
    let cfg30: DelectorParams = engine.read_config_param(30)?;
    let (validators, stakers) = match engine.index_provider.as_ref() {
        Some(index_provider) => (
            index_provider.get_accounts_by_init_code_hash(&cfg30.validator_init_code_hash)?,
            index_provider.get_accounts_by_init_code_hash(&cfg30.staker_init_code_hash)?,
        ),
        None => fail!("no index_provider set"),
    };
    let mc_seqno = engine.smci_param(13)?.as_integer()?.into(0..=u32::MAX)?;

    log::trace!(target: "tvm", "found {} validators", validators.len());
    let stakers = &mut process_stakers(stakers, engine);
    let cfg34_result = engine.read_config_param::<ConfigParam34>(34);
    let cur_validators = match cfg34_result.as_ref() {
        Ok(cfg34) => cfg34.cur_validators.list(),
        _ => &[]
    };
    let min_stake = cfg17.min_stake.as_u128();
    let max_stake = cfg17.max_stake.as_u128();
    let mut list = Vec::new();
    for validator in &validators {
        match process_validator(validator, stakers, &min_stake, &max_stake, engine) {
            Err(err) => {
                log::trace!(target: "tvm", "cannot use depool contract of account {:x}: {:?}",
                    validator.account_cell().repr_hash(), err);
            }
            Ok(mut descr) => {
                if cur_validators
                    .iter()
                    .any(|d| d.public_key.as_slice() == descr.key.pub_key.as_array())
                {
                    descr.mc_seq_no_since = mc_seqno;
                }
                list.push(descr)
            }
        }
    }
    Ok(list)
}

/// create tuple list with serializable objects
fn prepare_items_list<T: Serializable>(items: &[T]) -> Result<StackItem> {
    let mut tuple = StackItem::tuple(Vec::new());
    for item in items.iter().rev() {
        let cell = item.serialize()?;
        tuple = StackItem::tuple(vec![StackItem::Cell(cell), tuple]);
    }
    Ok(tuple)
}

fn execute_find_accounts<F>(engine: &mut Engine, name: &'static str, f: F) -> Status
where
    F: FnOnce(Arc<dyn IndexProvider>, &UInt256) -> Result<Vec<ShardAccount>>,
{
    engine.load_instruction(Instruction::new(name))?;
    engine.check_capability(GlobalCapabilities::CapIndexAccounts)?;
    fetch_stack(engine, 1)?;
    let hash = engine.cmd.var(0).as_slice()?.clone().get_next_hash()?;
    match engine.index_provider.clone() {
        Some(index_provider) => {
            let accounts = f(index_provider, &hash)?;
            let list = prepare_items_list(&accounts)?;
            engine.cc.stack.push(list);
            Ok(())
        }
        None => err!(ExceptionCode::FatalError, "no index_provider set"),
    }
}

pub(crate) fn execute_find_by_init_code_hash(engine: &mut Engine) -> Status {
    execute_find_accounts(engine, "FIND_BY_INIT_CODE_HASH", |index_provider, hash| {
        index_provider.get_accounts_by_init_code_hash(hash)
    })
}
pub(crate) fn execute_find_by_code_hash(engine: &mut Engine) -> Status {
    execute_find_accounts(engine, "FIND_BY_CODE_HASH", |index_provider, hash| {
        index_provider.get_accounts_by_code_hash(hash)
    })
}
pub(crate) fn execute_find_by_data_hash(engine: &mut Engine) -> Status {
    execute_find_accounts(engine, "FIND_BY_DATA_HASH", |index_provider, hash| {
        index_provider.get_accounts_by_data_hash(hash)
    })
}

#[cfg(test)]
#[path = "../tests/test_accounts.rs"]
mod tests;
