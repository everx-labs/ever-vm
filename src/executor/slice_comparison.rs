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

use executor::engine::Engine;
use executor::engine::storage::fetch_stack;
use stack::{StackItem, IntegerData, SliceData};
use types::{Failure};
use executor::types::Instruction;
use std::sync::Arc;

fn unary<F>(engine: &mut Engine, name: &'static str, operation: F) -> Failure 
where 
    F: Fn(&SliceData) -> StackItem 
{
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let slice = ctx.engine.cmd.var(0).as_slice()?.clone();
        let r = operation(&slice);
        ctx.engine.cc.stack.push(r);
        Ok(ctx)
    })
    .err()
}

fn binary<F>(engine: &mut Engine, name: &'static str, operation: F) -> Failure
where 
    F: Fn(SliceData, SliceData) -> StackItem 
{
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let s0 = ctx.engine.cmd.var(0).as_slice()?.clone();
        let s1 = ctx.engine.cmd.var(1).as_slice()?.clone();
        let r = operation(s1, s0);
        Ok(ctx.engine.cc.stack.push(r))
    })
    .err()
}

fn common_prefix<F>(engine: &mut Engine, name: &'static str, operation: F) -> Failure
where 
    F: Fn(Option<SliceData>, Option<SliceData>) -> StackItem 
{
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let s0 = ctx.engine.cmd.var(0).as_slice()?;
        let s1 = ctx.engine.cmd.var(1).as_slice()?;
        let (_, r_s1, r_s0) = SliceData::common_prefix(s1, s0);
        let r = operation(r_s1, r_s0);
        Ok(ctx.engine.cc.stack.push(r))
    })
    .err()
}

/// SEMPTY (s – s = ∅), checks whether a Slice s is empty 
/// (i.e., contains no bits of data and no cell references).
pub(super) fn execute_sempty(engine: &mut Engine) -> Failure {
    unary(engine, "SEMPTY", |slice| boolean!(
        (slice.remaining_bits() == 0) && (slice.remaining_references() == 0)
    ))
}

/// SDEMPTY (s – s ≈ ∅), checks whether Slice s has no bits of data.
pub(super) fn execute_sdempty(engine: &mut Engine) -> Failure {
    unary(engine, "SDEMPTY", |slice| boolean!(slice.remaining_bits() == 0))
}

/// SREMPTY (s – r(s) = 0), checks whether Slice s has no refer- ences.
pub(super) fn execute_srempty (engine: &mut Engine) -> Failure {
    unary(engine, "SREMPTY", |slice| boolean!(slice.remaining_references() == 0))
}

/// SDFIRST (s – s0 = 1), checks whether the first bit of Slice s is a one.
pub(super) fn execute_sdfirst (engine: &mut Engine) -> Failure {
    unary(engine, "SDFIRST", |slice| boolean!(
        (slice.remaining_bits() > 0) && (slice.get_bits(0, 1).unwrap() == 1)
    ))
}

/// SDLEXCMP (s s′ – c), compares the data of s lexicographically 
/// with the data of s′, returning −1, 0, or 1 depending on the result. s > s` => 1
pub(super) fn execute_sdlexcmp(engine: &mut Engine) -> Failure {
    common_prefix(engine, "SDLEXCMP", |r_s1, r_s0| int!(
        if r_s0.is_none() && r_s1.is_none() {
            0
        } else if r_s0.is_some() && r_s1.is_some() {
            if r_s1.unwrap().get_next_bit().unwrap() {
                1
            } else {
                -1
            }
        } else if r_s1.is_some() {
            1
        } else {
            -1
        }
    ))
}

/// SDEQ(s s′ – s ≈ s′), checks whether the data parts of s and s′ coincide, 
/// equivalent to SDLEXCMP; ISZERO.
pub(super) fn execute_sdeq(engine: &mut Engine) -> Failure {
    common_prefix(engine, "SDEQ", |r_s1, r_s0| boolean!(
        r_s0.is_none() && r_s1.is_none()
    ))
}

/// SDPFX (s s′ – ?), checks whether s is a prefix of s′.
pub(super) fn execute_sdpfx(engine: &mut Engine) -> Failure {
    common_prefix(engine, "SDPFX", |r_s1, _| boolean!(r_s1.is_none()))
}

/// SDPFXREV (s s′ – ?), checks whether s′ is a prefix of s, equivalent
/// to SWAP; SDPFX.
pub(super) fn execute_sdpfxrev(engine: &mut Engine) -> Failure {
    common_prefix(engine, "SDPFXREV", |_, r_s0| boolean!(r_s0.is_none()))
}

