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
use executor::types::Instruction;
use num::BigInt;
use types::Exception;
// use types::{Exception, ExceptionCode, Result};
use sha2::Digest;
use stack::{IntegerData, StackItem};
use stack::serialization::{Deserializer};
use stack::integer::serialization::{Encoding, IntoSliceExt, UnsignedIntegerBigEndianEncoding};
use std::sync::Arc;

// (x - )
pub(crate) fn execute_addrand(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("ADDRAND"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut hasher = sha2::Sha256::new();
        hasher.input(ctx.engine.rand()?
            .into_builder::<UnsignedIntegerBigEndianEncoding>(256)?.data());
        hasher.input(ctx.engine.cmd.var(0).as_integer()?
            .into_builder::<UnsignedIntegerBigEndianEncoding>(256)?.data());
        let sha256 = hasher.result();
        ctx.engine.set_rand(UnsignedIntegerBigEndianEncoding::new(256)
            .deserialize(&sha256))?;
        Ok(ctx)
    })
    .err()
}

// (y - z)
pub(crate) fn execute_rand(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("RAND"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let mut hasher = sha2::Sha512::new();
        hasher.input(ctx.engine.rand()?
            .into_builder::<UnsignedIntegerBigEndianEncoding>(256)?.data());
        let sha512 = hasher.result();
        let rand = ctx.engine.cmd.var(0).as_integer()?.take_value_of(|value|
            Some(BigInt::from_bytes_be(num::bigint::Sign::Plus, &sha512[32..]) * value >> 256))?;
            ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(IntegerData::from(rand)?)));
        ctx.engine.set_rand(UnsignedIntegerBigEndianEncoding::new(256)
            .deserialize(&sha512[..32]))?;
        Ok(ctx)
    })
    .err()
}

// ( - x)
pub(crate) fn execute_randu256(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("RANDU256"))
    .and_then(|ctx| {
        let mut hasher = sha2::Sha512::new();
        hasher.input(ctx.engine.rand()?
            .into_builder::<UnsignedIntegerBigEndianEncoding>(256)?.data());
        let sha512 = hasher.result();
        ctx.engine.set_rand(UnsignedIntegerBigEndianEncoding::new(256)
            .deserialize(&sha512[..32]))?;
        ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(UnsignedIntegerBigEndianEncoding::new(256)
            .deserialize(&sha512[32..]))));
        Ok(ctx)
    })
    .err()
}

// (x - )
pub(crate) fn execute_setrand(engine: &mut Engine) -> Option<Exception> {
    engine.load_instruction(Instruction::new("SETRAND"))
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let rand = ctx.engine.cmd.var_mut(0).as_integer_mut()?;
        ctx.engine.set_rand(rand)?;
        Ok(ctx)
    })
    .err()
}
