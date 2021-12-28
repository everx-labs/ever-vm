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
    executor::math::DivMode,
    stack::{StackItem, integer::IntegerData}
};
use std::{fmt, ops::Range};
use ton_types::{error, Result, SliceData};

#[derive(Debug)]
pub(super) struct Context {
    pub(super) params: Vec<InstructionParameter>
}

macro_rules! param {
    ($self:ident, $id:ident) => {{
         for p in &$self.params {
            if let InstructionParameter::$id(x) = p {
                return Some(*x)
            }
        }
        None
    }};
}

macro_rules! param_ref {
    ($self:ident, $id:ident) => {{
         for p in &$self.params {
            if let InstructionParameter::$id(ref x) = p {
                return Some(x)
            }
        }
        None
    }};
}

macro_rules! param_ref_mut {
    ($self:ident, $id:ident) => {{
         for p in &mut $self.params {
            if let InstructionParameter::$id(ref mut x) = p {
                return Some(x)
            }
        }
        None
    }};
}

impl Context {
    pub(super) fn creg(&self) -> Option<usize> {
        param!(self, ControlRegister)
    }
    pub(super) fn division_mode(&self) -> Option<&DivMode> {
        param_ref!(self, DivisionMode)
    }
    pub(super) fn integer(&self) -> Option<isize> {
        param!(self, Integer)
    }
    #[allow(dead_code)]
    pub(super) fn biginteger(&self) -> Option<&IntegerData> {
        param_ref!(self, BigInteger)
    }
    pub(super) fn biginteger_mut(&mut self) -> Option<&mut IntegerData> {
        param_ref_mut!(self, BigInteger)
    }
    pub(super) fn length(&self) -> Option<usize> {
        param!(self, Length)
    }
    pub(super) fn nargs(&self) -> Option<isize> {
        param!(self, Nargs)
    }
    pub(super) fn pargs(&self) -> Option<usize> {
        param!(self, Pargs)
    }
    pub(super) fn rargs(&self) -> Option<usize> {
        param!(self, Rargs)
    }
    pub(super) fn slice(&self) -> Option<&SliceData> {
        param_ref!(self, Slice)
    }
    pub(super) fn sreg(&self) -> Option<usize> {
        param!(self, StackRegister)
    }
    pub(super) fn sregs(&self) -> Option<&RegisterPair> {
        param_ref!(self, StackRegisterPair)
    }
    pub(super) fn sregs3(&self) -> Option<&RegisterTrio> {
        param_ref!(self, StackRegisterTrio)
    }
    pub(super) fn length_and_index(&self) -> Option<&LengthAndIndex> {
        param_ref!(self, LengthAndIndex)
    }
    pub(super) fn clear(&mut self) {
        self.params.clear()
    }
}

pub(super) enum InstructionOptions {              // What will be set:
    ArgumentConstraints,                          // Nargs, Pargs
    ArgumentAndReturnConstraints,                 // Pargs, Rargs
    BigInteger,                                   // BigInteger
    Bytestring(usize, usize, usize, usize),       // byte aligned SliceData from code
    ControlRegister,                              // ControlRegister
    Dictionary(usize, usize),                     // SliceData with dictionary and Integer for index
    DivisionMode,                                 // DivisionMode
    Integer(Range<isize>),                        // Integer
    Length(Range<usize>),                         // Length
    LengthAndIndex,                               // LengthAndIndex
    LengthMinusOne(Range<usize>),                 // Length
    LengthMinusOneAndIndexMinusOne,               // LengthAndIndex
    LengthMinusTwoAndIndex,                       // LengthAndIndex
    Pargs(Range<usize>),                          // Pargs
    Rargs(Range<usize>),                          // Rargs
    Bitstring(usize, usize, usize, usize),        // SliceData from code
    StackRegister(Range<usize>),                  // StackRegister
    StackRegisterPair(WhereToGetParams),          // StackRegisterPair
    StackRegisterTrio(WhereToGetParams),          // StackRegisterTrio
}

#[derive(Debug, PartialEq)]
pub(super) enum WhereToGetParams {
    GetFromLastByte2Bits,
    GetFromLastByte,
    GetFromNextByte,
    GetFromNextByteLong,
    GetFromNextByteMinusOne,
    GetFromNextByteMinusOneMinusOne,
    GetFromNextByteMinusOneMinusTwo,
}

