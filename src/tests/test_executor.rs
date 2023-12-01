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

use crate::{
    executor::{
        serialize_currency_collection, engine::Engine, math::DivMode,
        types::{Instruction, InstructionOptions}
    },
    stack::{
        Stack, StackItem,
        integer::{IntegerData, behavior::{OperationBehavior, Signaling, Quiet}},
    },
    types::Status
};
use std::collections::HashSet;
use ton_types::{BuilderData, IBitstring, SliceData};

#[test]
fn test_assert_stack() {
    let mut engine = Engine::with_capabilities(0);
    engine.cc.stack.push(int!(0));
    engine.cc.stack.push(int!(-1));
    engine.cc.stack.push(int!(1));
    let mut stack = Stack::new();
    stack.push(int!(0));
    stack.push(int!(-1));
    stack.push(int!(1));
    engine.assert_stack(&stack);
}

#[test]
fn test_next_cmd_failed() {
    let mut engine = Engine::with_capabilities(0);
    engine.next_cmd().expect_err("Should be generated exception for empty code");
}

#[test]
fn test_div_mode_names_not_intersect() {
    let mut set = HashSet::new();
    for flags in 0..=0b11111111 {
        let mode = DivMode::with_flags(flags);
        if mode.shift_parameter() {
            continue;
        }
        if let Ok(name) = mode.command_name() {
            assert!(set.insert(name.to_string()));
        }
    }
}

#[test]
fn test_division_primitives_execution() {
    let mut count = 0;
    for flags in 0..=0b11111111 {
        let mode = DivMode::with_flags(flags);
        if !mode.is_valid() {
            println!("Flags: {:#010b}, <NOT IMPLEMENTED>", mode.flags);
            continue;
        }
        test_div_primitive_execution::<Signaling>(&mode);
        test_div_primitive_execution::<Quiet>(&mode);
        if !mode.shift_parameter() {
            count += 1;
        }

    }
    assert_eq!(45, count);
}

fn get_command_name<T>(name: &str) -> String
where
    T: OperationBehavior
{
    let mut result = name.to_owned();
    if let Some(str) = T::name_prefix() {
        result.insert_str(0, str)
    };
    result
}

fn command_name_from_mode<T>(mode: &DivMode) -> String
where
    T: OperationBehavior
{
    match mode.command_name() {
        Ok(name) => {
            get_command_name::<T>(name)
        },
        Err(_) => {
            panic!("Flags: {:#010b}, Cmd: <NOT IMPLEMENTED>", mode.flags)
        }
    }
}

fn test_div_primitive_execution<T>(mode: &DivMode)
where
    T: OperationBehavior
{
    let command_name = command_name_from_mode::<T>(mode);
    println!("Flags: {:#010b}, Cmd: {}", mode.flags, command_name);

    let mut value = 15;
    let mul_shift = 3;
    let div_shift = 1;

    let multiplier: i32 = 1 << mul_shift;
    let divisor: i32 = 1 << div_shift;
    let mut stack = Stack::new();
    let mut swap = 0;

    stack.push(int!(value));

    if mode.premultiply() && (!mode.mul_by_shift() || !mode.shift_parameter()) {
        stack.push(int!(
            if mode.mul_by_shift() {
                swap = 1;
                mul_shift
            } else {
                multiplier
            }));
    }

    if !(mode.div_by_shift() && mode.shift_parameter()) {
        stack.push(int!(
            if mode.div_by_shift() {
                div_shift
            } else {
                if swap == 1 {
                    swap = 2
                }
                divisor
            }));
    }
    if swap == 2 {
        stack.swap(1, 0).unwrap()
    }

    let code = div_generate_bytecode::<T>(mode, mul_shift as u8, div_shift as u8);
    let mut engine = Engine::with_capabilities(0)
        .setup_with_libraries(code, None, Some(stack), None, vec![]);

    match engine.execute() {
        Err(e) => panic!("Execute error: {}", e),
        Ok(_) => {
            if mode.premultiply() {
                value *= multiplier
            }

            let (expected_quotient, expected_remainder)
                = IntegerData::from_i32(value).div::<T>(
                &IntegerData::from_i32(divisor),
                mode.rounding_strategy().unwrap()
            ).unwrap();

            if mode.need_remainder() {
                let actual_remainder_si = engine.cc.stack.drop(0).unwrap();
                let actual_remainder = actual_remainder_si.as_integer().unwrap();
                assert_eq!(expected_remainder, *actual_remainder, "Remainder");
            }

            if mode.need_quotient() {
                let actual_quotient_si = engine.cc.stack.drop(0).unwrap();
                let actual_quotient = actual_quotient_si.as_integer().unwrap();
                assert_eq!(expected_quotient, *actual_quotient, "Quotient");
            }
        }
    }
}

fn div_generate_bytecode<T>(mode: &DivMode, mul_shift: u8, div_shift: u8) -> SliceData
where
    T: OperationBehavior
{
    let mut res = Vec::<u8>::with_capacity(5);
    if T::quiet() {
        res.push(0xB7);
    }

    res.push(0xA9);
    res.push(mode.flags);
    if mode.shift_parameter() && (mode.mul_by_shift() || mode.div_by_shift()) {
        if mode.mul_by_shift() {
            res.push(mul_shift - 1);
        } else {
            res.push(div_shift - 1);
        }
    }

    res.push(0x80);
    SliceData::new(res)
}

fn test_slice(offset: usize, r: usize, x: usize) -> Status {
    let mut builder = BuilderData::default();
    builder.append_bits(0x7A53, offset)?; // prefix of command
    builder.append_bits(0, r)?; // references
    builder.append_bits(2, x)?; // bytes
    builder.append_bits(0, (8 - (offset + r + x) % 8) % 8)?; // remainder of data
    builder.append_bits(0xF837, 16)?; // data 2 bytes
    builder.append_bits(0x34, 8)?; // remainder in code slice

    let mut code = SliceData::load_builder(builder).unwrap();
    println!("offset: {}, r: {}, x: {}, code: {}", offset, r, x, code);
    let mut engine = Engine::with_capabilities(0)
        .setup_with_libraries(code.clone(), None, None, None, vec![]);
    engine.load_instruction(
        Instruction::new("PUSHCTR").set_opts(InstructionOptions::Bitstring(offset, r, x, 0))
    ).unwrap();


    let slice = engine.cmd.slice().clone();
    assert_eq!(engine.seek_next_cmd().unwrap(), None);

    let mut remainder = code.clone();
    remainder.shrink_data(32..);
    assert_eq!(&remainder, engine.cc.code());

    code.shrink_data(offset + r + x..31);
    assert_eq!(code, slice);
    Ok(())
}

#[test]
fn test_extract_slice() {
    test_slice( 9, 2, 3).unwrap(); // STSLICECONST a command, x, r and data in the same byte
    test_slice( 6, 0, 7).unwrap();
    test_slice( 7, 2, 7).unwrap();
    test_slice(12, 0, 4).unwrap();
    test_slice( 8, 0, 4).unwrap();
    test_slice( 8, 2, 5).unwrap();
    test_slice( 0, 3, 7).unwrap();

    for r in 0..4 {
        for x in 2..8 {
            let min_offset = std::cmp::min(8, 16-r-x);
            for offset in min_offset..16-r-x {
                test_slice(offset, r, x).unwrap();

            }
        }
    }
}

#[test]
fn test_currency_collection_ser() {
    let b1 = serialize_currency_collection(12345678u128, None).unwrap();
    let b2 = BuilderData::with_raw(vec![0x3b, 0xc6, 0x14, 0xe0], 29).unwrap();
    assert_eq!(b1, b2);
}

