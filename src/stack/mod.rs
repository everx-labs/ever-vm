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
    error::TvmError, stack::{continuation::ContinuationData, integer::IntegerData},
    types::{Exception, ResultMut, ResultOpt, ResultRef, ResultVec, Status}
};
use std::{fmt, mem, ops::Range, slice::Iter, sync::Arc};
use ton_types::{BuilderData, Cell, CellType, error, SliceData, Result, types::ExceptionCode};

pub mod serialization;
pub mod savelist;
pub mod continuation;
#[macro_use]
pub mod integer;

#[macro_export]
macro_rules! int {
    (nan) => {
        StackItem::Integer(Arc::new(IntegerData::nan()));
    };
    ($value: expr) => {
        StackItem::Integer(Arc::new(IntegerData::from($value).unwrap()))
    };
    (parse $str: expr) => {
        StackItem::Integer(Arc::new(IntegerData::from_str($str).unwrap()))
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

#[macro_export]
macro_rules! dict {
    (empty) => {
        StackItem::None
    };
    ($dict: ident) => {{
        match $dict.data() {
            Some(cell) => StackItem::Cell(cell.clone()),
            None => StackItem::None
        }
    }};
}

#[derive(Debug, PartialEq)]
pub enum StackItem {
    None,
    Builder(Arc<BuilderData>),
    Cell(Cell),
    Continuation(Arc<ContinuationData>),
    Integer(Arc<IntegerData>),
    Slice(SliceData),
    Tuple(Vec<StackItem>)
}

impl StackItem {

    pub const fn default() -> Self {
        StackItem::None
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
            &StackItem::Builder(ref data) => Ok(data),
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
            &StackItem::Cell(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_continuation(&self) -> ResultRef<ContinuationData> {
        match self {
            StackItem::Continuation(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError) 
        }
    }

    /// Returns continuation for modify in place
    pub fn as_continuation_mut<'a>(&'a mut self) -> ResultMut<ContinuationData> {
        let unref = if let StackItem::Continuation(ref mut r) = self {
            if Arc::strong_count(r) + Arc::weak_count(r) > 1 {
                StackItem::Continuation(Arc::new(Arc::as_ref(r).clone()))
            } else {
                StackItem::None
            }
        } else {
            return err!(ExceptionCode::TypeCheckError)
        };
        if unref != StackItem::None {
            *self = unref;
        }
        if let StackItem::Continuation(ref mut r) = self {
            Ok(Arc::get_mut(r).ok_or(ExceptionCode::FatalError)?)
        } else {
            err!(ExceptionCode::TypeCheckError)
        }
    }

    /// Returns type D None or Cell
    pub fn as_dict(&self) -> ResultOpt<&Cell> {
        match self {
            &StackItem::None => Ok(None),
            &StackItem::Cell(ref data) => Ok(Some(data)),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_integer(&self) -> ResultRef<IntegerData> {
        match self {
            StackItem::Integer(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    pub fn as_integer_mut(&mut self) -> Result<IntegerData> {
        self.as_integer()?;
        match self.withdraw() {
            StackItem::Integer(ref mut data) =>
                Ok(mem::replace(Arc::make_mut(data), IntegerData::zero())),
            _ => unreachable!("already checked")
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
            StackItem::Tuple(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError)
        }
    }

    /// Returns integer as grams and checks range 0..2^120
    pub fn as_grams(&self) -> Result<u128> {
        self.as_integer()?.into(0..=(1u128<<120)-1)
    }

    pub fn is_null(&self) -> bool {
        Stack::eq_item(self, &StackItem::None)
    }

    pub fn is_slice(&self) -> bool {
        match self {
            StackItem::Slice(_) => true,
            _ => false
        }
    }

    pub fn withdraw(&mut self) -> StackItem {
        mem::replace(self, StackItem::None)
    }

    pub fn dump_as_fift(&self) -> String {
        match self {
            StackItem::None => "(null)".to_string(),
            StackItem::Integer(data) => data.clone().to_string(),
            StackItem::Cell(data) => format!("C{{{:X}}}", data.repr_hash()),
            StackItem::Continuation(_data) => "???".to_string(),
            StackItem::Builder(data) => {
                let bits = data.length_in_bits();
                let mut bytes = vec![];
                bytes.push(data.references_used() as u8);
                let mut l = 2 * (bits / 8) as u8;
                let tag = if bits & 7 != 0 {
                    l += 1;
                    0x80 >> (bits & 7)
                } else {
                    0
                };
                bytes.push(l);
                bytes.extend_from_slice(&data.data());
                bytes.last_mut().map(|x| *x |= tag);
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
                bytes.extend_from_slice(&data.storage());
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
}

impl Default for StackItem {
    fn default() -> StackItem {
        StackItem::None
    }
}

#[cfg_attr(rustfmt, rustfmt_skip)]
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

#[cfg_attr(rustfmt, rustfmt_skip)]
impl fmt::Display for StackItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StackItem::None            => write!(f, "Null"),
            StackItem::Builder(x)      => write!(f, "Builder {}", Arc::as_ref(&x)),
            StackItem::Cell(x)         => write!(f, "Cell x{:x} x{:x}", x.repr_hash(), x),
            StackItem::Continuation(x) => write!(f, "Continuation x{:x}", x.code().cell().repr_hash()),
            StackItem::Integer(x)      => write!(f, "{}", Arc::as_ref(&x)),
            StackItem::Slice(x)        => write!(f, "Slice x{}", x.to_hex_string()),
            StackItem::Tuple(x)        => write!(f, "Tuple ({})", x.iter().map(|v| format!("{}", v)).collect::<Vec<_>>().join(", ")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stack {
    storage: Vec<StackItem>,
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
        if self.depth() <= j + i - 1 {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let mut block = self.drop_range(j..j + i)?;
            loop { 
                match block.pop() { 
                    Some(x) => self.push(x),
                    None => break
                };
            }
            Ok(())
        }
    }

    pub fn depth(&self) -> usize {
        self.storage.len()
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
        let depth = self.depth();
        if range.end > depth {
            err!(ExceptionCode::StackUnderflow, "drop_range: {}..{}, depth: {}", range.start, range.end, depth)
        } else {
            Ok(self.storage.drain(depth - range.end..depth - range.start).rev().collect())
        }
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
        self.storage.push(StackItem::Tuple(items));
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

    fn eq_tuple(x: &Vec<StackItem>, y: &StackItem) -> bool {
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

    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn eq_item(x: &StackItem, y: &StackItem) -> bool {
        match x {
            StackItem::Builder(x)      => Stack::eq_builder(&x, y),
            StackItem::Cell(x)         => Stack::eq_cell(&x, y),
            StackItem::Continuation(x) => Stack::eq_continuation(&x, y),
            StackItem::Integer(x)      => Stack::eq_integer(&x, y),
            StackItem::Slice(x)        => Stack::eq_slice(&x, y),
            StackItem::Tuple(x)        => Stack::eq_tuple(&x, y),
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
        return true;
    }
}

impl fmt::Display for Stack {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.storage.iter().fold(String::new(), |acc, item| format!("{}{}\n", acc, item)))
    }
}

