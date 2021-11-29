
/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
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
    stack::integer::{
        behavior::{OperationBehavior, Quiet, Signaling},
        serialization::Encoding,
    },
    types::ResultOpt
};
use ton_types::{Result, BuilderData, SliceData};

use core::mem;
use num_traits::{One, Signed, Zero};
use std::cmp;
use std::cmp::Ordering;

#[macro_use]
pub mod behavior;
mod fmt;

type Int = num::BigInt;

#[derive(Clone, Debug, PartialEq, Eq)]
enum IntegerValue {
    NaN,
    Value(Int)
}

impl cmp::PartialOrd for IntegerValue {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        match (self, other) {
            (IntegerValue::Value(x), IntegerValue::Value(y)) => {
                x.partial_cmp(y)
            },
            _ => None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegerData {
    value: IntegerValue
}

impl Default for IntegerData {
    fn default() -> Self {
        IntegerData::zero()
    }
}

impl IntegerData {

    /// Constructs new (set to 0) value. This is just a wrapper for Self::zero().
    #[inline]
    pub fn new() -> IntegerData {
        Self::zero()
    }

    /// Constructs new (set to 0) value.
    #[inline]
    pub fn zero() -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(Int::zero())
        }
    }

    /// Constructs new (set to 1) value.
    #[inline]
    pub fn one() -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(Int::one())
        }
    }

    /// Constructs new (set to -1) value.
    #[inline]
    pub fn minus_one() -> IntegerData {
        IntegerData {
            value: IntegerValue::Value(
                Int::from_biguint(
                    num::bigint::Sign::Minus,
                    num::BigUint::one()
                )
            )
        }
    }

    /// Constructs new Not-a-Number (NaN) value.
    #[inline]
    pub fn nan() -> IntegerData {
        IntegerData {
            value: IntegerValue::NaN
        }
    }

    /// Constructs mask for bits
    /// it must be refactored to simplify
    pub fn mask(bits: usize) -> Self {
        IntegerData::one()
            .shl::<Quiet>(bits).unwrap()
            .sub::<Quiet>(&IntegerData::one()).unwrap()
    }

    /// Clears value (sets to 0).
    #[inline]
    pub fn withdraw(&mut self) -> IntegerData {
        mem::replace(self, IntegerData::new())
    }

    /// Replaces value to a given one.
    #[inline]
    pub fn replace(&mut self, new_value: IntegerData) {
        *self = new_value;
    }

    /// Checks if value is a Not-a-Number (NaN).
    #[inline]
    pub fn is_nan(&self) -> bool {
        self.value == IntegerValue::NaN
    }

    /// Checks if value is negative (less than zero).
    #[inline]
    pub fn is_neg(&self) -> bool {
        match &self.value {
            IntegerValue::NaN => false,
            IntegerValue::Value(ref value) => value.is_negative()
        }
    }

    /// Checks if value is zero.
    #[inline]
    pub fn is_zero(&self) -> bool {
        match &self.value {
            IntegerValue::NaN => false,
            IntegerValue::Value(ref value) => value.is_zero()
        }
    }

    /// constuct
    pub fn from_unsigned_bytes_be(data: impl AsRef<[u8]>) -> Self {
        Self {
            value: IntegerValue::Value(Int::from_bytes_be(num::bigint::Sign::Plus, data.as_ref()))
        }
    }

    /// Compares value with another taking in account behavior of operation.
    #[inline]
    pub(crate) fn compare<T: OperationBehavior>(&self, other: &IntegerData) -> ResultOpt<Ordering> {
        match (&self.value, &other.value) {
            (IntegerValue::Value(l), IntegerValue::Value(r)) => Ok(Some(l.cmp(r))),
            _ => {
                on_nan_parameter!(T)?;
                Ok(None)
            }
        }
    }

    /// Returns true if signed value fits into a given bits size; otherwise false.
    #[inline]
    pub fn fits_in(&self, bits: usize) -> bool {
        self.bitsize() <= bits
    }

    /// Returns true if unsigned value fits into a given bits size; otherwise false.
    #[inline]
    pub fn ufits_in(&self, bits: usize) -> bool {
        !self.is_neg() && self.ubitsize() <= bits
    }

    /// Determines a fewest bits necessary to express signed value.
    #[inline]
    pub fn bitsize(&self) -> usize {
        utils::process_value(self, |value| {
            utils::bitsize(value)
        })
    }

    /// Determines a fewest bits necessary to express unsigned value.
    #[inline]
    pub fn ubitsize(&self) -> usize {
        utils::process_value(self, |value| {
            debug_assert!(!value.is_negative());
            value.bits() as usize
        })
    }

    pub fn as_slice<T: Encoding>(&self, bits: usize) -> Result<SliceData> {
        Ok(self.as_builder::<T>(bits)?.into_cell()?.into())
    }

    pub fn as_builder<T: Encoding>(&self, bits: usize) -> Result<BuilderData> {
        if self.is_nan() {
            Signaling::on_nan_parameter(file!(), line!())?;
        }
        T::new(bits).try_serialize(self)
    }
    pub fn as_unsigned_bytes_be(&self) -> Result<Vec<u8>> {
        unimplemented!()
    }
}

