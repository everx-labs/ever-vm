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
    error::TvmError,
    executor::gas::gas_state::Gas,
    stack::{continuation::ContinuationData, integer::IntegerData},
    types::{Exception, ResultMut, ResultOpt, ResultRef, ResultVec, Status}
};
use std::{fmt, mem, ops::Range, slice::Iter, sync::Arc};
use integer::serialization::{Encoding, SignedIntegerBigEndianEncoding};
use serialization::Deserializer;
use ton_types::{BuilderData, Cell, CellType, ExceptionCode, HashmapType, IBitstring, MAX_DATA_BITS, MAX_REFERENCES_COUNT, Result, SliceData, error};

pub mod serialization;
pub mod savelist;
pub mod continuation;
#[macro_use]
pub mod integer;

#[macro_export]
macro_rules! int {
    (nan) => {
        StackItem::nan()
    };
    ($value: expr) => {
        StackItem::Integer(Arc::new(IntegerData::from($value).unwrap()))
    };
    (parse $str: expr) => {
        StackItem::Integer(Arc::new(std::str::FromStr::from_str($str).unwrap()))
    };
    (parse_hex $str: expr) => {
        StackItem::Integer(Arc::new(IntegerData::from_str_radix($str, 16).unwrap()))
    };
}

#[macro_export]
macro_rules! boolean {
    ($val:expr) => {
        if $val {
            int!(-1)
        } else {
            int!(0)
        }
    };
}

#[derive(Debug, PartialEq)]
pub enum StackItem {
    None,
    Builder(Arc<BuilderData>),
    Cell(Cell),
    Continuation(Arc<ContinuationData>),
    Integer(Arc<IntegerData>),
    Slice(SliceData),
    Tuple(Arc<Vec<StackItem>>)
}

fn slice_serialize(slice: &SliceData) -> Result<BuilderData> {
    let mut builder = BuilderData::new();
    builder.append_reference_cell(slice.cell().clone());
    builder.append_bits(slice.pos(), 10)?;
    builder.append_bits(slice.remaining_bits() + slice.pos(), 10)?;
    builder.append_bits(slice.get_references().start, 3)?;
    builder.append_bits(slice.get_references().end, 3)?;
    Ok(builder)
}

fn slice_deserialize(slice: &mut SliceData) -> Result<SliceData> {
    let cell = slice.checked_drain_reference()?;
    let data_start = slice.get_next_int(10)? as usize;
    let data_end = slice.get_next_int(10)? as usize;
    if data_start > MAX_DATA_BITS || data_end > MAX_DATA_BITS || data_start > data_end {
        return err!(ExceptionCode::FatalError)
    }
    let ref_start = slice.get_next_int(3)? as usize;
    let ref_end = slice.get_next_int(3)? as usize;
    if ref_start > MAX_REFERENCES_COUNT || ref_end > MAX_REFERENCES_COUNT || ref_start > ref_end {
        return err!(ExceptionCode::FatalError)
    }
    let mut res = SliceData::from(cell);
    res.shrink_data(data_start..data_end);
    res.shrink_references(ref_start..ref_end);
    Ok(res)
}

impl StackItem {

    /// new default stack item
    pub const fn default() -> Self {
        StackItem::None
    }

    /// new stack item as builder
    pub fn builder(builder: BuilderData) -> Self {
        StackItem::Builder(Arc::new(builder))
    }

    /// new stack item as cell
    pub fn cell(cell: Cell) -> Self {
        StackItem::Cell(cell)
    }

    /// new stack item as cell
    pub fn dict(dict: &impl HashmapType) -> Self {
        match dict.data() {
            Some(root) => StackItem::Cell(root.clone()),
            None => StackItem::None
        }
    }

    /// new stack item as continuation
    pub fn continuation(continuation: ContinuationData) -> Self {
        StackItem::Continuation(Arc::new(continuation))
    }

    /// new stack item as integer
    pub fn int(integer: impl Into<IntegerData>) -> Self {
        StackItem::Integer(Arc::new(integer.into()))
    }

    /// new stack item as integer with internal data
    pub fn integer(integer: IntegerData) -> Self {
        StackItem::Integer(Arc::new(integer))
    }

    /// new stack item as integer not a number
    pub fn nan() -> Self {
        StackItem::Integer(Arc::new(IntegerData::nan()))
    }

    /// new stack item as bool
    pub fn boolean(boolean: bool) -> Self {
        match boolean {
            true => StackItem::Integer(Arc::new(IntegerData::minus_one())),
            false => StackItem::Integer(Arc::new(IntegerData::zero())),
        }
    }