#[derive(Debug)]
pub(super) enum InstructionParameter {
    BigInteger(IntegerData),
    ControlRegister(usize),
    DivisionMode(DivMode),
    Integer(isize),
    Length(usize),
    LengthAndIndex(LengthAndIndex),
    Nargs(isize),
    Pargs(usize),
    Rargs(usize),
    Slice(SliceData),
    StackRegister(usize),
    StackRegisterPair(RegisterPair),
    StackRegisterTrio(RegisterTrio),
}

#[derive(Debug)]
pub(super) struct RegisterPair {
    pub(super) ra: usize,
    pub(super) rb: usize
}

#[derive(Debug)]
pub(super) struct RegisterTrio {
    pub(super) ra: usize,
    pub(super) rb: usize,
    pub(super) rc: usize
}

#[derive(Debug)]
pub(super) struct LengthAndIndex {
    pub(super) length: usize,
    pub(super) index: usize
}

pub(super) struct Instruction {
    /// Instruction mnemonic
    pub(super) name: &'static str,
    pub(super) name_prefix: Option<&'static str>,
    /// Options
    pub(super) opts: Option<InstructionOptions>,
    /// Instruction context
    pub(super) ictx: Context,
    /// Variables
    pub(super) vars: Vec<StackItem>,
}

impl Instruction {
    pub(super) fn new(name: &'static str) -> Instruction {
        Instruction {
            name,
            name_prefix: None,
            opts: None,
            ictx: Context {
                params: Vec::new()
            },
            vars: Vec::new(),
        }
    }
    fn name(&self) -> String {
        match self.name_prefix {
            Some(prefix) => prefix.to_string() + self.name,
            None => self.name.to_string()
        }
    }
    pub(super) fn set_name_prefix(mut self, prefix: Option<&'static str>) -> Instruction {
        self.name_prefix = prefix;
        self
    }
    pub(super) fn set_opts(mut self, opts: InstructionOptions) -> Instruction {
        self.opts = Some(opts);
        self
    }
    pub(super) fn creg(&self) -> usize {
        self.ictx.creg().unwrap()
    }
    pub(super) fn biginteger_mut(&mut self) -> &mut IntegerData {
         self.ictx.biginteger_mut().unwrap()
    }
    pub(super) fn division_mode(&self) -> &DivMode {
        self.ictx.division_mode().unwrap()
    }
    pub(super) fn has_length(&self) -> bool {
        self.ictx.length().is_some()
    }
    pub(super) fn integer(&self) -> isize {
        self.ictx.integer().unwrap()
    }
    pub(super) fn length(&self) -> usize {
        self.ictx.length().unwrap()
    }
    pub(super) fn nargs(&self) -> isize {
        self.ictx.nargs().unwrap_or(-1)
    }
    pub(super) fn pargs(&self) -> usize {
        self.ictx.pargs().unwrap_or(0)
    }
    pub(super) fn push_var(&mut self, var: StackItem) {
        self.vars.push(var)
    }
    pub(super) fn rargs(&self) -> usize {
        self.ictx.rargs().unwrap_or(0)
    }
    pub(super) fn slice(&self) -> &SliceData {
        self.ictx.slice().unwrap()
    }
    pub(super) fn sreg(&self) -> usize {
        self.ictx.sreg().unwrap()
    }
    pub(super) fn sregs(&self) -> &RegisterPair {
        self.ictx.sregs().unwrap()
    }
    pub(super) fn sregs3(&self) -> &RegisterTrio {
        self.ictx.sregs3().unwrap()
    }
    pub(super) fn length_and_index(&self) -> &LengthAndIndex {
        self.ictx.length_and_index().unwrap()
    }
    pub(super) fn var(&self, index: usize) -> &StackItem {
        self.vars.get(index).unwrap()
    }
    pub(super) fn var_count(&self) -> usize {
        self.vars.len()
    }
    pub(super) fn var_mut(&mut self, index: usize) -> &mut StackItem {
        self.vars.get_mut(index).unwrap()
    }
    pub(super) fn last_var(&self) -> Result<&StackItem> {
        self.vars.last().ok_or_else(|| error!("no vars for {}", self.name()))
    }
    pub(super) fn pop_var(&mut self) -> Result<StackItem> {
        self.vars.pop().ok_or_else(|| error!("no vars for {}", self.name()))
    }
    #[allow(dead_code)]
    pub(super) fn dump_with_params(&self) -> Option<String> {
        let mut trace = String::new();
        if let Some(prefix) = self.name_prefix {
            trace += prefix;
        }
        trace += self.name;
        trace += &match self.opts {
            Some(InstructionOptions::ArgumentAndReturnConstraints) =>
                format!(" {}, {}",
                    self.ictx.pargs()?,
                    self.ictx.rargs()?
                ),
            Some(InstructionOptions::ArgumentConstraints) =>
                format!(" {}, {}",
                    self.ictx.pargs()?,
                    self.ictx.nargs()?
                ),
            Some(InstructionOptions::BigInteger) =>
                format!(" {}", self.ictx.biginteger()?), // TODO: it is zero because execution withdraws it
            Some(InstructionOptions::ControlRegister) =>
                format!(" c{}", self.ictx.creg()?),
            Some(InstructionOptions::DivisionMode) => {
                let mode = self.division_mode();
                if mode.shift_parameter() {
                    format!(" {}", self.length())
                } else {
                    String::new()
                }
            },
            Some(InstructionOptions::Integer(_)) =>
                format!(" {}", self.ictx.integer()?),
            Some(InstructionOptions::Length(_)) |
            Some(InstructionOptions::LengthMinusOne(_)) =>
                format!(" {}", self.ictx.length()?),
            Some(InstructionOptions::LengthAndIndex) |
            Some(InstructionOptions::LengthMinusOneAndIndexMinusOne) |
            Some(InstructionOptions::LengthMinusTwoAndIndex) => {
                let length_and_index = self.ictx.length_and_index()?;
                format!(" {}, {}", length_and_index.length, length_and_index.index)
            },
            Some(InstructionOptions::Pargs(_)) =>
                format!(" {}", self.ictx.pargs()?),
            Some(InstructionOptions::Rargs(_)) =>
                format!(" {}", self.ictx.rargs()?),
            Some(InstructionOptions::StackRegister(_)) =>
                format!(" s{}", self.ictx.sreg()?),
            Some(InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByteMinusOne)) =>
                format!(" s{},s{}",
                    self.ictx.sregs()?.ra,
                    self.ictx.sregs()?.rb as isize - 1
                ),
            Some(InstructionOptions::StackRegisterPair(_)) =>
                format!(" s{},s{}",
                    self.ictx.sregs()?.ra,
                    self.ictx.sregs()?.rb
                ),
            Some(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOne)) =>
                format!(" s{},s{},s{}",
                    self.ictx.sregs3()?.ra,
                    self.ictx.sregs3()?.rb,
                    self.ictx.sregs3()?.rc as isize - 1,
                ),
            Some(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusOne)) =>
                format!(" s{},s{},s{}",
                    self.ictx.sregs3()?.ra,
                    self.ictx.sregs3()?.rb as isize - 1,
                    self.ictx.sregs3()?.rc as isize - 1,
                ),
            Some(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusTwo)) =>
                format!(" s{},s{},s{}",
                    self.ictx.sregs3()?.ra,
                    self.ictx.sregs3()?.rb as isize - 1,
                    self.ictx.sregs3()?.rc as isize - 2,
                ),
            Some(InstructionOptions::StackRegisterTrio(_)) =>
                format!(" s{},s{},s{}",
                    self.ictx.sregs3()?.ra,
                    self.ictx.sregs3()?.rb,
                    self.ictx.sregs3()?.rc,
                ),
            Some(InstructionOptions::Bitstring(_, _, _, _)) =>
                format!(" x{:X}", self.ictx.slice()?),
            Some(InstructionOptions::Bytestring(_, _, _, _)) =>
                format!(" x{:X}", self.ictx.slice()?),
            Some(InstructionOptions::Dictionary(_, _)) =>
                format!(" {}", self.ictx.length()?),
            None => String::new()
        };
        Some(trace)
    }
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}
