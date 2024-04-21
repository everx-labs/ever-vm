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

use super::*;

#[test]
fn test_smart_contract_info_serialization_default() {
    let sci = SmartContractInfo::default();
    sci.into_temp_data_item();
}

fn check_additional_fields(capabilities: u64, count: usize) {
    let sci = SmartContractInfo {
        capabilities,
        ..Default::default()
    };
    let item = sci.into_temp_data_item();
    let result = item
        .as_tuple().expect("result must be a tuple")
        .first().expect("tuple must have at least one item")
        .as_tuple().expect("SMCI list must be a tuple")
        .len();
    assert_eq!(result, count, "wrong total count for capabilities {:X}", capabilities);
}

#[test]
fn test_smart_contract_info_with_different_caps() {
    check_additional_fields(GlobalCapabilities::CapMycode as u64, 11);
    check_additional_fields(GlobalCapabilities::CapInitCodeHash as u64, 12);
    check_additional_fields(GlobalCapabilities::CapStorageFeeToTvm as u64, 13);
    check_additional_fields(GlobalCapabilities::CapDelections as u64, 14);

    let capabilities = GlobalCapabilities::CapMycode as u64
        | GlobalCapabilities::CapStorageFeeToTvm as u64;
    check_additional_fields(capabilities, 13);
}
