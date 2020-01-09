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

use super::{
    *,
    math::*,
    behavior::*
};

use std::ops::{
    Add,
    AddAssign,
    Sub,
    SubAssign,
    Mul,
    MulAssign,
    Div,
    DivAssign,
    Neg,
    BitAnd,
    BitAndAssign,
    BitOr,
    BitOrAssign,
    BitXor,
    BitXorAssign,
    Not,
    Shl,
    ShlAssign,
    Shr,
    ShrAssign,
};

fn unwrap<T>(result: Result<T>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{}", error),
    }
}

impl<T> PartialOrd<T> for IntegerData
where
    T: AsRef<IntegerData>,
    IntegerData: PartialEq<T>
{
    fn partial_cmp(&self, other: &T) -> Option<Ordering> {
        unwrap(self.cmp::<Quiet>(other.as_ref()))
    }
}

macro_rules! impl_trait {
    ($trait: ident, $rhs: ty, $for: ty, $trait_method: ident, $method: ident) => {
        impl $trait<$rhs> for $for {
            type Output = IntegerData;

            fn $trait_method(self, rhs: $rhs) -> Self::Output {
                unwrap(IntegerData::$method::<Signaling>(&self, &rhs))
            }
        }
    };

    ($trait: ident, $rhs: ty, $trait_method: ident, $method: ident) => {
        impl_trait!($trait, $rhs, IntegerData, $trait_method, $method);
        impl_trait!($trait, $rhs, &IntegerData, $trait_method, $method);
        impl_trait!($trait, &$rhs, IntegerData, $trait_method, $method);
        impl_trait!($trait, &$rhs, &IntegerData, $trait_method, $method);
    }
}

macro_rules! impl_primitive_rhs_trait {
    ($trait: ident, $rhs: ty, $for: ty, $trait_method: ident, $method: ident) => {
        impl $trait<$rhs> for $for {
            type Output = IntegerData;

            fn $trait_method(self, rhs: $rhs) -> Self::Output {
                unwrap(IntegerData::$method::<Signaling>(&self, rhs))
            }
        }

        impl $trait<&$rhs> for $for {
            type Output = IntegerData;

            fn $trait_method(self, rhs: &$rhs) -> Self::Output {
                unwrap(IntegerData::$method::<Signaling>(&self, *rhs))
            }
        }
    };

    ($trait: ident, $rhs: ty, $trait_method: ident, $method: ident) => {
        impl_primitive_rhs_trait!($trait, $rhs, IntegerData, $trait_method, $method);
        impl_primitive_rhs_trait!($trait, $rhs, &IntegerData, $trait_method, $method);
    }
}

macro_rules! impl_div_trait {
    ($rhs: ty, $for: ty) => {
        impl Div<$rhs> for $for {
            type Output = IntegerData;

            fn div(self, rhs: $rhs) -> Self::Output {
                unwrap(IntegerData::div::<Signaling>(&self, &rhs, Round::FloorToZero)).0
            }
        }
    };

    ($rhs: ty) => {
        impl_div_trait!($rhs, IntegerData);
        impl_div_trait!($rhs, &IntegerData);
        impl_div_trait!(&$rhs, IntegerData);
        impl_div_trait!(&$rhs, &IntegerData);
    }
}

macro_rules! impl_unary_trait {
    ($trait: ident, $for: ty, $trait_method: ident, $method: ident) => {
        impl $trait for $for {
            type Output = IntegerData;

            fn $trait_method(self) -> Self::Output {
                unwrap(IntegerData::$method::<Signaling>(&self))
            }
        }
    };

    ($trait: ident, $trait_method: ident, $method: ident) => {
        impl_unary_trait!($trait, IntegerData, $trait_method, $method);
        impl_unary_trait!($trait, &IntegerData, $trait_method, $method);
    }
}

macro_rules! impl_assign_trait {
    ($trait: ident, $rhs: ty, $for: ty, $trait_method: ident, $method: ident) => {
        impl $trait<$rhs> for $for {
            fn $trait_method(&mut self, rhs: $rhs) {
                *self = unwrap(IntegerData::$method::<Signaling>(&self, &rhs));
            }
        }
    };

    ($trait: ident, $rhs: ty, $trait_method: ident, $method: ident) => {
        impl_assign_trait!($trait, $rhs, IntegerData, $trait_method, $method);
        impl_assign_trait!($trait, &$rhs, IntegerData, $trait_method, $method);
    }
}

macro_rules! impl_div_assign_trait {
    ($rhs: ty) => {
        impl DivAssign<$rhs> for IntegerData {
            fn div_assign(&mut self, rhs: $rhs) {
                *self = unwrap(IntegerData::div::<Signaling>(&self, &rhs, Round::FloorToZero)).0;
            }
        }
    };

    ($rhs: ty: ident) => {
        impl_div_assign_trait!($rhs);
        impl_div_assign_trait!(&$rhs);
    }
}

macro_rules! impl_primitive_rhs_assign_trait {
    ($trait: ident, $rhs: ty, $trait_method: ident, $method: ident) => {
        impl $trait<$rhs> for IntegerData {
            fn $trait_method(&mut self, rhs: $rhs) {
                *self = unwrap(IntegerData::$method::<Signaling>(&self, rhs));
            }
        }

        impl $trait<&$rhs> for IntegerData {
            fn $trait_method(&mut self, rhs: &$rhs) {
                *self = unwrap(IntegerData::$method::<Signaling>(&self, *rhs));
            }
        }
    };
}

impl_trait!(Add, IntegerData, add, add);
impl_trait!(Add, i8, add, add_i8);
impl_trait!(Sub, IntegerData, sub, sub);
impl_trait!(Sub, i8, sub, sub_i8);
impl_trait!(Mul, IntegerData, mul, mul);
impl_trait!(Mul, i8, mul, mul_i8);
impl_div_trait!(IntegerData);

impl_assign_trait!(AddAssign, IntegerData, add_assign, add);
impl_assign_trait!(AddAssign, i8, add_assign, add_i8);
impl_assign_trait!(SubAssign, IntegerData, sub_assign, sub);
impl_assign_trait!(SubAssign, i8, sub_assign, sub_i8);
impl_assign_trait!(MulAssign, IntegerData, mul_assign, mul);
impl_assign_trait!(MulAssign, i8, mul_assign, mul_i8);
impl_div_assign_trait!(IntegerData);

impl_unary_trait!(Neg, neg, neg);
impl_unary_trait!(Not, not, not);

impl_trait!(BitAnd, IntegerData, bitand, and);
impl_trait!(BitOr, IntegerData, bitor, or);
impl_trait!(BitXor, IntegerData, bitxor, xor);

impl_assign_trait!(BitAndAssign, IntegerData, bitand_assign, and);
impl_assign_trait!(BitOrAssign, IntegerData, bitor_assign, or);
impl_assign_trait!(BitXorAssign, IntegerData, bitxor_assign, xor);

impl_primitive_rhs_trait!(Shl, usize, shl, shl);
impl_primitive_rhs_trait!(Shr, usize, shr, shr);

impl_primitive_rhs_assign_trait!(ShlAssign, usize, shl_assign, shl);
impl_primitive_rhs_assign_trait!(ShrAssign, usize, shr_assign, shr);
