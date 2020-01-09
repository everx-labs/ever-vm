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


macro_rules! declare {
    ($mnemonic:ident, $value:expr) => {
        pub(super) const $mnemonic: u16 = $value;
    };
}

// Microfunction stuff ********************************************************

// How to address TVM objects using macros:
//
// CC                   - engine.cc
// ctrl!(i)             - c(i)
// savelist!(x, i)      - x.savelist(i), x is addressed independently 
//                        and supposed to be a continuation
// stack!(i)            - cc.stack(i)
// var!(i)              - engine.current_command.vars(i)

#[macro_export]
macro_rules! address_tag {
    ($code:expr) => {
        $code & 0x0F00
    };
}

#[macro_export]
macro_rules! ctrl {
    ($index:expr) => {
        CTRL | ($index as u16)
    };
}

#[macro_export]
macro_rules! savelist {
    ($storage:expr, $index:expr) => {
        $storage | SAVELIST | (($index as u16) << 12)
    };
}

#[macro_export]
macro_rules! savelist_index {
    ($code:expr) => {
        (($code & 0xF000) >> 12) as usize
    };
}

#[macro_export]
macro_rules! storage_index {
    ($code:expr) => {
        ($code & 0x000F) as usize
    };
}

#[macro_export]
macro_rules! stack {
    ($index:expr) => {
        STACK | (($index & 0xFF) as u16) 
    };
}

#[macro_export]
macro_rules! stack_index {
    ($code:expr) => {
        ($code & 0x00FF) as usize
    };
}

#[macro_export]
macro_rules! var {
    ($index:expr) => {
        VAR | ($index as u16)
    };
}

// Address tags
declare!(CC,           0x0000); // Current continuation
declare!(CTRL,         0x0100); // Control register
declare!(STACK,        0x0200); // Data stack 
declare!(VAR,          0x0300); // Instruction variable
declare!(SAVELIST,     0x0800); // Savelist

// Data tags
declare!(BUILDER,      0x0000);
declare!(CELL,         0x0001);
declare!(CONTINUATION, 0x0002);
//declare!(INTEGER,      0x0003);
declare!(SLICE,        0x0004);

pub(super) const CC_SAVELIST: u16 = CC | SAVELIST;
pub(super) const CTRL_SAVELIST: u16 = CTRL | SAVELIST;
pub(super) const VAR_SAVELIST: u16 = VAR | SAVELIST;
