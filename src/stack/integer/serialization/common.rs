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

/// Calculates fewest byte count needed to fit a given bit count.
#[inline]
pub fn bits_to_bytes(length_in_bits: usize) -> usize {
    (length_in_bits + 7) >> 3
}

/// Calculates excess bits. Bit count which overflows octet.
#[inline]
pub fn calc_excess_bits(length_in_bits: usize) -> usize {
    length_in_bits & 0b111
}


#[inline]
fn get_fill(is_negative: bool) -> u8 {
    if is_negative {
        0xFF
    } else {
        0
    }
}

/// Extends buffer, if needed (big-endian).
#[inline]
pub fn extend_buffer_be(mut buffer: Vec<u8>, length_in_bits: usize, is_negative: bool) -> Vec<u8> {
    let new_len = bits_to_bytes(length_in_bits);
    if new_len > buffer.len() {
        let mut new_buffer = vec![get_fill(is_negative); new_len - buffer.len()];
        new_buffer.append(&mut buffer);
        new_buffer
    } else {
        buffer
    }
}

/// Extends buffer, if needed (little-endian).
#[inline]
pub fn extend_buffer_le(mut buffer: Vec<u8>, length_in_bits: usize, is_negative: bool) -> Vec<u8> {
    let new_len = bits_to_bytes(length_in_bits);
    if new_len > buffer.len() {
        buffer.resize(new_len, get_fill(is_negative));
    }
    buffer
}