    /// new stack item as slice
    pub fn slice(slice: SliceData) -> Self {
        StackItem::Slice(slice)
    }

    /// new stack item as tuple
    pub fn tuple(tuple: Vec<StackItem>) -> Self {
        StackItem::Tuple(Arc::new(tuple))
    }

    /// Returns integer not equal to zero
    /// Checks type and NaN
    pub fn as_bool(&self) -> Result<bool> {
        match self {
            StackItem::Integer(ref data) => {
                if data.is_nan() {
                    err!(ExceptionCode::IntegerOverflow)
                } else {
                    Ok(!data.is_zero())
                }
            }
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_builder(&self) -> ResultRef<BuilderData> {
        match self {
            StackItem::Builder(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    /// Extracts builder to modify, exceptions should not be after
    /// If is single reference it will not clone on write
    pub fn as_builder_mut(&mut self) -> Result<BuilderData> {
        self.as_builder()?;
        match self.withdraw() {
            StackItem::Builder(ref mut data) =>
                Ok(mem::replace(Arc::make_mut(data), BuilderData::default())),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_cell(&self) -> ResultRef<Cell> {
        match self {
            StackItem::Cell(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_continuation(&self) -> ResultRef<ContinuationData> {
        match self {
            StackItem::Continuation(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_continuation_mut(&mut self) -> ResultMut<ContinuationData> {
        match self {
            StackItem::Continuation(ref mut data) => Ok(Arc::make_mut(data)),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    /// Returns type D None or Cell
    pub fn as_dict(&self) -> ResultOpt<&Cell> {
        match self {
            StackItem::None => Ok(None),
            StackItem::Cell(ref data) => Ok(Some(data)),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_integer(&self) -> ResultRef<IntegerData> {
        match self {
            StackItem::Integer(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_small_integer(&self) -> Result<usize> {
        self.as_integer()?.into(0..=255)
    }

    pub fn as_integer_mut(&mut self) -> ResultMut<IntegerData> {
        match self {
            StackItem::Integer(ref mut data) => Ok(Arc::make_mut(data)),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_slice(&self) -> ResultRef<SliceData> {
        match self {
            StackItem::Slice(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_tuple(&self) -> ResultRef<Vec<StackItem>> {
        match self {
            StackItem::Tuple(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    /// Extracts tuple to modify, exceptions should not be after
    /// If is single reference it will not clone on write
    pub fn as_tuple_mut(&mut self) -> ResultVec<StackItem> {
        self.as_tuple()?;
        match self.withdraw() {
            StackItem::Tuple(arc) => {
                Ok(Arc::try_unwrap(arc).unwrap_or_else(|arc| arc.as_ref().clone()))
            }
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    /// Returns integer as grams and checks range 0..2^120
    pub fn as_grams(&self) -> Result<u128> {
        self.as_integer()?.into(0..=(1u128<<120)-1)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, StackItem::None)
    }

    pub fn is_slice(&self) -> bool {
        matches!(self, StackItem::Slice(_))
    }

    pub fn withdraw(&mut self) -> StackItem {
        mem::take(self)
    }

    pub fn dump_as_fift(&self) -> String {
        match self {
            StackItem::None => "(null)".to_string(),
            StackItem::Integer(data) => data.clone().to_string(),
            StackItem::Cell(data) => format!("C{{{:X}}}", data.repr_hash()),
            StackItem::Continuation(_data) => "???".to_string(),
            StackItem::Builder(data) => {
                let bits = data.length_in_bits();
                let mut bytes = vec![data.references_used() as u8];
                let mut l = 2 * (bits / 8) as u8;
                let tag = if bits & 7 != 0 {
                    l += 1;
                    0x80 >> (bits & 7)
                } else {
                    0
                };
                bytes.push(l);
                bytes.extend_from_slice(data.data());
                *bytes.last_mut().unwrap() |= tag; // safe because vector always not empty
                format!("BC{{{}}}", hex::encode(bytes))
            }
            StackItem::Slice(data) => {
                let d1 = |level_mask : u8, refs_count : u8, is_special: u8| {
                    (refs_count + 8 * is_special + 32 * level_mask) as u8
                };
                let d2 = |bits : u32| {
                    let res = ((bits / 8) * 2) as u8;
                    if bits & 7 != 0 { res + 1 } else { res }
                };
                let start = data.pos();
                let end = start + data.remaining_bits();
                let refs = data.get_references();
                let data = SliceData::from(data.cell());
                let mut bytes = vec![];
                let is_special = data.cell().cell_type() != CellType::Ordinary;
                bytes.push(d1(data.cell().level_mask().mask(), data.cell().references_count() as u8, is_special as u8));
                bytes.push(d2(data.remaining_bits() as u32));
                bytes.extend_from_slice(data.storage());
                if bytes.last() == Some(&0x80) {
                    bytes.pop();
                }
                format!("CS{{Cell{{{}}} bits: {}..{}; refs: {}..{}}}",
                    hex::encode(bytes),
                    start, end, refs.start, refs.end
                )
            }
            StackItem::Tuple(data) => if data.is_empty() {
                "[]".to_string()
            } else {
                format!("[ {} ]", data.iter().map(|v| v.dump_as_fift()).collect::<Vec<_>>().join(" "))
            }
        }
    }

    pub fn serialize(&self) -> Result<(BuilderData, i64)> {
        let mut builder = BuilderData::new();
        let mut gas = 0;
        match self {
            StackItem::None => {
                builder.append_bits(0x00, 8)?;
            },
            StackItem::Integer(data) => {
                if data.is_nan() {
                    builder.append_bits(0x02ff, 16)?;
                } else {
                    builder.append_bits(0x02, 8)?;
                    builder.append_bits(0x00, 7)?;
                    builder.append_builder(&data.as_builder::<SignedIntegerBigEndianEncoding>(257)?)?;
                }
            },
            StackItem::Cell(data) => {
                builder.append_bits(0x03, 8)?;
                builder.append_reference_cell(data.clone());
            },
            StackItem::Continuation(data) => {
                builder.append_bits(0x06, 8)?;
                let (serialized, gas2) = data.serialize()?;
                gas += gas2;
                builder.append_builder(&serialized)?;
            },
            StackItem::Builder(data) => {
                builder.append_bits(0x05, 8)?;
                let cell = data.as_ref().clone().into_cell()?;
                builder.append_reference_cell(cell);
                gas += Gas::finalize_price();
            },
            StackItem::Slice(data) => {
                builder.append_bits(0x04, 8)?;
                builder.append_builder(&slice_serialize(data)?)?;
            },
            StackItem::Tuple(data) => {
                builder.append_bits(0x07, 8)?;
                let mut tuple = BuilderData::new();
                tuple.append_bits(data.len(), 8)?;
                let mut tuple_list = BuilderData::new();
                for item in data.iter().rev() {
                    let mut cons = BuilderData::new();
                    let (serialized, gas2) = item.serialize()?;
                    gas += gas2;
                    cons.append_builder(&serialized)?;
                    cons.append_reference_cell(tuple_list.into_cell()?);
                    gas += Gas::finalize_price();
                    tuple_list = cons;
                }
                tuple.append_builder(&tuple_list)?;
                builder.append_builder(&tuple)?;
            }
        }
        Ok((builder, gas))
    }

    pub fn deserialize(slice: &mut SliceData) -> Result<(StackItem, i64)> {
        let mut gas = 0;
        match slice.get_next_byte()? {
            0x00 => Ok((StackItem::None, gas)),
            0x02 => {
                match slice.get_next_int(7)? {
                    0x00 => {
                        let value = SignedIntegerBigEndianEncoding::new(257).deserialize(slice.get_next_bits(257)?.as_slice());
                        Ok((StackItem::integer(value), gas))
                    }
                    0x7f => {
                        if slice.get_next_bit()? {
                            Ok((StackItem::nan(), gas))
                        } else {
                            err!(ExceptionCode::UnknownError)
                        }
                    }
                    _ => err!(ExceptionCode::UnknownError)
                }
            },
            0x03 => Ok((StackItem::cell(slice.checked_drain_reference()?), gas)),
            0x04 => {
                gas += Gas::load_cell_price(true);
                Ok((StackItem::slice(slice_deserialize(slice)?), gas))
            },
            0x05 => Ok((StackItem::builder(BuilderData::from(slice.checked_drain_reference()?)), gas)),
            0x06 => {
                let (cont, gas2) = ContinuationData::deserialize(slice)?;
                gas += gas2;
                Ok((StackItem::continuation(cont), gas))
            },
            0x07 => {
                let mut tuple = vec![];
                let len = slice.get_next_int(8)? as usize;
                if len > 0 {
                    let (item, gas2) = StackItem::deserialize(slice)?;
                    tuple.push(item);
                    gas += gas2;
                }
                let mut cell = slice.checked_drain_reference()?;
                for _ in 1..len {
                    let mut slice = SliceData::from(cell);
                    gas += Gas::load_cell_price(true);
                    let (item, gas2) = StackItem::deserialize(&mut slice)?;
                    tuple.push(item);
                    gas += gas2;
                    cell = slice.checked_drain_reference()?;
                }
                Ok((StackItem::tuple(tuple), gas))
            },
            _ => err!(ExceptionCode::UnknownError)
        }
    }
}

impl Default for StackItem {
    fn default() -> StackItem {
        StackItem::None
    }
}

#[rustfmt::skip]
impl Clone for StackItem {
    fn clone(&self) -> StackItem {
        match self {
            StackItem::None            => StackItem::None,
            StackItem::Builder(x)      => StackItem::Builder(x.clone()),
            StackItem::Cell(x)         => StackItem::Cell(x.clone()),
            StackItem::Continuation(x) => StackItem::Continuation(x.clone()),
            StackItem::Integer(x)      => StackItem::Integer(x.clone()),
            StackItem::Slice(x)        => StackItem::Slice(x.clone()),
            StackItem::Tuple(x)        => StackItem::Tuple(x.clone()),
        }
    }
}

#[rustfmt::skip]
impl fmt::Display for StackItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StackItem::None            => write!(f, "Null"),
            StackItem::Builder(x)      => write!(f, "Builder {}", Arc::as_ref(x)),
            StackItem::Cell(x)         => write!(f, "Cell x{:x} x{:x}", x.repr_hash(), x),
            StackItem::Continuation(x) => write!(f, "Continuation x{:x}", x.code().cell().repr_hash()),
            StackItem::Integer(x)      => write!(f, "{}", Arc::as_ref(x)),
            StackItem::Slice(x)        => write!(f, "Slice x{:x}", x),
            StackItem::Tuple(x)        => write!(f, "Tuple ({})", x.iter().map(|v| format!("{}", v)).collect::<Vec<_>>().join(", ")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stack {
    pub storage: Vec<StackItem>,
}

impl Stack {

    pub const fn new() -> Self {
        Stack {
            storage: Vec::new(),
        }
    }

    // Swaps blocks (0...j-1) and (j...j+i-1)
    // e.g. block_swap(i=2, j=4): (8 7 6 {5 4} {3 2 1 0} -> 8 7 6 {3 2 1 0} {5 4})
    pub fn block_swap(&mut self, i: usize, j: usize) -> Status {
        if self.depth() < j + i {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let mut block = self.drop_range(j..j + i)?;
            while let Some(x) = block.pop() {
                self.push(x);
            }
            Ok(())
        }
    }

    pub fn depth(&self) -> usize {
        self.storage.len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn drop_top(&mut self, n: usize) {
        let depth = self.depth();
        if depth < n {
            log::error!(
                 target: "tvm",
                 "Corrupted stack state. This method can only be called \
                  when stack state is well known."
            );
        } else {
            self.storage.truncate(depth - n);
        }
    }

    pub fn drop(&mut self, i: usize) -> Result<StackItem> {
        let depth = self.depth();
        if i >= depth {
            err!(ExceptionCode::StackUnderflow)
        } else {
            Ok(self.storage.remove(depth - i - 1))
        }
    }

    pub fn drop_range(&mut self, range: Range<usize>) -> ResultVec<StackItem> {
        if range.is_empty() {
            return Ok(vec!())
        }
        let depth = self.depth();
        if range.end > depth {
            err!(ExceptionCode::StackUnderflow, "drop_range: {}..{}, depth: {}", range.start, range.end, depth)
        } else {
            Ok(self.storage.drain(depth - range.end..depth - range.start).rev().collect())
        }
    }

    pub fn drop_range_straight(&mut self, range: Range<usize>) -> ResultVec<StackItem> {
        if range.is_empty() {
            return Ok(vec!())
        }
        let depth = self.depth();
        if range.end > depth {
            err!(ExceptionCode::StackUnderflow, "drop_range: {}..{}, depth: {}", range.start, range.end, depth)
        } else if range.end == depth {
            let mut rem = Vec::from(&self.storage[depth - range.start..]);
            self.storage.truncate(depth - range.start);
            std::mem::swap(&mut rem, &mut self.storage);
            Ok(rem)
        } else {
            Ok(self.storage.drain(depth - range.end..depth - range.start).collect())
        }
    }

    pub fn append(&mut self, other: &mut Vec<StackItem>) {
        self.storage.append(other)
    }

    pub fn get(&self, i: usize) -> &StackItem {
        &self.storage[self.depth() - i - 1]
    }

    pub fn get_mut(&mut self, i: usize) -> &mut StackItem {
        let depth = self.depth();
        &mut self.storage[depth - i - 1]
    }

    pub fn insert(&mut self, i: usize, item: StackItem) -> &mut Stack {
        let depth = self.depth();
        self.storage.insert(depth - i, item);
        self
    }
    /// pushes a new var to stack
    pub fn push(&mut self, item: StackItem) -> &mut Stack {
        self.storage.push(item);
        self
    }
    /// pushes a builder as new var to stack
    pub fn push_builder(&mut self, item: BuilderData) -> &mut Stack {
        self.storage.push(StackItem::Builder(Arc::new(item)));
        self
    }
    /// pushes a continuation as new var to stack
    pub fn push_cont(&mut self, item: ContinuationData) -> &mut Stack {
        self.storage.push(StackItem::Continuation(Arc::new(item)));
        self
    }
    /// pushes a vector as tuple
    pub fn push_tuple(&mut self, items: Vec<StackItem>) -> &mut Stack {
        self.storage.push(StackItem::tuple(items));
        self
    }

    // Reverses order of (j...j+i-1)
    pub fn reverse_range(&mut self, range: Range<usize>) -> Status {
        let depth = self.depth();
        if range.end > depth {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let length = range.end - range.start;
            for i in 0..length/2 {
                self.storage.swap(depth - range.start - i - 1, depth - range.end + i);
            }
            Ok(())
        }
    }

    /// pushes a copy of the stack var to stack
    pub fn push_copy(&mut self, index: usize) -> Status {
        let depth = self.depth();
        if index >= depth {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let item = self.storage[depth - 1 - index].clone();
            self.storage.push(item);
            Ok(())
        }
    }

    /// swaps two values inside the stack
    pub fn swap(&mut self, i: usize, j: usize) -> Status {
        let depth = self.depth();
        if (i >= depth) || (j >= depth) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            self.storage.swap(depth - i - 1, depth - j - 1);
            Ok(())
        }
    }

    fn eq_builder(x: &BuilderData, y: &StackItem) -> bool {
        match y {
            StackItem::Builder(y) => x.eq(y),
            _ => false,
        }
    }

    fn eq_cell(x: &Cell, y: &StackItem) -> bool {
        match y {
            StackItem::Cell(y) => x.eq(y),
            _ => false,
        }
    }

    fn eq_continuation(x: &ContinuationData, y: &StackItem) -> bool {
        match y {
            StackItem::Continuation(y) => x.eq(y),
            _ => false,
        }
    }

    fn eq_integer(x: &IntegerData, y: &StackItem) -> bool {
        match y {
            StackItem::Integer(y) => x.eq(y),
            _ => false,
        }
    }

    fn eq_slice(x: &SliceData, y: &StackItem) -> bool {
        match y {
            StackItem::Slice(y) => x.eq(y),
            _ => false,
        }
    }

    fn eq_tuple(x: &[StackItem], y: &StackItem) -> bool {
        match y {
            StackItem::Tuple(y) => {
                let len = x.len();
                if len != y.len() {
                    return false
                }
                for i in 0..len {
                    if !Stack::eq_item(&x[i], &y[i]) {
                        return false
                    }
                }
                true
            }
            _ => false,
        }
    }

    #[rustfmt::skip]
    pub fn eq_item(x: &StackItem, y: &StackItem) -> bool {
        match x {
            StackItem::Builder(x)      => Stack::eq_builder(x, y),
            StackItem::Cell(x)         => Stack::eq_cell(x, y),
            StackItem::Continuation(x) => Stack::eq_continuation(x, y),
            StackItem::Integer(x)      => Stack::eq_integer(x, y),
            StackItem::Slice(x)        => Stack::eq_slice(x, y),
            StackItem::Tuple(x)        => Stack::eq_tuple(x, y),
            StackItem::None            => y == &StackItem::None,
        }
    }

    pub fn iter(&self) -> Iter<StackItem> {
        self.storage.iter()
    }

}

impl PartialEq for Stack {
    fn eq(&self, stack: &Stack) -> bool {
        if self.depth() != stack.depth() {
            return false;
        }
        for i in 0..self.depth() {
            if !Stack::eq_item(self.get(i), stack.get(i)) {
                return false;
            }
        }
        true
    }
}

impl fmt::Display for Stack {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.storage.iter().fold(String::new(), |acc, item| format!("{}{}\n", acc, item)))
    }
}

