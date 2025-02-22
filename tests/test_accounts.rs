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

mod common;
use std::cmp::Ordering;
use std::sync::Arc;

use common::*;
use ever_block::{GlobalCapabilities, ShardAccount};
use ever_block::{fail, ExceptionCode, Result, UInt256};
use ever_vm::executor::IndexProvider;

lazy_static::lazy_static! {
    static ref VALIDATOR_INIT_CODE_HASH: UInt256 = "1111111111111111111111111111111111111111111111111111111111111111".parse().unwrap();
    static ref STAKER_INIT_CODE_HASH: UInt256 = "2222222222222222222222222222222222222222222222222222222222222222".parse().unwrap();
}

struct FakeIndexProvider;

impl IndexProvider for FakeIndexProvider {
    fn get_accounts_by_init_code_hash(&self, hash: &UInt256) -> Result<Vec<ShardAccount>> {
        let mut list = Vec::new();
        if VALIDATOR_INIT_CODE_HASH.cmp(hash) == Ordering::Equal {
            list.push(ShardAccount::default());
            list.push(ShardAccount::default());
        }
        Ok(list)
    }
    fn get_accounts_by_code_hash(&self, _hash: &UInt256) -> Result<Vec<ShardAccount>> {
        fail!("something goes wrong for test purposes")
    }
    fn get_accounts_by_data_hash(&self, _hash: &UInt256) -> Result<Vec<ShardAccount>> {
        unreachable!()
    }
}

#[test]
fn test_get_accounts() {
    let index_provider = Arc::new(FakeIndexProvider);

    test_case(
        "
        PUSHSLICE x1111111111111111111111111111111111111111111111111111111111111111
        FIND_BY_INIT_CODE_HASH
        UNTUPLE 2
        NIP
        TLEN
        PUSHSLICE x2222222222222222222222222222222222222222222222222222222222222222
        FIND_BY_INIT_CODE_HASH
        TLEN
    ",
    )
    .with_capability(GlobalCapabilities::CapIndexAccounts)
    .with_index_provider(index_provider.clone())
    .expect_int_stack(&[2, 0]);

    test_case(
        "
        FIND_BY_INIT_CODE_HASH
    ",
    )
    .expect_failure(ExceptionCode::InvalidOpcode);

    test_case(
        "
        FIND_BY_INIT_CODE_HASH
    ",
    )
    .with_capability(GlobalCapabilities::CapIndexAccounts)
    .expect_failure(ExceptionCode::StackUnderflow);

    test_case(
        "
        NULL
        FIND_BY_INIT_CODE_HASH
    ",
    )
    .with_capability(GlobalCapabilities::CapIndexAccounts)
    .expect_failure(ExceptionCode::TypeCheckError);

    test_case(
        "
        PUSHSLICE xd5998356dcd3163456e95f5f8cf1697
        FIND_BY_CODE_HASH
    ",
    )
    .with_capability(GlobalCapabilities::CapIndexAccounts)
    .with_index_provider(index_provider.clone())
    .expect_failure(ExceptionCode::CellUnderflow);

    test_case(
        "
        PUSHSLICE xd5998356dcd3163456e95f5f8cf1697b262ac21a8ddda80ae47cac67a5ad7513
        FIND_BY_CODE_HASH
    ",
    )
    .with_capability(GlobalCapabilities::CapIndexAccounts)
    .with_index_provider(index_provider)
    .expect_failure(ExceptionCode::FatalError);
}