impl AsRef<IntegerData> for IntegerData {
    #[inline]
    fn as_ref(&self) -> &IntegerData {
        self
    }
}

#[macro_use]
pub mod utils {
    use super::*;
    use std::ops::Not;

    #[inline]
    pub fn process_value<F, R>(value: &IntegerData, call_on_valid: F) -> R
    where
        F: Fn(&Int) -> R,
    {
        match value.value {
            IntegerValue::NaN => panic!("IntegerData must be a valid number"),
            IntegerValue::Value(ref value) => call_on_valid(value),
        }
    }

    /// This macro extracts internal Int value from IntegerData using given NaN behavior
    /// and NaN constructor.
    macro_rules! extract_value {
        ($T: ident, $v: ident, $nan_constructor: ident) => {
            match $v.value {
                IntegerValue::NaN => {
                    on_nan_parameter!($T)?;
                    return Ok($nan_constructor());
                },
                IntegerValue::Value(ref $v) => $v,
            }
        }
    }

    /// Unary operation. Checks lhs for NaN, unwraps it, calls closure and returns wrapped result.
    #[inline]
    pub fn unary_op<T, F, FNaN, FRes, RInt, R>(
        lhs: &IntegerData,
        callback: F,
        nan_constructor: FNaN,
        result_processor: FRes
    ) -> Result<R>
    where
        T: OperationBehavior,
        F: Fn(&Int) -> RInt,
        FNaN: Fn() -> R,
        FRes: Fn(RInt, FNaN) -> Result<R>,
    {
        let lhs = extract_value!(T, lhs, nan_constructor);

        result_processor(callback(lhs), nan_constructor)
    }

    /// Binary operation. Checks lhs & rhs for NaN, unwraps them, calls closure and returns wrapped result.
    #[inline]
    pub fn binary_op<T, F, FNaN, FRes, RInt, R>(
        lhs: &IntegerData,
        rhs: &IntegerData,
        callback: F,
        nan_constructor: FNaN,
        result_processor: FRes
    ) -> Result<R>
    where
        T: OperationBehavior,
        F: Fn(&Int, &Int) -> RInt,
        FNaN: Fn() -> R,
        FRes: Fn(RInt, FNaN) -> Result<R>,
    {
        let lhs = extract_value!(T, lhs, nan_constructor);
        let rhs = extract_value!(T, rhs, nan_constructor);

        result_processor(callback(lhs, rhs), nan_constructor)
    }

    #[inline]
    pub fn process_single_result<T, FNaN>(result: Int, nan_constructor: FNaN) -> Result<IntegerData>
    where
        T: OperationBehavior,
        FNaN: Fn() -> IntegerData,
    {
        IntegerData::from(result).or_else(|_| {
            on_integer_overflow!(T)?;
            Ok(nan_constructor())
        })
    }

    #[inline]
    pub fn process_double_result<T, FNaN>(result: (Int, Int), nan_constructor: FNaN)
        -> Result<(IntegerData, IntegerData)>
    where
        T: OperationBehavior,
        FNaN: Fn() -> (IntegerData, IntegerData),
    {
        let (r1, r2) = result;
        match IntegerData::from(r1) {
            Ok(r1) => Ok((r1, IntegerData::from(r2).unwrap())),
            Err(_) => {
                on_integer_overflow!(T)?;
                Ok(nan_constructor())
            },
        }
    }

    #[inline]
    pub fn construct_single_nan() -> IntegerData {
        IntegerData::nan()
    }

    #[inline]
    pub fn construct_double_nan() -> (IntegerData, IntegerData) {
        (construct_single_nan(), construct_single_nan())
    }

    /// Integer overflow checking. Returns true, if value fits into IntegerData; otherwise false.
    #[inline]
    pub fn check_overflow(value: &Int) -> bool {
        bitsize(value) < 258
    }

    #[inline]
    pub fn bitsize(value: &Int) -> usize {
        if value.is_zero() ||
           (value == &Int::from_biguint(num::bigint::Sign::Minus, num::BigUint::one())) {
            return 1
        }
        let res = value.bits() as usize;
        if value.is_positive() {
            return res + 1
        }
        // For negative values value.bits() returns correct result only when value is power of 2.
        let mut modpow2 = value.abs();
        modpow2 &= &modpow2 - 1;
        if modpow2.is_zero() {
            return res
        }
        res + 1
    }

    /// Perform in-place two's complement of the given digit iterator
    /// starting from the least significant byte.
    #[inline]
    pub fn twos_complement<'a, I>(digits: I)
    where
        I: IntoIterator<Item = &'a mut u32>,
    {
        let mut carry = true;
        for d in digits {
            *d = d.not();
            if carry {
                *d = d.wrapping_add(1);
                carry = d.is_zero();
            }
        }
    }
}

#[macro_use]
pub mod conversion;
pub mod serialization;
pub mod math;
pub mod bitlogics;



