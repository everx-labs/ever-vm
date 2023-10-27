/*
* Copyright (C) 2019-2023 TON Labs. All Rights Reserved.
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
    types::{Exception, ResultMut, ResultOpt, ResultRef, ResultVec, Status},
};
use self::{savelist::SaveList, continuation::ContinuationData, integer::IntegerData};
use std::{fmt, mem, ops::Range, slice::Iter, sync::Arc, cmp::Ordering};
use integer::serialization::{Encoding, SignedIntegerBigEndianEncoding};
use serialization::Deserializer;
use ton_types::{
    MAX_DATA_BITS, MAX_REFERENCES_COUNT,
    error,
    BuilderData, Cell, CellType, ExceptionCode, HashmapType, IBitstring,
    Result, SliceData, GasConsumer, HashmapE
};

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
        StackItem::integer(IntegerData::from($value).unwrap())
    };
    (parse $str: expr) => {
        StackItem::integer(std::str::FromStr::from_str($str).unwrap())
    };
    (parse_hex $str: expr) => {
        StackItem::integer(IntegerData::from_str_radix($str, 16).unwrap())
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

#[derive(Clone, Debug, Default, PartialEq)]
pub enum StackItem {
    #[default]
    None,
    Builder(Arc<BuilderData>),
    Cell(Cell),
    Continuation(Arc<ContinuationData>),
    Integer(Arc<IntegerData>),
    Slice(SliceData),
    Tuple(Arc<Vec<StackItem>>)
}

impl Drop for StackItem {
    fn drop(&mut self) {
        if self.is_tuple() || self.is_continuation() {
            let mut stack = vec!();
            collect_items(&mut stack, self);
            while let Some(ref mut item) = stack.pop() {
                collect_items(&mut stack, item);
            }
        }
    }
}

fn collect_items(stack: &mut Vec<StackItem>, item: &mut StackItem) {
    match item {
        StackItem::Tuple(data) => {
            match Arc::try_unwrap(std::mem::take(data)) {
                Ok(ref mut tuple) => {
                    for item in tuple {
                        stack.push(std::mem::take(item))
                    }
                }
                Err(data) => drop(data)
            }
        }
        StackItem::Continuation(data) => {
            match Arc::try_unwrap(std::mem::take(data)) {
                Ok(ref mut cont) => {
                    for creg in [0, 1, 2, 3, 7] {
                        if let Some(item) = cont.savelist.get_mut(creg) {
                            stack.push(std::mem::take(item))
                        }
                    }
                    for item in &mut cont.stack.storage {
                        stack.push(std::mem::take(item))
                    }
                }
                Err(data) => drop(data)
            }
        }
        _ => ()
    }
}

pub(crate) enum SerializeItem<'a> {
    Item(&'a StackItem),
    Tuple(BuilderData),
    Cont(BuilderData, &'a ContinuationData, bool),
    SaveList,
    SaveListItem(usize),
}

pub fn slice_serialize(slice: &SliceData) -> Result<BuilderData> {
    let mut builder = BuilderData::new();
    let cell = match slice.cell_opt() {
        Some(cell) => cell.clone(),
        None => slice.as_builder().into_cell()?
    };
    builder.checked_append_reference(cell)?;
    builder.append_bits(slice.pos(), 10)?;
    builder.append_bits(slice.pos() + slice.remaining_bits(), 10)?;
    builder.append_bits(slice.get_references().start, 3)?;
    builder.append_bits(slice.get_references().end, 3)?;
    Ok(builder)
}

pub fn slice_deserialize(slice: &mut SliceData) -> Result<SliceData> {
    let cell = slice.checked_drain_reference()?;
    let data_start = slice.get_next_int(10)? as usize;
    let data_end = slice.get_next_int(10)? as usize;
    if data_start > MAX_DATA_BITS || data_end > MAX_DATA_BITS || data_start > data_end {
        return err!(ExceptionCode::FatalError, "slice deserialize error data: {}..{}", data_start, data_end)
    }
    let ref_start = slice.get_next_int(3)? as usize;
    let ref_end = slice.get_next_int(3)? as usize;
    if ref_start > MAX_REFERENCES_COUNT || ref_end > MAX_REFERENCES_COUNT || ref_start > ref_end {
        return err!(ExceptionCode::FatalError, "slice deserialize error refs: {}..{}", ref_start, ref_end)
    }
    let mut res = SliceData::load_cell(cell)?;
    res.shrink_data(data_start..data_end);
    res.shrink_references(ref_start..ref_end);
    Ok(res)
}

fn items_serialize(mut items: Vec<SerializeItem>, gas_consumer: &mut dyn GasConsumer) -> Result<BuilderData> {
    let mut list = Some(BuilderData::default());
    let mut list_stack = Vec::new();
    let mut savelist = None;
    let mut savelist_stack = Vec::new();
    while let Some(item) = items.pop() {
        let mut builder = match item {
            SerializeItem::Item(item) => {
                if let Some(cons) = item.serialize_internal(&mut items, gas_consumer)? {
                    cons
                } else {
                    if let Some(cell) = list.replace(BuilderData::default()) {
                        list_stack.push(cell);
                    }
                    continue
                }
            }
            SerializeItem::Tuple(mut header) => {
                if let Some(cell) = list {
                    let cell = gas_consumer.finalize_cell(cell)?;
                    header.checked_append_reference(cell)?;
                }
                list = list_stack.pop();
                header
            }
            SerializeItem::Cont(mut header, cont, append_cell) => {
                let cons = cont.serialize_internal(
                    list.unwrap_or_default(),
                    savelist.unwrap_or_default(),
                    gas_consumer
                )?;
                if append_cell {
                    let cell = gas_consumer.finalize_cell(cons)?;
                    header.checked_append_reference(cell)?;
                } else {
                    header.append_builder(&cons)?;
                }
                list = list_stack.pop();
                savelist = savelist_stack.pop();
                header
            }
            SerializeItem::SaveListItem(index) => {
                if let Some(value) = list.replace(BuilderData::default()) {
                    if let Some(savelist) = savelist.as_mut() {
                        let mut builder = BuilderData::new();
                        builder.append_bits(index, 4)?;
                        let key = SliceData::load_bitstring(builder)?;
                        savelist.set_builder(key, &value)?; // TODO: gas here?
                    } else {
                        panic!("savelist is None {}", index)
                    }
                } else {
                    panic!("list is None {}", index)
                }
                continue
            }
            SerializeItem::SaveList => {
                if let Some(savelist) = savelist.replace(HashmapE::with_bit_len(4)) {
                    savelist_stack.push(savelist);
                }
                continue
            }
        };
        if let Some(cell) = list {
            let cell = gas_consumer.finalize_cell(cell)?;
            builder.checked_append_reference(cell)?;
        }
        list = Some(builder);
    }
    debug_assert_eq!(0, list_stack.len());
    list.ok_or_else(|| exception!(ExceptionCode::FatalError, "nothing was serialized"))
}

fn prepare_savelist_serialize_vars<'a>(savelist: &'a SaveList, items: &mut Vec<SerializeItem<'a>>) {
    for index in 0..SaveList::NUMREGS {
        if let Some(item) = savelist.get(index) {
            items.push(SerializeItem::SaveListItem(if index == 6 { 7 } else { index }));
            items.push(SerializeItem::Item(item));
        }
    }
    items.push(SerializeItem::SaveList);
}

fn prepare_cont_serialize_vars<'a>(cont: &'a ContinuationData, header: BuilderData, items: &mut Vec<SerializeItem<'a>>, append_cell: bool) {
    items.push(SerializeItem::Cont(header, cont, append_cell));
    items.extend(cont.stack.iter().map(SerializeItem::Item));
    prepare_savelist_serialize_vars(&cont.savelist, items);
}

#[derive(Debug)]
pub(crate) enum DeserializeItem {
    Items(usize, SliceData),
    Tuple(usize),
    Cont(ContinuationData),
    SaveListItem(usize),
    SaveList,
}

fn items_deserialize(mut list: Vec<DeserializeItem>, gas_consumer: &mut dyn GasConsumer) -> Result<Vec<StackItem>> {
    let mut items_stack = Vec::new();
    let mut items = Vec::new();
    let mut length = 0;
    let mut slice = SliceData::default();
    let mut savelist_stack = Vec::new();

    loop {
        for _ in items.len()..length {
            let item = StackItem::deserialize_internal(&mut list, &mut slice, gas_consumer)?;
            if let Ok(cell) = slice.checked_drain_reference() {
                slice = gas_consumer.load_cell(cell)?
            }
            let Some(item) = item else { break };
            items.push(item);
        }
        let Some(list_item) = list.pop() else { return Ok(items) };
        let item = match list_item {
            DeserializeItem::Items(next_length, next_slice) => {
                items_stack.push((items, length, slice));
                items = Vec::new();
                slice = next_slice;
                length = next_length;
                continue
            }
            DeserializeItem::Tuple(new_length) => {
                if new_length != items.len() {
                    return err!(ExceptionCode::CellUnderflow, "tuple must be length {} but {}", new_length, items.len())
                }
                Some(StackItem::tuple(items))
            }
            DeserializeItem::Cont(mut continuation) => {
                continuation.stack = Stack::with_storage(items);
                debug_assert_ne!(0, savelist_stack.len());
                if let Some(savelist) = savelist_stack.pop() {
                    continuation.savelist = savelist;
                }
                Some(StackItem::continuation(continuation))
            }
            DeserializeItem::SaveListItem(index) => {
                if let Some(savelist) = savelist_stack.last_mut() {
                    debug_assert_eq!(1, items.len());
                    if let Some(item) = items.first_mut() {
                        savelist.put(index, item)?;
                    }
                }
                None
            }
            DeserializeItem::SaveList => {
                // start new savelist
                savelist_stack.push(SaveList::default());
                continue
            }
        };
        let (prev_items, prev_length, prev_slice) = items_stack.pop().unwrap_or_default();
        items = prev_items;
        slice = prev_slice;
        length = prev_length;
        if let Some(item) = item {
            items.push(item);
        }
    }
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
            true => StackItem::int(IntegerData::minus_one()),
            false => StackItem::int(IntegerData::zero()),
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
            StackItem::Integer(data) => {
                if data.is_nan() {
                    err!(ExceptionCode::IntegerOverflow)
                } else {
                    Ok(!data.is_zero())
                }
            }
            _ => err!(ExceptionCode::TypeCheckError, "item is not a bool")
        }
    }

    pub fn as_builder(&self) -> ResultRef<BuilderData> {
        match self {
            StackItem::Builder(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError, "item is not a builder")
        }
    }

    /// Extracts builder to modify, exceptions should not be after
    /// If is single reference it will not clone on write
    pub fn as_builder_mut(&mut self) -> Result<BuilderData> {
        match self {
            StackItem::Builder(data) => Ok(mem::take(Arc::make_mut(data))),
            _ => err!(ExceptionCode::TypeCheckError, "item is not a builder")
        }
    }

    pub fn as_cell(&self) -> ResultRef<Cell> {
        match self {
            StackItem::Cell(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError, "item is not a cell")
        }
    }

    pub fn as_continuation(&self) -> ResultRef<ContinuationData> {
        match self {
            StackItem::Continuation(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError, "item {} is not a continuation", self)
        }
    }

    pub fn as_continuation_mut(&mut self) -> ResultMut<ContinuationData> {
        match self {
            StackItem::Continuation(data) => Ok(Arc::make_mut(data)),
            _ => err!(ExceptionCode::TypeCheckError, "item {} is not a continuation", self)
        }
    }

    /// Returns type D None or Cell
    pub fn as_dict(&self) -> ResultOpt<&Cell> {
        match self {
            StackItem::None => Ok(None),
            StackItem::Cell(ref data) => Ok(Some(data)),
            _ => err!(ExceptionCode::TypeCheckError, "item is not a dictionary")
        }
    }

    pub fn as_integer(&self) -> ResultRef<IntegerData> {
        match self {
            StackItem::Integer(ref data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError, "item is not an integer")
        }
    }

    pub fn as_small_integer(&self) -> Result<usize> {
        self.as_integer()?.into(0..=255)
    }

    pub fn as_integer_mut(&mut self) -> ResultMut<IntegerData> {
        match self {
            StackItem::Integer(data) => Ok(Arc::make_mut(data)),
            _ => err!(ExceptionCode::TypeCheckError, "item is not an integer")
        }
    }

    pub fn as_slice(&self) -> ResultRef<SliceData> {
        match self {
            StackItem::Slice(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError, "item {} is not a slice", self)
        }
    }

    pub fn as_tuple(&self) -> ResultRef<[StackItem]> {
        match self {
            StackItem::Tuple(data) => Ok(data),
            _ => err!(ExceptionCode::TypeCheckError, "item {} is not a tuple", self)
        }
    }

    pub fn tuple_item(&self, index: usize, default: bool) -> Result<StackItem> {
        let tuple = self.as_tuple()?;
        match tuple.get(index) {
            Some(value) => Ok(value.clone()),
            None if default => Ok(StackItem::None),
            None => err!(ExceptionCode::RangeCheckError, "tuple index is {} but length is {}", index, tuple.len())
        }
    }

    pub fn tuple_item_ref(&self, index: usize) -> ResultRef<StackItem> {
        let tuple = self.as_tuple()?;
        match tuple.get(index) {
            Some(value) => Ok(value),
            None => err!(ExceptionCode::RangeCheckError, "tuple index is {} but length is {}", index, tuple.len())
        }
    }

    /// Extracts tuple to modify, exceptions should not be after
    /// If is single reference it will not clone on write
    pub fn as_tuple_mut(&mut self) -> ResultVec<StackItem> {
        match self {
            StackItem::Tuple(data) => Ok(mem::take(Arc::make_mut(data))),
            _ => err!(ExceptionCode::TypeCheckError, "item is not a tuple")
        }
    }

    // Extracts tuple items
    pub fn withdraw_tuple_part(&mut self, length: usize) -> ResultVec<StackItem> {
        match self {
            StackItem::Tuple(arc) => {
                match Arc::try_unwrap(mem::take(arc)) {
                    Ok(mut tuple) => {
                        tuple.truncate(length);
                        Ok(tuple)
                    }
                    Err(arc) => {
                        Ok(arc[0..length].to_vec())
                    }
                }
            }
            _ => err!(ExceptionCode::TypeCheckError, "item is not a tuple")
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

    pub fn is_tuple(&self) -> bool {
        matches!(self, StackItem::Tuple(_))
    }

    pub fn is_continuation(&self) -> bool {
        matches!(self, StackItem::Continuation(_))
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
                    refs_count + 8 * is_special + 32 * level_mask
                };
                let d2 = |bits : u32| {
                    let res = ((bits / 8) * 2) as u8;
                    if bits & 7 != 0 { res + 1 } else { res }
                };
                let start = data.pos();
                let end = start + data.remaining_bits();
                let refs = data.get_references();
                let cell = data.cell_opt().unwrap();
                let data = match SliceData::load_cell_ref(cell) {
                    Ok(data) => data,
                    Err(err) => return err.to_string()
                };
                let mut bytes = vec![];
                let is_special = cell.cell_type() != CellType::Ordinary;
                bytes.push(d1(cell.level_mask().mask(), cell.references_count() as u8, is_special as u8));
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

    #[allow(dead_code)] // reserved for future use
    pub(crate) fn serialize(&self, gas_consumer: &mut dyn GasConsumer) -> Result<BuilderData> {
        let items = vec!(SerializeItem::Item(self));
        items_serialize(items, gas_consumer)
    }

    fn serialize_internal<'a>(&'a self, items: &mut Vec<SerializeItem<'a>>, gas_consumer: &mut dyn GasConsumer) -> Result<Option<BuilderData>> {
        let mut builder = BuilderData::new();
        match self {
            StackItem::None => {
                builder.append_bits(0x00, 8)?;
            }
            StackItem::Integer(data) => {
                if data.is_nan() {
                    builder.append_bits(0x02ff, 16)?;
                } else {
                    builder.append_bits(0x02, 8)?;
                    builder.append_bits(0x00, 7)?;
                    builder.append_builder(&data.as_builder::<SignedIntegerBigEndianEncoding>(257)?)?;
                }
            }
            StackItem::Cell(data) => {
                builder.append_bits(0x03, 8)?;
                builder.checked_append_reference(data.clone())?;
            }
            StackItem::Continuation(data) => {
                builder.append_bits(0x06, 8)?;
                prepare_cont_serialize_vars(data, builder, items, true);
                return Ok(None)
            }
            StackItem::Builder(data) => {
                builder.append_bits(0x05, 8)?;
                let cell = gas_consumer.finalize_cell(data.as_ref().clone())?;
                builder.checked_append_reference(cell)?;
            }
            StackItem::Slice(data) => {
                builder.append_bits(0x04, 8)?;
                builder.append_builder(&slice_serialize(data)?)?;
            }
            StackItem::Tuple(data) => {
                builder.append_bits(0x07, 8)?;
                builder.append_bits(data.len(), 8)?;
                items.push(SerializeItem::Tuple(builder));
                items.extend(data.iter().map(SerializeItem::Item));
                return Ok(None)
            }
        }
        Ok(Some(builder))
    }

    pub fn deserialize(slice: SliceData, gas_consumer: &mut dyn GasConsumer) -> Result<StackItem> {
        let list = vec!(DeserializeItem::Items(1, slice));
        Ok(items_deserialize(list, gas_consumer)?.remove(0))
        // Ok(items_deserialize(1, slice, gas_consumer)?.remove(0))
    }

    fn deserialize_internal(list: &mut Vec<DeserializeItem>, slice: &mut SliceData, gas_consumer: &mut dyn GasConsumer) -> Result<Option<StackItem>> {
        let item = match slice.get_next_byte()? {
            0x00 => StackItem::None,
            0x02 => {
                match slice.get_next_int(7)? {
                    0x00 => {
                        let decoder = SignedIntegerBigEndianEncoding::new(257);
                        let value = slice.get_next_bits(257)?;
                        let value = decoder.deserialize(&value);
                        StackItem::integer(value)
                    }
                    0x7f => {
                        if slice.get_next_bit()? {
                            StackItem::nan()
                        } else {
                            return err!(ExceptionCode::UnknownError)?
                        }
                    }
                    _ => return err!(ExceptionCode::UnknownError)
                }
            }
            0x03 => StackItem::cell(slice.checked_drain_reference()?),
            0x04 => StackItem::slice(slice_deserialize(slice)?),
            0x05 => StackItem::builder(BuilderData::from_cell(&slice.checked_drain_reference()?)?),
            0x06 => {
                let mut slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
                ContinuationData::deserialize_internal(list, &mut slice, gas_consumer)?;
                return Ok(None)
            }
            0x07 => {
                let length = slice.get_next_int(8)? as usize;
                let tuple_slice = gas_consumer.load_cell(slice.checked_drain_reference()?)?;
                list.push(DeserializeItem::Tuple(length));
                list.push(DeserializeItem::Items(length, tuple_slice));
                return Ok(None)
            }
            typ => return err!(ExceptionCode::UnknownError, "unknown StackItem type {}", typ)
        };
        Ok(Some(item))
    }
}

impl StackItem {
    pub fn serialize_old(&self) -> Result<(BuilderData, i64)> {
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
                builder.checked_append_reference(data.clone())?;
            },
            StackItem::Continuation(data) => {
                builder.append_bits(0x06, 8)?;
                let (serialized, gas2) = data.serialize_old()?;
                gas += gas2;
                builder.append_builder(&serialized)?;
            },
            StackItem::Builder(data) => {
                builder.append_bits(0x05, 8)?;
                let cell = data.as_ref().clone().into_cell()?;
                builder.checked_append_reference(cell)?;
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
                    let (serialized, gas2) = item.serialize_old()?;
                    gas += gas2;
                    cons.append_builder(&serialized)?;
                    cons.checked_append_reference(tuple_list.into_cell()?)?;
                    gas += Gas::finalize_price();
                    tuple_list = cons;
                }
                tuple.append_builder(&tuple_list)?;
                builder.append_builder(&tuple)?;
            }
        }
        Ok((builder, gas))
    }

    pub fn deserialize_old(slice: &mut SliceData) -> Result<(StackItem, i64)> {
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
            0x05 => Ok((StackItem::builder(BuilderData::from_cell(&slice.checked_drain_reference()?)?), gas)),
            0x06 => {
                let (cont, gas2) = ContinuationData::deserialize_old(slice)?;
                gas += gas2;
                Ok((StackItem::continuation(cont), gas))
            },
            0x07 => {
                let mut tuple = vec![];
                let len = slice.get_next_int(8)? as usize;
                if len > 0 {
                    let (item, gas2) = StackItem::deserialize_old(slice)?;
                    tuple.push(item);
                    gas += gas2;
                }
                let mut cell = slice.checked_drain_reference()?;
                for _ in 1..len {
                    let mut slice = SliceData::load_cell(cell)?;
                    gas += Gas::load_cell_price(true);
                    let (item, gas2) = StackItem::deserialize_old(&mut slice)?;
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

#[rustfmt::skip]
impl fmt::Display for StackItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StackItem::None            => write!(f, "Null"),
            StackItem::Builder(x)      => write!(f, "Builder {}", Arc::as_ref(x)),
            StackItem::Cell(x)         => write!(f, "Cell x{:x} x{:x}", x.repr_hash(), x),
            StackItem::Continuation(x) => write!(f, "Continuation x{:x}", x.code().repr_hash()),
            StackItem::Integer(x)      => write!(f, "{}", Arc::as_ref(x)),
            StackItem::Slice(x)        => write!(f, "Slice x{:x}", x),
            StackItem::Tuple(x)        => {
                if f.alternate() {
                    write!(f, "Tuple ({})", x.len())
                } else {
                    write!(f, "Tuple ({})", x.len())?;
                    f.debug_list().entries(x.iter().map(|v| format!("{:#}", v))).finish()
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stack {
    pub storage: Vec<StackItem>,
}

impl Stack {

    pub const fn new() -> Self {
        Stack {
            storage: Vec::new(),
        }
    }

    pub const fn with_storage(storage: Vec<StackItem>) -> Self {
        Stack { storage }
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
        match range.end.cmp(&depth) {
            Ordering::Greater => {
                err!(ExceptionCode::StackUnderflow, "drop_range: {}..{}, depth: {}", range.start, range.end, depth)
            }
            Ordering::Equal => {
                let mut rem = Vec::from(&self.storage[depth - range.start..]);
                self.storage.truncate(depth - range.start);
                std::mem::swap(&mut rem, &mut self.storage);
                Ok(rem)
            }
            Ordering::Less => {
                Ok(self.storage.drain(depth - range.end..depth - range.start).collect())
            }
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
        self.storage.push(StackItem::builder(item));
        self
    }
    /// pushes a continuation as new var to stack
    pub fn push_cont(&mut self, item: ContinuationData) -> &mut Stack {
        self.storage.push(StackItem::continuation(item));
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

