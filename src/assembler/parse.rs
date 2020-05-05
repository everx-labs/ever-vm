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

use num::{
    Num,
};
use std::cmp::PartialOrd;
use std::ops::Bound;
use std::ops::{
    Range,
    RangeBounds,
};
use super::errors::ParameterError;

fn parse_range<T, R>(range: R) -> impl Fn(&str) -> Result<T, ParameterError>
where
    T: Num + PartialOrd,
    R: RangeBounds<T>,
{
    move |p: &str| match T::from_str_radix(p, 10) {
        Ok(value) => {
            match range.start_bound() {
                Bound::Included(min) => {
                    if value < *min {
                        return Err(ParameterError::OutOfRange);
                    }
                }
                Bound::Excluded(min_excluded) => {
                    if value <= *min_excluded {
                        return Err(ParameterError::OutOfRange);
                    }
                }
                Bound::Unbounded => {}
            }
            match range.end_bound() {
                Bound::Included(max) => {
                    if value > *max {
                        return Err(ParameterError::OutOfRange);
                    }
                }
                Bound::Excluded(max_excluded) => {
                    if value >= *max_excluded {
                        return Err(ParameterError::OutOfRange);
                    }
                }
                Bound::Unbounded => {}
            }
            Ok(value)
        }
        _ => Err(ParameterError::UnexpectedType),
    }
}

pub(super) fn parse_const_u2(par: &str) -> Result<u8, ParameterError> {
    parse_range(0..4)(par)
}

pub(super) fn parse_const_i4(par: &str) -> Result<u8, ParameterError> {
    parse_range(-1i8..=14)(par).map(|e| (e & 0x0F) as u8)
}

pub(super) fn parse_const_u4(par: &str) -> Result<u8, ParameterError> {
    parse_range(0u8..=15)(par)
}

pub(super) fn parse_const_u4_plus_one(par: &str) -> Result<u8, ParameterError> {
    parse_range(1u8..=16)(par).map(|e| (e - 1) as u8)
}

pub(super) fn parse_const_u4_plus_two(par: &str) -> Result<u8, ParameterError> {
    parse_range(2u8..=17)(par).map(|e| (e - 2) as u8)
}

pub(super) fn parse_const_u4_14(par: &str) -> Result<u8, ParameterError> {
    parse_range(0i8..=14)(par).map(|e| e as u8)
}

pub(super) fn parse_const_u4_1_14(par: &str) -> Result<u8, ParameterError> {
    parse_range(1i8..=14)(par).map(|e| e as u8)
}

pub(super) fn parse_const_u4_nonzero(par: &str) -> Result<u8, ParameterError> {
    parse_range(1u8..=16)(par)
}

// 5-bit arguments
pub(super) fn parse_const_u5(par: &str) -> Result<u8, ParameterError> {
    parse_range(0u8..32)(par)
}

// 10-bit arguments
pub(super) fn parse_const_u10(par: &str) -> Result<u16, ParameterError> {
    parse_range(0..1024)(par)
}

// 11-bit arguments for THROW* instructions
pub(super) fn parse_const_u11(par: &str) -> Result<u16, ParameterError> {
    parse_range(0u16..2048)(par)
}

pub(super) fn parse_const_u14(par: &str) -> Result<u16, ParameterError> {
    parse_range(0u16..16384)(par)
}

/// parses as argument for SETCP -15..240
pub(super) fn parse_const_u8_setcp(par: &str) -> Result<u8, ParameterError> {
    parse_range(-15..240)(par).map(|z| z as u8)
}

pub(super) fn parse_const_i8(par: &str) -> Result<u8, ParameterError> {
    parse_range(-128i16..=127)(par).map(|e| e as u8)
}

pub(super) fn parse_const_u8_plus_one(par: &str) -> Result<u8, ParameterError> {
    parse_range(1u16..=256)(par).map(|e| (e - 1) as u8)
}

pub(super) fn parse_const_u8_240(par: &str) -> Result<u8, ParameterError> {
    parse_range(0u8..240)(par)
}

pub(super) fn parse_control_register(par: &str) -> Result<u8, ParameterError> {
    Ok(parse_register(par, 'C', 0..16)? as u8)
}

pub(super) fn parse_register(
    register: &str,
    symbol: char,
    range: Range<isize>,
) -> Result<isize, ParameterError> {
    if register.len() <= 1 {
        Err(ParameterError::UnexpectedType)
    } else if register.chars().next().unwrap().to_ascii_uppercase() != symbol {
        Err(ParameterError::UnexpectedType)
    } else {
        match isize::from_str_radix(&register[1..], 10) {
            Ok(number) => if (number < range.start) || (number >= range.end) {
                Err(ParameterError::OutOfRange)
            } else {
                Ok(number)
            },
            Err(_e) => Err(ParameterError::UnexpectedType)
        }
    }
}


pub fn parse_slice(slice: &str, bits: usize) -> Result<Vec<u8>, ParameterError> {
    if slice.len() <= 1 {
        log::error!(target: "compile", "empty string");
        Err(ParameterError::UnexpectedType)
    } else if slice.chars().next().unwrap().to_ascii_uppercase() != 'X' {
        log::error!(target: "compile", "base not set");
        Err(ParameterError::UnexpectedType)
    } else {
        parse_slice_base(&slice[1..], bits, 16)
    }
}

pub fn parse_slice_base(slice: &str, mut bits: usize, base: u32) -> Result<Vec<u8>, ParameterError> {
    debug_assert!(bits < 8, "it is offset to get slice parsed");
    let mut acc = 0u8;
    let mut data = vec![];
    let mut completion_tag = false;
    for ch in slice.chars() {
        if completion_tag {
            return Err(ParameterError::UnexpectedType);
        }
        match ch.to_digit(base) {
            Some(x) => {
                if bits < 4 {
                    acc |= (x << (4 - bits)) as u8;
                    bits += 4;
                } else {
                    data.push(acc | (x as u8 >> (bits - 4)));
                    acc = (x << (12 - bits)) as u8;
                    bits -= 4;
                }
            }
            None => {
                if ch == '_' {
                    completion_tag = true
                } else {
                    return Err(ParameterError::UnexpectedType);
                }
            }
        }
    }
    if bits != 0 {
        if !completion_tag {
            acc |= 1 << (7 - bits);
        }
        if acc != 0 || data.is_empty() {
            data.push(acc);
        }
    } else if !completion_tag {
        data.push(0x80);
    }
    Ok(data)
}

pub(super) fn parse_stack_register_u4(par: &str) -> Result<u8, ParameterError> {
    Ok(parse_register(par, 'S', 0..16)? as u8)
}

pub(super) fn parse_stack_register_u4_minus_one(par: &str) -> Result<u8, ParameterError> {
    Ok((parse_register(par, 'S', -1..15)? + 1) as u8)
}

pub(super) fn parse_stack_register_u4_minus_two(par: &str) -> Result<u8, ParameterError> {
    Ok((parse_register(par, 'S', -2..14)? + 2) as u8)
}

pub(super) fn parse_plduz_parameter(par: &str) -> Result<u8, ParameterError> {
    (parse_range(32u16..=256))(par)
        .and_then(|c| {
            if c % 32 == 0 {
                Ok(((c / 32) - 1) as u8)
            } else {
                Err(ParameterError::OutOfRange)
            }
        })
}