/// SDPPFX (s s′ – ?), checks whether s is a proper prefix of s′ 
/// (i.e., prefix distinct from s′).
pub(super) fn execute_sdppfx(engine: &mut Engine) -> Failure {
    common_prefix(engine, "SDPPFX", |r_s1, r_s0| boolean!(
        r_s0.is_some() && r_s1.is_none()
    ))
}

/// SDPPFXREV (s s′ – ?), checks whether s′ is a proper prefix of s.
pub(super) fn execute_sdppfxrev(engine: &mut Engine) -> Failure {
    common_prefix(engine, "SDPPFXREV", |r_s1, r_s0| boolean!(
        r_s0.is_none() && r_s1.is_some()
    ))
}

/// SDSFX(s s′ – ?), checks whether s is a suffix of s′.
pub(super) fn execute_sdsfx(engine: &mut Engine) -> Failure {
    binary(engine, "SDSFX", |s1, mut s0| boolean!({
        let l0 = s0.remaining_bits();
        let l1 = s1.remaining_bits();
        if l1 <= l0 {
            s0.shrink_data(l0 - l1..);
            let (_, r_s0, r_s1) = SliceData::common_prefix(&s0, &s1);
            r_s0.is_none() && r_s1.is_none()
        } else {
            false
        }
    }))
}

/// SDSFXREV (s s′ – ?), checks whether s′ is a suffix of s.
pub(super) fn execute_sdsfxrev(engine: &mut Engine) -> Failure {
    binary(engine, "SDSFXREV", |mut s1, s0| boolean!({
        let l0 = s0.remaining_bits();
        let l1 = s1.remaining_bits();
        if l0 <= l1 {
            s1.shrink_data(l1 - l0..);
            let (_, r_s0, r_s1) = SliceData::common_prefix(&s0, &s1);
            r_s0.is_none() && r_s1.is_none()
        } else {
            false
        }
    }))
}

///  SDPSFX (s s′ – ?), checks whether s is a proper suffix of s′.
pub(super) fn execute_sdpsfx(engine: &mut Engine) -> Failure {
    binary(engine, "SDPSFX", |s1, mut s0| boolean!({
        let l0 = s0.remaining_bits();
        let l1 = s1.remaining_bits();
        if l1 < l0 {
            s0.shrink_data(l0 - l1..);
            let (_, r_s0, r_s1) = SliceData::common_prefix(&s0, &s1);
            r_s0.is_none() && r_s1.is_none()
        } else {
            false
        }
    }))
}

/// SDPSFXREV (s s′ – ?), checks whether s′ is a proper suffix of s.
pub(super) fn execute_sdpsfxrev(engine: &mut Engine) -> Failure {
    binary(engine, "SDPSFXREV", |mut s1, s0| boolean!({
        let l0 = s0.remaining_bits();
        let l1 = s1.remaining_bits();
        if l0 < l1 {
            s1.shrink_data(l1 - l0..);
            let (_, r_s0, r_s1) = SliceData::common_prefix(&s0, &s1);
            r_s0.is_none() && r_s1.is_none()
        } else {
            false
        }
    }))
}

/// SDCNTLEAD0 (s – n), returns the number of leading zeroes in s.
pub(super) fn execute_sdcntlead0(engine: &mut Engine) -> Failure {
    unary(engine, "SDCNTLEAD0", |slice| int!({
        let n = slice.remaining_bits();
        (0..n).position(|i| slice.get_bits(i, 1).unwrap() == 1).unwrap_or(n)
    }))
}

/// SDCNTLEAD1 (s – n), returns the number of leading ones in s.
pub(super) fn execute_sdcntlead1(engine: &mut Engine) -> Failure {
    unary(engine, "SDCNTLEAD1", |slice| int!({
        let n = slice.remaining_bits();
        (0..n).position(|i| slice.get_bits(i, 1).unwrap() == 0).unwrap_or(n)
    }))
}

/// SDCNTTRAIL0 (s – n), returns the number of trailing zeroes in s.
pub(super) fn execute_sdcnttrail0(engine: &mut Engine) -> Failure {
    unary(engine, "SDCNTTRAIL0", |slice| int!({
        let n = slice.remaining_bits();
        (0..n).position(|i| slice.get_bits(n - i - 1, 1).unwrap() == 1).unwrap_or(n)
    }))
}

/// SDCNTTRAIL1 (s – n), returns the number of trailing ones in s.
pub(super) fn execute_sdcnttrail1(engine: &mut Engine) -> Failure {
    unary(engine, "SDCNTTRAIL1", |slice| int!({
        let n = slice.remaining_bits();
        (0..n).position(|i| slice.get_bits(n - i - 1, 1).unwrap() == 0).unwrap_or(n)
    }))
}

