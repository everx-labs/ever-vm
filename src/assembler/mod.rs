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

use crate::{error::tvm_exception, stack::integer::IntegerData};
use std::{collections::HashMap, marker::PhantomData, ops::{Range, RangeInclusive}};
use ton_types::{SliceData, types::ExceptionCode};

mod errors;
pub use errors::{
    CompileError, OperationError, ParameterError, Position, 
    ToOperationParameterError,
};

#[macro_use]
mod macros;
mod parse;
use parse::*;

mod writer;
use writer::{CodePage0, Writer};

// Basic types *****************************************************************
/// Operation Compilation result
type CompileResult = Result<(), OperationError>;
type CompileHandler<T> = fn(&mut Engine<T>, &Vec<&str>, destination:&mut T) -> CompileResult;

// CompileError::Operation handlers ***********************************************************
trait CommandBehaviourModifier {
    fn modify(code: Vec<u8>) -> Vec<u8>;
}

struct Signaling {}
struct Quiet {}

impl CommandBehaviourModifier for Signaling {
    fn modify(code: Vec<u8>) -> Vec<u8> { code }
}

impl CommandBehaviourModifier for Quiet {
    fn modify(code: Vec<u8>) -> Vec<u8> {
        let mut code = code;
        code.insert(0, 0xB7);
        code
    }
}

trait EnsureParametersCountInRange {
    fn assert_empty(&self) -> Result<(), OperationError>;
    fn assert_len(&self, _n: usize) -> Result<(), OperationError>;
    fn assert_len_in(&self, _r: RangeInclusive<usize>) -> Result<(), OperationError>;
}

impl<T> EnsureParametersCountInRange for Vec<T>{
    fn assert_empty(&self) -> Result<(), OperationError> {
        self.assert_len_in(0..=0)
    }

    fn assert_len(&self, n: usize) -> Result<(), OperationError> {
        self.assert_len_in(n..=n)
    }

    fn assert_len_in(&self, range: RangeInclusive<usize>) -> Result<(), OperationError> {
        if &self.len() < range.start() {
            Err(OperationError::MissingRequiredParameters)
        } else if &self.len() > range.end() {
            Err(OperationError::TooManyParameters)
        } else {
            Ok(())
        }
    }
}

fn compile_with_register<T: Writer>(
    register: &str,
    symbol: char,
    range: Range<isize>,
    code: &[u8],
    destination: &mut T
) -> CompileResult {
    let reg = parse_register(register, symbol, range).parameter("arg 0")? as u8;
    let mut ret = code.to_vec();
    ret[code.len() - 1] |= reg;
    destination.write_command(ret.as_slice())
}

fn compile_with_any_register<T: Writer>(
    register: &str,
    code_stack_short: &[u8],
    code_stack_long: &[u8],
    code_ctrls: &[u8],
    destination: &mut T
) -> CompileResult {
    compile_with_register(register, 'S', 0..16, code_stack_short, destination).or_else(
        |e| if let OperationError::Parameter(_, ParameterError::UnexpectedType) = e {
            compile_with_register(register, 'C', 0..16, code_ctrls, destination)
        } else if let OperationError::Parameter(_, ParameterError::OutOfRange) = e {
            compile_with_register(register, 'S', 16..256, code_stack_long, destination)
        } else {
            Err(e)
        }
    )
}

fn compile_call<T: Writer>(_engine: &mut Engine<T>,  par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(1)?;
    let number = parse_const_u14(par[0]).parameter("Number")?;
    if number < 256 {
        destination.write_command(&[0xF0, number as u8])
    } else if number < 16384 {
        let hi = 0x3F & ((number / 256) as u8);
        let lo = (number % 256) as u8;
        destination.write_command(&[0xF1, hi, lo])
    } else {
        Err(ParameterError::OutOfRange.parameter("Number"))
    }
}

fn compile_pop<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(1)?;
    compile_with_any_register(par[0], &[0x30], &[0x57, 0x00], &[0xED, 0x50], destination)
}

fn compile_push<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(1)?;
    compile_with_any_register(par[0],  &[0x20], &[0x56, 0x00], &[0xED, 0x40], destination)
}

fn compile_pushcont<T: Writer>(engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(1)?;
    let cont = engine
        .compile(par[0])
        .map_err(|e| OperationError::Nested(Box::new(e)))?
        .finalize();
    let refs = cont.remaining_references() as u8;
    if refs > 0 {
        destination.write_composite_command(
            &[0x8E as u8 | ((refs & 0x2) >> 1), (refs & 0x1) << 0x7], 
            cont
        )
    } else {
        let code = cont.get_bytestring(0);
        let n = code.len();
        if n <= 15 {
            let mut command = vec![0x90 | n as u8];
            command.extend(code);
            destination.write_command(command.as_slice())
        } else if n <= 125 {
            let mut command = vec![0x8E, n as u8];
            command.extend(code);
            destination.write_command(command.as_slice())
        } else if n <= 127 {
            //We cannot put command and code in one cell, because it will 
            //be more than 1023 bits: 127 bytes (pushcont data) + 2 bytes(opcode).
            //Write as r = 1 and xx = 0x00.
            destination.write_composite_command(&[0x8E, 0x80], cont)
        } else {
            log::error!(target: "compile", "Maybe cell longer than 1024 bit?");
            Err(OperationError::NotFitInSlice)
        }
    }
}

fn compile_callxargs<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(2)?;
    let pargs = parse_const_u4(par[0]).parameter("pargs")?;
    if par[1] == "-1" {
        destination.write_command(&[0xDB, pargs & 0x0F])
    } else {
        let rargs = parse_const_i4(par[1]).parameter("rargs")?;
        destination.write_command(&[0xDA, ((pargs & 0x0F) << 4) | (rargs & 0x0F)])
    }
}

struct Div<M: CommandBehaviourModifier> (PhantomData<M>);

macro_rules! div_variant {
    (@resolve $command:ident => $code: expr) => {
        impl<M: CommandBehaviourModifier> Div<M> {
            pub fn $command<T: Writer>(
                _engine: &mut Engine<T>,
                par: &Vec<&str>,
                destination: &mut T
            ) -> CompileResult {
                par.assert_len_in(0..=1)?;
                destination.write_command(
                    &M::modify({
                        if par.len() == 1 {
                            let v = $code | 0b00010000;
                            vec![0xA9, v, parse_const_u8_plus_one(par[0]).parameter("arg 0")?]
                        } else {
                            let v = $code & (!0b00010000);
                            vec![0xA9, v]
                        }
                    })
                )
            }
        }
    };

    ($($command: ident => $code:expr)*) => {
        $(
            div_variant!(@resolve $command => $code);
        )*
    };
}

div_variant!(
    lshiftdiv => 0b11010100
    lshiftdivc => 0b11010110
    lshiftdivr => 0b11000101
    lshiftdivmod => 0b11011100
    lshiftdivmodc => 0b11011110
    lshiftdivmodr => 0b11011101
    lshiftmod => 0b11011000
    lshiftmodc => 0b11011010
    lshiftmodr => 0b11011001
    modpow2 => 0b00111000
    modpow2c => 0b00111010
    modpow2r => 0b00111001
    mulmodpow2 => 0b10111000
    mulmodpow2c => 0b10111010
    mulmodpow2r => 0b10111001
    mulrshift => 0b10110100
    mulrshiftc => 0b10110110
    mulrshiftr => 0b10110101
    mulrshiftmod => 0b10111100
    mulrshiftmodc => 0b10111110
    mulrshiftmodr => 0b10111101
    rshiftc => 0b00110110
    rshiftr => 0b00110101
    rshiftmod => 0b00111100
    rshiftmodr => 0b00111101
    rshiftmodc => 0b00111110
);

impl<M: CommandBehaviourModifier> Div<M> {
    pub fn lshift<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
        par.assert_len_in(0..=1)?;
        destination.write_command(
            &M::modify({
                if par.len() == 1 {
                    vec![0xAA, parse_const_u8_plus_one(par[0]).parameter("arg 0")?]
                } else {
                    vec![0xAC]
                }
            })
        )
    }

    fn rshift<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
        par.assert_len_in(0..=1)?;
        let command = if par.len() == 1 {
            vec![0xAB, parse_const_u8_plus_one(par[0]).parameter("value")?]
        } else {
            vec![0xAD]
        };
        destination.write_command(&M::modify(command))
    }

}

fn compile_setcontargs<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len_in(1..=2)?;
    let rargs = parse_const_u4(par[0]).parameter("register")?;
    let nargs = if par.len() == 2 {
        parse_const_i4(par[1]).parameter("arg 1")?
    } else {
        0x0F
    };
    destination.write_command(&[0xEC, ((rargs & 0x0F) << 4) | (nargs & 0x0F)])
}

#[cfg_attr(rustfmt, rustfmt_skip)]
fn compile_pushint<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(1)?;
    let (sub_str, radix) = if par[0].len() > 2 && (par[0][0..2].eq("0x") || par[0][0..2].eq("0X")) {
        (par[0][2..].to_string(), 16)
    } else if par[0].len() > 3 && (par[0][0..3].eq("-0x") || par[0][0..3].eq("-0X")) {
        let mut sub_str = par[0].to_string();
        sub_str.replace_range(1..3, "");
        (sub_str, 16)
    } else {
        (par[0].to_string(), 10)
    };
    destination.write_command(match i32::from_str_radix(sub_str.as_str(), radix) {
        Ok(number @ -5..=10) =>
            Ok(vec![0x70 | ((number & 0x0F) as u8)]),
        Ok(number @ -128..=127) =>
            Ok(vec![0x80, (number & 0xFF) as u8]),
        Ok(number @ -32768..=32767) =>
            Ok(vec![0x81, ((number >> 8) & 0xFF) as u8, (number & 0xFF) as u8]),
        _ => {
            let int = match IntegerData::from_str_radix(sub_str.as_str(), radix) {
                Ok(value) => value,
                Err(err) => if let Ok(err) = tvm_exception(err) {
                    match err.code {
                        ExceptionCode::TypeCheckError => {
                            return Err(ParameterError::UnexpectedType.parameter("arg 0"));
                        }
                        ExceptionCode::IntegerOverflow => {
                            return Err(ParameterError::OutOfRange.parameter("arg 0"));
                        }
                        _ => unimplemented!()
                    }
                } else {
                    return Err(ParameterError::UnexpectedType.parameter("arg 0"));
                }
            };
            let mut int_bytes = int.to_big_endian_octet_string();
            let mut bytecode = vec![0x82];
            bytecode.append(&mut int_bytes);
            Ok(bytecode)
        },
    }?.as_slice())
} 

fn compile_bchkbits<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    destination.write_command({
        if par.len() == 1 {
            Ok(vec![0xCF, 0x38, parse_const_u8_plus_one(par[0]).parameter("value")?])
        } else {
            Ok(vec![0xCF, 0x39])
        }
    }?.as_slice())
}

fn compile_bchkbitsq<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    if par.len() == 1 {
        destination.write_command(
            vec![0xCF, 0x3C, parse_const_u8_plus_one(par[0]).parameter("value")?].as_slice()
        )
    } else {
        destination.write_command(&[0xCF, 0x3D])
    }
}

fn compile_dumpstr<T: Writer>(
    _engine: &mut Engine<T>,
    par: &Vec<&str>,
    destination: &mut T,
    mut buffer: Vec<u8>,
    max_len: usize
) -> CompileResult {
    par.assert_len(1)?;
    let string = par[0].as_bytes();
    let len = string.len();
    if len > max_len {
        return Err(ParameterError::OutOfRange.parameter(par[0]))
    }
    buffer[1] |= (len - 1 + 16 - max_len) as u8;
    buffer.extend_from_slice(string);
    destination.write_command(buffer.as_slice())
}

fn compile_dumptosfmt<T: Writer>(engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    compile_dumpstr::<T>(engine, par, destination, vec![0xFE, 0xF0], 16)
}

fn compile_logstr<T: Writer>(engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    compile_dumpstr::<T>(engine, par, destination, vec![0xFE, 0xF0, 0x00], 15)
}

fn compile_printstr<T: Writer>(engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    compile_dumpstr::<T>(engine, par, destination, vec![0xFE, 0xF0, 0x01], 15)
}

fn compile_stsliceconst<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T) -> CompileResult {
    par.assert_len(1)?;
    if par[0] == "0" {
        destination.write_command(&[0xCF, 0x81])
    } else if par[0] == "1" {
        destination.write_command(&[0xCF, 0x83])
    } else {
        let buffer = compile_slice(par[0], vec![0xCF, 0x80], 9, 2, 3).parameter("arg 0")?;
        destination.write_command(buffer.as_slice())
    }
}

fn compile_pushslice<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    par.assert_len(1)?;
    let buffer = match compile_slice(par[0], vec![0x8B, 0], 8, 0, 4) {
        Ok(buffer) => buffer,
        Err(_) => compile_slice(par[0], vec![0x8D, 0], 8, 3, 7).parameter("arg 0")?
    };
    destination.write_command(buffer.as_slice())
}

#[allow(dead_code)]
fn slice_cutting(mut long_slice: Vec<u8>, len: usize) -> SliceData {
    if long_slice.len() < len {
        return SliceData::new(long_slice);
    }

    let mut slices: Vec<Vec<u8>> = Vec::new();
    while !long_slice.is_empty() {
        if long_slice.len() <= len {
            if long_slice.len() != 1 {
                slices.push(long_slice);
            }
            break;
        }
        let vec;
        {
            let (head, tail) = long_slice.split_at(len);
            let mut head = head.to_vec();
            head.push(0x80);
            slices.push(head);
            vec = tail.to_vec();
        }
        long_slice = vec;
    }

    let mut cursor = SliceData::new(slices.pop().unwrap());
    while !slices.is_empty() {
        let mut destination = SliceData::new(slices.pop().unwrap());
        destination.append_reference(cursor);
        cursor = destination;
    }

    return cursor;
}

fn compile_xchg<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    par.assert_len_in(0..=2)?;
    if par.len() == 0 {
        destination.write_command(&[0x01])
    } else if par.len() == 1 {
        compile_with_register(par[0], 'S', 1..16, &[0x00], destination)
    } else {
        // 2 parameters
        let reg1 = parse_register(par[0], 'S', 0..16).parameter("arg 0")? as u8;
        let reg2 = parse_register(par[1], 'S', 0..256).parameter("arg 1")? as u8;
        if reg1 >= reg2 {
            Err(OperationError::LogicErrorInParameters(
                "arg 1 should be greater than arg 0"
                ))
        } else if reg1 == 0 {
            if reg2 <= 15 {
                // XCHG s0, si == XCHG si
                destination.write_command(&[reg2 as u8])
            } else {
                destination.write_command(&[0x11, reg2 as u8])
            }
        } else if reg1 == 1 {
            if (reg2 >= 2) && (reg2 <= 15) {
                destination.write_command(&[0x10 | reg2 as u8])
            } else {
                Err(ParameterError::OutOfRange.parameter("Register 2"))
            }
        } else {
            if reg2 > 15 {
                Err(ParameterError::OutOfRange.parameter("Register 2"))
            } else {
                destination.write_command(&[0x10, (((reg1 << 4) & 0xF0) | (reg2 & 0x0F)) as u8])
            }
        }
    }
}

fn compile_throw_helper<T: Writer>(par: &Vec<&str>, short_opcode: u8, long_opcode: u8, destination: &mut T)
-> CompileResult {
    par.assert_len(1)?;
    let number = parse_const_u11(par[0]).parameter("Number")?;
    destination.write_command({
        if number < 64 {
            let number = number as u8;
            Ok(vec![0xF2, (short_opcode | number) as u8])
        } else if number < 2048 {
            let hi = long_opcode | ((number / 256) as u8);
            let lo = (number % 256) as u8;
            Ok(vec![0xF2, hi, lo])
        } else {
            Err(ParameterError::OutOfRange.parameter("Number"))
        }
    }?.as_slice())
}

fn compile_slice(par: &str, mut prefix: Vec<u8>, offset: usize, r: usize, x: usize)
-> Result<Vec<u8>, ParameterError> {
    // prefix - offset..r..x - data
    let shift = (offset + r + x) % 8;
    let mut buffer = parse_slice(par, shift)?;
    let len = buffer.len() as u8 - 1;
    if len >= (1 << x) {
        return Err(ParameterError::OutOfRange)
    }
    if (offset % 8) + r + x < 8 {
        // a tail of the prefix and a start of the data are in a same byte
        buffer[0] |= prefix.pop().unwrap();
    }
    prefix.append(&mut buffer);
    // skip r writing - no references writing
    if shift < x {
        prefix[(offset + r) / 8] |= len >> shift
    }
    if shift != 0 {
        prefix[(offset + r + x) / 8] |= len << (8 - shift)
    }
    Ok(prefix)
}

fn compile_sdbegins<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    par.assert_len(1)?;
    // Regular version have special two aliaces: SDBEGINS '0', SDBEGINS '1'
    if par[0] == "0" {
        destination.write_command(&[0xD7, 0x28, 0x02])
    } else if par[0] == "1" {
        destination.write_command(&[0xD7, 0x28, 0x06])
    } else {
        let buffer = compile_slice(par[0], vec![0xD7, 0x28], 14, 0, 7).parameter("arg 0")?;
        destination.write_command(buffer.as_slice())
    }
}

fn compile_sdbeginsq<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    par.assert_len(1)?;
    let buffer = compile_slice(par[0], vec![0xD7, 0x2C], 14, 0, 7).parameter("arg 0")?;
    destination.write_command(buffer.as_slice())
}

fn compile_throw<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    compile_throw_helper(par, 0x00, 0xC0, destination)
}

fn compile_throwif<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    compile_throw_helper(par, 0x40, 0xD0, destination)
}

fn compile_throwifnot<T: Writer>(_engine: &mut Engine<T>, par: &Vec<&str>, destination: &mut T)
-> CompileResult {
    compile_throw_helper(par, 0x80, 0xE0, destination)
}

// Command compilation context ************************************************

struct CommandContext<T> 
where
    T: Writer
{
    operation: String,
    line_no_cmd: usize,
    char_no_cmd: usize,
    line_no_par: usize,
    char_no_par: usize,
    rule_option: Option<CompileHandler<T>>,
}

impl<T: Writer> CommandContext<T> {
    fn abort<X>(&self, error: OperationError) -> Result<X, CompileError> {
        Err(CompileError::Operation(
            Position {
                line: self.line_no_cmd,
                column: self.char_no_cmd,
            },
            self.operation.clone(),
            error,
        ))
    }
    fn has_command(&self) -> bool {
        self.rule_option.is_some()
    }
    fn compile(
        &mut self,
        destination: &mut T,
        par: &mut Vec<(usize, usize, &str, bool)>,
        engine: &mut Engine<T>,
    ) -> Option<Result<(), CompileError>> {
        match self.rule_option {
            Some(rule) => {
                let (line_no, char_no) = if (self.line_no_par != 0) && (self.char_no_par != 0) {
                    engine.set_pos(self.line_no_par, self.char_no_par)
                } else {
                    (0, 0)
                };
                let mut n = par.len();
                loop {
                    let par = &par[0..n].iter().map(|(_, _, e, _)| *e).collect::<Vec<_>>();
                    match rule(engine, par, destination) {
                        Ok(_) => break,
                        Err(e @ OperationError::TooManyParameters) => {
                            if n == 0 {
                                return Some(self.abort(e));
                            } else {
                                n -= 1;
                            }
                        }
                        Err(e) => return Some(self.abort(e)),
                    }
                }
                if (line_no != 0) && (char_no != 0) {
                    engine.set_pos(line_no, char_no);
                    self.line_no_par = 0;
                    self.char_no_par = 0;
                }
                self.rule_option = None;
                // detecting some errors here if was
                if n > 1 {
                    for (line, column, _, was_comma) in &par[1..n] {
                        if !*was_comma {
                            return Some(Err(CompileError::Syntax(
                                Position {line: *line, column: *column},
                                "Missing comma".to_string(),
                            )));
                        }
                    }
                }
                par.drain(..n);
                if !par.is_empty() {
                    let (position, token, was_comma) = {
                        let (line, column, token, was_comma) = par.remove(0);
                        (Position { line, column }, token, was_comma)
                    };
                    if was_comma {
                        return Some(Err(CompileError::Operation(
                            position,
                            self.operation.clone(),
                            OperationError::TooManyParameters,
                        )));
                    } else if n == 0 {
                        // or CompileError::Operation
                        return Some(Err(CompileError::Operation(
                            position,
                            self.operation.clone(),
                            OperationError::TooManyParameters,
                        )));
                    } else {
                        // or CompileError::Syntax "missing comma"
                        return Some(Err(CompileError::UnknownOperation(
                            position, token.into()
                        )));
                    }
                }
            }
            None => (),
        }
        None
    }
}

// Compilation engine *********************************************************

#[allow(non_snake_case)]
pub struct Engine<T: Writer> {
    line_no: usize,
    char_no: usize,
    COMPILE_ROOT: HashMap<&'static str, CompileHandler<T>>,
}

#[cfg_attr(rustfmt, rustfmt_skip)]
impl<T: Writer> Engine<T> {

    #[cfg_attr(rustfmt, rustfmt_skip)]
    simple_commands! {
        ABS                                  => 0xB6, 0x0B
        ACCEPT                               => 0xF8, 0x00
        ADD                                  => 0xA0
        ADDCONST z = parse_const_i8          => 0xA6, z
        ADDRAND                              => 0xF8, 0x15
        AGAIN                                => 0xEA
        AGAINBRK                             => 0xE3, 0x1A
        AGAINEND                             => 0xEB
        AGAINENDBRK                          => 0xE3, 0x1B
        AND                                  => 0xB0
        ATEXIT                               => 0xED, 0xF3
        ATEXITALT                            => 0xED, 0xF4
        BALANCE                              => 0xF8, 0x27
        BBITREFS                             => 0xCF, 0x33
        BBITS                                => 0xCF, 0x31
        BCHKBITREFS                          => 0xCF, 0x3B
        BCHKBITREFSQ                         => 0xCF, 0x3F
        BCHKREFS                             => 0xCF, 0x3A
        BCHKREFSQ                            => 0xCF, 0x3E
        BINDUMP                              => 0xFE, 0x12
        BINPRINT                             => 0xFE, 0x13
        BITSIZE                              => 0xB6, 0x02
        BLESS                                => 0xED, 0x1E
        BLESSARGS c1 = parse_const_u4;
                  c2 = parse_const_i4        => 0xEE, (c1 << 4) | c2
        BLESSNUMARGS c = parse_const_u4_14   => 0xEE, c
        BLESSVARARGS                         => 0xED, 0x1F
        BLKDROP c = parse_const_u4           => 0x5F, c
        BLKDROP2 c1 = parse_const_u4_nonzero;
                 c2 = parse_const_u4         => 0x6C, (c1 << 4) | c2
        BLKPUSH c1 = parse_const_u4_nonzero;
                c2 = parse_const_u4          => 0x5F, (c1 << 4) | c2
        BLKSWAP c1 = parse_const_u4_plus_one;
                c2 = parse_const_u4_plus_one => 0x55, (c1 << 4) | c2
        BLKSWX                               => 0x63
        BLOCKLT                              => 0xF8, 0x24
        BOOLAND                              => 0xED, 0xF0
        BOOLEVAL                             => 0xED, 0xF9
        BOOLOR                               => 0xED, 0xF1
        BRANCH                               => 0xDB, 0x32
        BREFS                                => 0xCF, 0x32
        BREMBITS                             => 0xCF, 0x35
        BREMBITREFS                          => 0xCF, 0x37
        BREMREFS                             => 0xCF, 0x36
        BUYGAS                               => 0xF8, 0x02
        CADR                                 => 0x6F, 0xB4
        CADDR                                => 0x6F, 0xD4
        CDDR                                 => 0x6F, 0xB5
        CDDDR                                => 0x6F, 0xD5
        CALLCC                               => 0xDB, 0x34
        CALLCCARGS  c1 = parse_const_u4;
                    c2 = parse_const_i4      => 0xDB, 0x36, (c1 << 4) | c2
        CALLCCVARARGS                        => 0xDB, 0x3B
        CALLREF                              => 0xDB, 0x3C
        CALLX                                => 0xD8
        CALLXVARARGS                         => 0xDB, 0x38
        CAR                                  => 0x6F, 0x10
        CDR                                  => 0x6F, 0x11
        CDATASIZE                            => 0xF9, 0x41
        CDATASIZEQ                           => 0xF9, 0x40
        CHANGELIB                            => 0xFB, 0x07
        CHKBOOL                              => 0xB4, 0x00
        CHKBIT                               => 0xB5, 0x00
        CHKNAN                               => 0xC5
        CHKDEPTH                             => 0x69
        CHKSIGNS                             => 0xF9, 0x11
        CHKSIGNU                             => 0xF9, 0x10
        CHKTUPLE                             => 0x6F, 0x30
        CMP                                  => 0xBF
        COMMA                                => 0x6F, 0x8C
        COMMIT                               => 0xF8, 0x0F
        COMPOS                               => 0xED, 0xF0
        COMPOSALT                            => 0xED, 0xF1
        COMPOSBOTH                           => 0xED, 0xF2
        CONDSEL                              => 0xE3, 0x04
        CONDSELCHK                           => 0xE3, 0x05
        CONFIGROOT                           => 0xF8, 0x29
        CONFIGDICT                           => 0xF8, 0x30
        CONFIGPARAM                          => 0xF8, 0x32
        CONFIGOPTPARAM                       => 0xF8, 0x33
        CONS                                 => 0x6F, 0x02
        CTOS                                 => 0xD0
        DEC                                  => 0xA5
        DEBUG z = parse_const_u8_240         => 0xFE, z
        DEBUGOFF                             => 0xFE, 0x1E
        DEBUGON                              => 0xFE, 0x1F
        DEPTH                                => 0x68
        DICTADD                              => 0xF4, 0x32
        DICTADDB                             => 0xF4, 0x51
        DICTADDGET                           => 0xF4, 0x3A
        DICTADDGETB                          => 0xF4, 0x55
        DICTADDGETREF                        => 0xF4, 0x3B
        DICTADDREF                           => 0xF4, 0x33
        DICTDEL                              => 0xF4, 0x59
        DICTDELGET                           => 0xF4, 0x62
        DICTDELGETREF                        => 0xF4, 0x63
        DICTEMPTY                            => 0x6E
        DICTGET                              => 0xF4, 0x0A
        DICTGETNEXT                          => 0xF4, 0x74
        DICTGETNEXTEQ                        => 0xF4, 0x75
        DICTGETOPTREF                        => 0xF4, 0x69
        DICTIGETOPTREF                       => 0xF4, 0x6A
        DICTUGETOPTREF                       => 0xF4, 0x6B
        DICTSETGETOPTREF                     => 0xF4, 0x6D
        DICTISETGETOPTREF                    => 0xF4, 0x6E
        DICTUSETGETOPTREF                    => 0xF4, 0x6F
        DICTGETPREV                          => 0xF4, 0x76
        DICTGETPREVEQ                        => 0xF4, 0x77
        DICTGETREF                           => 0xF4, 0x0B
        DICTIADD                             => 0xF4, 0x34
        DICTIADDB                            => 0xF4, 0x52
        DICTIADDGET                          => 0xF4, 0x3C
        DICTIADDGETB                         => 0xF4, 0x56
        DICTIADDGETREF                       => 0xF4, 0x3D
        DICTIADDREF                          => 0xF4, 0x35
        DICTIDEL                             => 0xF4, 0x5A
        DICTIDELGET                          => 0xF4, 0x64
        DICTIDELGETREF                       => 0xF4, 0x65
        DICTIGET                             => 0xF4, 0x0C
        DICTIGETEXEC                         => 0xF4, 0xA2
        DICTIGETEXECZ                        => 0xF4, 0xBE
        DICTIGETJMP                          => 0xF4, 0xA0
        DICTIGETJMPZ                         => 0xF4, 0xBC
        DICTIGETNEXT                         => 0xF4, 0x78
        DICTIGETNEXTEQ                       => 0xF4, 0x79
        DICTIGETPREV                         => 0xF4, 0x7A
        DICTIGETPREVEQ                       => 0xF4, 0x7B
        DICTIGETREF                          => 0xF4, 0x0D
        DICTIMAX                             => 0xF4, 0x8C
        DICTIMAXREF                          => 0xF4, 0x8D
        DICTIMIN                             => 0xF4, 0x84
        DICTIMINREF                          => 0xF4, 0x85
        DICTIREMMAX                          => 0xF4, 0x9C
        DICTIREMMAXREF                       => 0xF4, 0x9D
        DICTIREMMIN                          => 0xF4, 0x94
        DICTIREMMINREF                       => 0xF4, 0x95
        DICTIREPLACE                         => 0xF4, 0x24
        DICTIREPLACEB                        => 0xF4, 0x4A
        DICTIREPLACEGET                      => 0xF4, 0x2C
        DICTIREPLACEGETB                     => 0xF4, 0x4E
        DICTIREPLACEGETREF                   => 0xF4, 0x2D
        DICTIREPLACEREF                      => 0xF4, 0x25
        DICTISET                             => 0xF4, 0x14
        DICTISETB                            => 0xF4, 0x42
        DICTISETGET                          => 0xF4, 0x1C
        DICTISETGETB                         => 0xF4, 0x46
        DICTISETGETREF                       => 0xF4, 0x1D
        DICTISETREF                          => 0xF4, 0x15
        DICTMAX                              => 0xF4, 0x8A
        DICTMAXREF                           => 0xF4, 0x8B
        DICTMIN                              => 0xF4, 0x82
        DICTMINREF                           => 0xF4, 0x83
        DICTPUSHCONST n = parse_const_u10    => 0xF4, 0xA4 | (n >> 8) as u8, n as u8
        DICTREMMAX                           => 0xF4, 0x9A
        DICTREMMAXREF                        => 0xF4, 0x9B
        DICTREMMIN                           => 0xF4, 0x92
        DICTREMMINREF                        => 0xF4, 0x93
        DICTREPLACE                          => 0xF4, 0x22
        DICTREPLACEB                         => 0xF4, 0x49
        DICTREPLACEGET                       => 0xF4, 0x2A
        DICTREPLACEGETB                      => 0xF4, 0x4D
        DICTREPLACEGETREF                    => 0xF4, 0x2B
        DICTREPLACEREF                       => 0xF4, 0x23
        DICTSET                              => 0xF4, 0x12
        DICTSETB                             => 0xF4, 0x41
        DICTSETGET                           => 0xF4, 0x1A
        DICTSETGETB                          => 0xF4, 0x45
        DICTSETGETREF                        => 0xF4, 0x1B
        DICTSETREF                           => 0xF4, 0x13
        DICTUADD                             => 0xF4, 0x36
        DICTUADDB                            => 0xF4, 0x53
        DICTUADDGET                          => 0xF4, 0x3E
        DICTUADDGETB                         => 0xF4, 0x57
        DICTUADDGETREF                       => 0xF4, 0x3F
        DICTUADDREF                          => 0xF4, 0x37
        DICTUDEL                             => 0xF4, 0x5B
        DICTUDELGET                          => 0xF4, 0x66
        DICTUDELGETREF                       => 0xF4, 0x67
        DICTUGET                             => 0xF4, 0x0E
        DICTUGETEXEC                         => 0xF4, 0xA3
        DICTUGETEXECZ                        => 0xF4, 0xBF
        DICTUGETJMP                          => 0xF4, 0xA1
        DICTUGETJMPZ                         => 0xF4, 0xBD
        DICTUGETNEXT                         => 0xF4, 0x7C
        DICTUGETNEXTEQ                       => 0xF4, 0x7D
        DICTUGETPREV                         => 0xF4, 0x7E
        DICTUGETPREVEQ                       => 0xF4, 0x7F
        DICTUGETREF                          => 0xF4, 0x0F
        DICTUMAX                             => 0xF4, 0x8E
        DICTUMAXREF                          => 0xF4, 0x8F
        DICTUMIN                             => 0xF4, 0x86
        DICTUMINREF                          => 0xF4, 0x87
        DICTUREMMAX                          => 0xF4, 0x9E
        DICTUREMMAXREF                       => 0xF4, 0x9F
        DICTUREMMIN                          => 0xF4, 0x96
        DICTUREMMINREF                       => 0xF4, 0x97
        DICTUREPLACE                         => 0xF4, 0x26
        DICTUREPLACEB                        => 0xF4, 0x4B
        DICTUREPLACEGET                      => 0xF4, 0x2E
        DICTUREPLACEGETB                     => 0xF4, 0x4F
        DICTUREPLACEGETREF                   => 0xF4, 0x2F
        DICTUREPLACEREF                      => 0xF4, 0x27
        DICTUSET                             => 0xF4, 0x16
        DICTUSETB                            => 0xF4, 0x43
        DICTUSETGET                          => 0xF4, 0x1E
        DICTUSETGETB                         => 0xF4, 0x47
        DICTUSETGETREF                       => 0xF4, 0x1F
        DICTUSETREF                          => 0xF4, 0x17
        DIV                                  => 0xA9, 0x04
        DIVC                                 => 0xA9, 0x06
        DIVR                                 => 0xA9, 0x05
        DIVMOD                               => 0xA9, 0x0C
        DIVMODC                              => 0xA9, 0x0E
        DIVMODR                              => 0xA9, 0x0D
        DROP                                 => 0x30
        DROPX                                => 0x65
        DROP2                                => 0x5B
        DUMP z = parse_const_u4_14           => 0xFE, 0x20 | z
        DUMPSTK                              => 0xFE, 0x00
        DUMPSTKTOP z = parse_const_u4_1_14   => 0xFE, z
        DUP                                  => 0x20
        DUP2                                 => 0x5C
        ENDC                                 => 0xC9
        ENDCST                               => 0xCD
        ENDS                                 => 0xD1
        EQUAL                                => 0xBA
        EQINT z = parse_const_i8             => 0xC0, z
        EXECUTE                              => 0xD8
        EXPLODE c = parse_const_u4           => 0x6F, 0x40 | c
        EXPLODEVAR                           => 0x6F, 0x84
        FALSE                                => 0x70
        FIRST                                => 0x6F, 0x10
        FITS z = parse_const_u8_plus_one     => 0xB4, z
        FITSX                                => 0xB6, 0x00
        GASTOGRAM                            => 0xF8, 0x05
        GEQ                                  => 0xBE
        GETGLOBVAR                           => 0xF8, 0x40
        GETGLOB k = parse_const_u5           => 0xF8, 0x40 | k
        GETPARAM c = parse_const_u4          => 0xF8, 0x20 | c
        GRAMTOGAS                            => 0xF8, 0x04
        GREATER                              => 0xBC
        GTINT z = parse_const_i8             => 0xC2, z
        HASHCU                               => 0xF9, 0x00
        HASHSU                               => 0xF9, 0x01
        IF                                   => 0xDE
        IFBITJMP n = parse_const_u5          => 0xE3, 0x80 | n
        IFBITJMPREF n = parse_const_u5       => 0xE3, 0xC0 | n
        IFELSE                               => 0xE2
        IFJMP                                => 0xE0
        IFNBITJMP n = parse_const_u5         => 0xE3, 0xA0 | n
        IFNBITJMPREF n = parse_const_u5      => 0xE3, 0xE0 | n
        IFNOT                                => 0xDF
        IFNOTJMP                             => 0xE1
        IFNOTRET                             => 0xDD
        IFNOTRETALT                          => 0xE3, 0x09
        IFRET                                => 0xDC
        IFRETALT                             => 0xE3, 0x08
        INC                                  => 0xA4
        INTSORT2                             => 0xB6, 0x0A
        INVERT                               => 0xED, 0xF8
        IFELSEREF                            => 0xE3, 0x0E
        IFREF                                => 0xE3, 0x00
        IFREFELSE                            => 0xE3, 0x0D
        IFREFELSEREF                         => 0xE3, 0x0F
        IFNOTREF                             => 0xE3, 0x01
        IFJMPREF                             => 0xE3, 0x02
        IFNOTJMPREF                          => 0xE3, 0x03
        INDEX c = parse_const_u4             => 0x6F, 0x10 | c
        INDEXQ c = parse_const_u4            => 0x6F, 0x60 | c
        INDEXVAR                             => 0x6F, 0x81
        INDEXVARQ                            => 0x6F, 0x86
        INDEX2 i = parse_const_u2;
               j = parse_const_u2            => 0x6F, 0xB0 | (i << 2) | j
        INDEX3 i = parse_const_u2;
               j = parse_const_u2;
               k = parse_const_u2            => 0x6F, 0xC0 | (i << 4) | (j << 2) | k
        ISNAN                                => 0xC4
        ISNEG                                => 0xC1, 0x00
        ISNPOS                               => 0xC1, 0x01
        ISNNEG                               => 0xC2, 0xFF
        ISNULL                               => 0x6E
        ISPOS                                => 0xC2, 0x00
        ISTUPLE                              => 0x6F, 0x8A
        ISZERO                               => 0xC0, 0x00
        JMP n = parse_const_u14              => 0xF1, 0x40 | (((n / 256) as u8)), ((n % 256) as u8)
        JMPX                                 => 0xD9
        JMPXARGS p = parse_const_u4          => 0xDB, 0x10 | p
        JMPXDATA                             => 0xDB, 0x35
        JMPXVARARGS                          => 0xDB, 0x3A
        JMPREF                               => 0xDB, 0x3D
        JMPREFDATA                           => 0xDB, 0x3E
        HEXDUMP                              => 0xFE, 0x10
        HEXPRINT                             => 0xFE, 0x11
        LAST                                 => 0x6F, 0x8B
        LDI cc = parse_const_u8_plus_one     => 0xD2, cc
        LDDICT                               => 0xF4, 0x04
        LDDICTS                              => 0xF4, 0x02
        LDDICTQ                              => 0xF4, 0x06
        LDGRAMS                              => 0xFA, 0x00
        LDILE4                               => 0xD7, 0x50
        LDILE4Q                              => 0xD7, 0x58
        LDILE8                               => 0xD7, 0x52
        LDILE8Q                              => 0xD7, 0x5A
        LDIQ cc = parse_const_u8_plus_one    => 0xD7, 0x0C, cc
        LDIX                                 => 0xD7, 0x00
        LDIXQ                                => 0xD7, 0x04
        LDMSGADDR                            => 0xFA, 0x40
        LDMSGADDRQ                           => 0xFA, 0x41
        LDONES                               => 0xD7, 0x61
        LDOPTREF                             => 0xF4, 0x04
        LDREF                                => 0xD4
        LDREFRTOS                            => 0xD5
        LDSAME                               => 0xD7, 0x62
        LDSLICE cc = parse_const_u8_plus_one => 0xD6, cc
        LDSLICEQ 
            cc = parse_const_u8_plus_one     => 0xD7, 0x1E, cc 
        LDSLICEX                             => 0xD7, 0x18
        LDSLICEXQ                            => 0xD7, 0x1A
        LDU z = parse_const_u8_plus_one      => 0xD3, z
        LDULE4                               => 0xD7, 0x51
        LDULE4Q                              => 0xD7, 0x59
        LDULE8                               => 0xD7, 0x53
        LDULE8Q                              => 0xD7, 0x5B
        LDUQ cc = parse_const_u8_plus_one    => 0xD7, 0x0D, cc
        LDUX                                 => 0xD7, 0x01
        LDUXQ                                => 0xD7, 0x05
        LDVARINT16                           => 0xFA, 0x01
        LDVARINT32                           => 0xFA, 0x05
        LDVARUINT16                          => 0xFA, 0x00
        LDVARUINT32                          => 0xFA, 0x04
        LDZEROES                             => 0xD7, 0x60
        LEQ                                  => 0xBB
        LESS                                 => 0xB9
        LESSINT z = parse_const_i8           => 0xC1, z
        LOGFLUSH                             => 0xFE, 0xF0, 0x00
        LTIME                                => 0xF8, 0x25
        MAX                                  => 0xB6, 0x09
        MIN                                  => 0xB6, 0x08
        MINMAX                               => 0xB6, 0x0A
        MOD                                  => 0xA9, 0x08
        MODC                                 => 0xA9, 0x0A
        MODR                                 => 0xA9, 0x09
        MUL                                  => 0xA8
        MULCONST z = parse_const_i8          => 0xA7, z
        MULDIV                               => 0xA9, 0x84
        MULDIVC                              => 0xA9, 0x86
        MULDIVR                              => 0xA9, 0x85
        MULDIVMOD                            => 0xA9, 0x8C
        MULDIVMODC                           => 0xA9, 0x8E
        MULDIVMODR                           => 0xA9, 0x8D
        MULMOD                               => 0xA9, 0x88
        MULMODC                              => 0xA9, 0x8A
        MULMODR                              => 0xA9, 0x89
        MYADDR                               => 0xF8, 0x28
        NEGATE                               => 0xA3
        NEQ                                  => 0xBD
        NEQINT z = parse_const_i8            => 0xC3, z
        NEWC                                 => 0xC8
        NEWDICT                              => 0x6D
        NIL                                  => 0x6F, 0x00
        NIP                                  => 0x31
        NOP                                  => 0x00
        NOT                                  => 0xB3
        NOW                                  => 0xF8, 0x23
        NULL                                 => 0x6D
        NULLROTRIF                           => 0x6F, 0xA2
        NULLROTRIF2                          => 0x6F, 0xA6
        NULLROTRIFNOT                        => 0x6F, 0xA3
        NULLROTRIFNOT2                       => 0x6F, 0xA7
        NULLSWAPIF                           => 0x6F, 0xA0
        NULLSWAPIF2                          => 0x6F, 0xA4
        NULLSWAPIFNOT                        => 0x6F, 0xA1
        NULLSWAPIFNOT2                       => 0x6F, 0xA5
        ONE                                  => 0x71
        OR                                   => 0xB1
        OVER                                 => 0x21
        OVER2                                => 0x5D
        ONLYTOPX                             => 0x6A
        ONLYX                                => 0x6B
        PAIR                                 => 0x6F, 0x02
        PARSEMSGADDR                         => 0xFA, 0x42
        PARSEMSGADDRQ                        => 0xFA, 0x43
        PFXDICTADD                           => 0xF4, 0x72
        PFXDICTCONSTGETJMP n = parse_const_u10 => 0xF4, 0xAC | (n >> 8) as u8, n as u8
        PFXDICTDEL                           => 0xF4, 0x73
        PFXDICTGET                           => 0xF4, 0xA9
        PFXDICTGETEXEC                       => 0xF4, 0xAB
        PFXDICTGETJMP                        => 0xF4, 0xAA
        PFXDICTGETQ                          => 0xF4, 0xA8
        PFXDICTREPLACE                       => 0xF4, 0x71
        PFXDICTSET                           => 0xF4, 0x70
        PFXDICTSWITCH n = parse_const_u10 => 0xF4, 0xAC | (n >> 8) as u8, n as u8
        PLDDICT                              => 0xF4, 0x05
        PLDDICTS                             => 0xF4, 0x03
        PLDDICTQ                             => 0xF4, 0x07
        PLDI cc = parse_const_u8_plus_one    => 0xD7, 0x0A, cc
        PLDILE4                              => 0xD7, 0x54
        PLDILE4Q                             => 0xD7, 0x5C
        PLDILE8                              => 0xD7, 0x56
        PLDILE8Q                             => 0xD7, 0x5E
        PLDIQ cc = parse_const_u8_plus_one   => 0xD7, 0x0E, cc
        PLDIX                                => 0xD7, 0x02
        PLDIXQ                               => 0xD7, 0x06
        PLDSLICE 
            cc = parse_const_u8_plus_one     => 0xD7, 0x1D, cc
        PLDSLICEQ 
            cc = parse_const_u8_plus_one     => 0xD7, 0x1F, cc
        PLDOPTREF                            => 0xF4, 0x05
        PLDREF                               => 0xD7, 0x4C
        PLDREFVAR                            => 0xD7, 0x48
        PLDREFIDX n = parse_const_u2         => 0xD7, 0x4C | n
        PLDSLICEX                            => 0xD7, 0x19
        PLDSLICEXQ                           => 0xD7, 0x1B
        PLDU cc = parse_const_u8_plus_one    => 0xD7, 0x0B, cc
        PLDULE4                              => 0xD7, 0x55
        PLDULE4Q                             => 0xD7, 0x5D
        PLDULE8                              => 0xD7, 0x57
        PLDULE8Q                             => 0xD7, 0x5F
        PLDUQ cc = parse_const_u8_plus_one   => 0xD7, 0x0F, cc
        PLDUX                                => 0xD7, 0x03
        PLDUXQ                               => 0xD7, 0x07
        PLDUZ c = parse_plduz_parameter      => 0xD7, 0x10 | c
        PICK                                 => 0x60
        PUSHX                                => 0x60
        POPCTR z = parse_control_register    => 0xED, 0x50 | z
        POPCTRSAVE z=parse_control_register  => 0xED, 0x90 | z
        POPCTRX                              => 0xED, 0xE1
        POPROOT                              => 0xED, 0x54
        POPSAVE z = parse_control_register   => 0xED, 0x90 | z
        POW2                                 => 0xAE
        PREPARE n = parse_const_u14          => 0xF1, 0x80 | ((n / 256) as u8), ((n % 256) as u8)
        PREPAREDICT n = parse_const_u14      => 0xF1, 0x80 | ((n / 256) as u8), ((n % 256) as u8)
        PRINT z = parse_const_u4_14          => 0xFE, 0x30 | z
        PU2XC  
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4_minus_one;
            s3 = parse_stack_register_u4_minus_two 
                                             => 0x54, 0x60 | s1, (s2 << 4) | s3
        PUSH2  
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4     => 0x53, (s1 << 4) | s2
        PUSH3  
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4;
            s3 = parse_stack_register_u4     => 0x54, 0x70 | s1, (s2 << 4) | s3
        PUSHCTR z = parse_control_register   => 0xED, 0x40 | z
        PUSHCTRX                             => 0xED, 0xE0
        PUSHNAN                              => 0x83, 0xFF
        PUSHNEGPOW2 
            s1 = parse_const_u8_plus_one     => 0x85, s1
        PUSHNULL                             => 0x6D
        PUSHPOW2 
            s1 = parse_const_u8_plus_one     => 0x83, s1
        PUSHPOW2DEC
            s1 = parse_const_u8_plus_one     => 0x84, s1
        PUSHREF                              => 0x88
        PUSHREFSLICE                         => 0x89
        PUSHREFCONT                          => 0x8A
        PUSHROOT                             => 0xED, 0x44
        PUXC   
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4_minus_one
                                             => 0x52, (s1 << 4) | s2
        PUXC2  
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4_minus_one;
            s3 = parse_stack_register_u4_minus_one
                                             => 0x54, 0x40 | s1, (s2 << 4) | s3
        PUXCPU 
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4_minus_one;
            s3 = parse_stack_register_u4_minus_one
                                             => 0x54, 0x50 | s1, (s2 << 4) | s3
        QABS                                 => 0xB7, 0xB6, 0x0B
        QADD                                 => 0xB7, 0xA0
        QADDCONST z = parse_const_i8         => 0xB7, 0xA6, z
        QAND                                 => 0xB7, 0xB0
        QBITSIZE                             => 0xB7, 0xB6, 0x02
        QCMP                                 => 0xB7, 0xBF
        QDEC                                 => 0xB7, 0xA5
        QDIV                                 => 0xB7, 0xA9, 0x04
        QDIVC                                => 0xB7, 0xA9, 0x06
        QDIVR                                => 0xB7, 0xA9, 0x05
        QDIVMOD                              => 0xB7, 0xA9, 0x0C
        QDIVMODC                             => 0xB7, 0xA9, 0x0E
        QDIVMODR                             => 0xB7, 0xA9, 0x0D
        QEQINT z = parse_const_i8            => 0xB7, 0xC0, z
        QEQUAL                               => 0xB7, 0xBA
        QFITS z = parse_const_u8_plus_one    => 0xB7, 0xB4, z
        QFITSX                               => 0xB7, 0xB6, 0x00
        QGEQ                                 => 0xB7, 0xBE
        QGREATER                             => 0xB7, 0xBC
        QGTINT z = parse_const_i8            => 0xB7, 0xC2, z
        QINC                                 => 0xB7, 0xA4
        QINTSORT2                            => 0xB7, 0xB6, 0x0A
        QMAX                                 => 0xB7, 0xB6, 0x09
        QMIN                                 => 0xB7, 0xB6, 0x08
        QMINMAX                              => 0xB7, 0xB6, 0x0A
        QMOD                                 => 0xB7, 0xA9, 0x08
        QMODC                                => 0xB7, 0xA9, 0x0A
        QMODR                                => 0xB7, 0xA9, 0x09
        QMUL                                 => 0xB7, 0xA8
        QMULCONST z = parse_const_i8         => 0xB7, 0xA7, z
        QMULDIV                              => 0xB7, 0xA9, 0x84
        QMULDIVC                             => 0xB7, 0xA9, 0x86
        QMULDIVR                             => 0xB7, 0xA9, 0x85
        QMULDIVMOD                           => 0xB7, 0xA9, 0x8C
        QMULDIVMODC                          => 0xB7, 0xA9, 0x8E
        QMULDIVMODR                          => 0xB7, 0xA9, 0x8D
        QMULMOD                              => 0xB7, 0xA9, 0x88
        QMULMODC                             => 0xB7, 0xA9, 0x8A
        QMULMODR                             => 0xB7, 0xA9, 0x89
        QLESS                                => 0xB7, 0xB9
        QLESSINT z = parse_const_i8          => 0xB7, 0xC1, z
        QLEQ                                 => 0xB7, 0xBB
        QNEGATE                              => 0xB7, 0xA3
        QNEQ                                 => 0xB7, 0xBD
        QNEQINT z = parse_const_i8           => 0xB7, 0xC3, z
        QNOT                                 => 0xB7, 0xB3
        QOR                                  => 0xB7, 0xB1
        QPOW2                                => 0xB7, 0xAE
        QSGN                                 => 0xB7, 0xB8
        QSUB                                 => 0xB7, 0xA1
        QSUBR                                => 0xB7, 0xA2
        QTLEN                                => 0x6F, 0x89
        QUBITSIZE                            => 0xB7, 0xB6, 0x03
        QUFITS z = parse_const_u8_plus_one   => 0xB7, 0xB5, z
        QUFITSX                              => 0xB7, 0xB6, 0x01
        QXOR                                 => 0xB7, 0xB2
        RAND                                 => 0xF8, 0x11
        RANDSEED                             => 0xF8, 0x26
        RANDU256                             => 0xF8, 0x10
        RAWRESERVE                           => 0xFB, 0x02
        RAWRESERVEX                          => 0xFB, 0x03
        REPEAT                               => 0xE4
        REPEATBRK                            => 0xE3, 0x14
        REPEATEND                            => 0xE5
        REPEATENDBRK                         => 0xE3, 0x15
        RET                                  => 0xDB, 0x30
        RETALT                               => 0xDB, 0x31
        RETARGS r = parse_const_u4           => 0xDB, 0x20 | r
        RETBOOL                              => 0xDB, 0x32
        RETDATA                              => 0xDB, 0x3F
        RETFALSE                             => 0xDB, 0x31
        RETTRUE                              => 0xDB, 0x30
        RETURNARGS c = parse_const_u4        => 0xED, c
        RETURNVARARGS                        => 0xED, 0x10
        RETVARARGS                           => 0xDB, 0x39
        REVERSE c1 = parse_const_u4_plus_two;
                c2 = parse_const_u4          => 0x5E, (c1 << 4) | c2
        REVX                                 => 0x64
        REWRITESTDADDR                       => 0xFA, 0x44
        REWRITESTDADDRQ                      => 0xFA, 0x45
        REWRITEVARADDR                       => 0xFA, 0x46
        REWRITEVARADDRQ                      => 0xFA, 0x47
        ROT                                  => 0x58
        ROT2                                 => 0x55, 0x13
        ROTREV                               => 0x59
        ROLL c = parse_const_u4_plus_one     => 0x55, c
        ROLLREV c = parse_const_u4_plus_one  => 0x55, c << 4
        ROLLX                                => 0x61
        ROLLREVX                             => 0x62
        SAMEALT                              => 0xED, 0xFA
        SAMEALTSAV                           => 0xED, 0xFB
        SAVE z = parse_control_register      => 0xED, 0xA0 | z
        SAVEALT z = parse_control_register   => 0xED, 0xB0 | z
        SAVEALTCTR z=parse_control_register  => 0xED, 0xB0 | z
        SAVEBOTH z = parse_control_register  => 0xED, 0xC0 | z
        SAVEBOTHCTR z=parse_control_register => 0xED, 0xC0 | z
        SAVECTR z = parse_control_register   => 0xED, 0xA0 | z
        SBITS                                => 0xD7, 0x49
        SBITREFS                             => 0xD7, 0x4B
        SCHKBITS                             => 0xD7, 0x41
        SCHKBITREFS                          => 0xD7, 0x43
        SCHKBITREFSQ                         => 0xD7, 0x47
        SCHKBITSQ                            => 0xD7, 0x45
        SCHKREFS                             => 0xD7, 0x42
        SCHKREFSQ                            => 0xD7, 0x46
        SCUTFIRST                            => 0xD7, 0x30
        SCUTLAST                             => 0xD7, 0x32
        SDATASIZE                            => 0xF9, 0x43
        SDATASIZEQ                           => 0xF9, 0x42
        SDBEGINSX                            => 0xD7, 0x26
        SDBEGINSXQ                           => 0xD7, 0x27
        SDCNTLEAD0                           => 0xC7, 0x10
        SDCNTLEAD1                           => 0xC7, 0x11
        SDCNTTRAIL0                          => 0xC7, 0x12
        SDCNTTRAIL1                          => 0xC7, 0x13
        SDCUTFIRST                           => 0xD7, 0x20
        SDCUTLAST                            => 0xD7, 0x22
        SDEMPTY                              => 0xC7, 0x01
        SDEQ                                 => 0xC7, 0x05
        SDFIRST                              => 0xC7, 0x03
        SDPFX                                => 0xC7, 0x08
        SDPFXREV                             => 0xC7, 0x09
        SDPPFX                               => 0xC7, 0x0A
        SDPPFXREV                            => 0xC7, 0x0B
        SDPSFX                               => 0xC7, 0x0E
        SDPSFXREV                            => 0xC7, 0x0F
        SDSFX                                => 0xC7, 0x0C
        SDSFXREV                             => 0xC7, 0x0D
        SDLEXCMP                             => 0xC7, 0x04
        SDSKIPFIRST                          => 0xD7, 0x21
        SDSKIPLAST                           => 0xD7, 0x23
        SDSUBSTR                             => 0xD7, 0x24
        SECOND                               => 0x6F, 0x11
        SEMPTY                               => 0xC7, 0x00
        SENDRAWMSG                           => 0xFB, 0x00
        SETALTCTR z = parse_control_register => 0xED, 0x80 | z
        SETCODE                              => 0xFB, 0x04
        SETCONT z = parse_control_register   => 0xED, 0x60 | z
        SETCONTCTR z=parse_control_register  => 0xED, 0x60 | z
        SETCONTCTRX                          => 0xED, 0xE2
        SETCONTVARARGS                       => 0xED, 0x11
        SETCP z = parse_const_u8_setcp       => 0xFF, z
        SETCP0                               => 0xFF, 0x00
        SETCPX                               => 0xFF, 0xF0
        SETEXITALT                           => 0xED, 0xF5
        SETGASLIMIT                          => 0xF8, 0x01
        SETGLOBVAR                           => 0xF8, 0x60
        SETGLOB k = parse_const_u5           => 0xF8, 0x60 | k
        SETFIRST                             => 0x6F, 0x50
        SETINDEX c = parse_const_u4          => 0x6F, 0x50 | c
        SETINDEXQ c = parse_const_u4         => 0x6F, 0x70 | c
        SETINDEXVAR                          => 0x6F, 0x85
        SETINDEXVARQ                         => 0x6F, 0x87
        SETLIBCODE                           => 0xFB, 0x06
        SETNUMARGS c = parse_const_u4_14     => 0xEC, c
        SETNUMVARARGS                        => 0xED, 0x12
        SETRAND                              => 0xF8, 0x14
        SETRETCTR z = parse_control_register => 0xED, 0x70 | z
        SETSECOND                            => 0x6F, 0x51
        SETTHIRD                             => 0x6F, 0x52
        SGN                                  => 0xB8
        SHA256U                              => 0xF9, 0x02
        SINGLE                               => 0x6F, 0x01
        SKIPDICT                             => 0xF4, 0x01
        SKIPOPTREF                           => 0xF4, 0x01
        SPLIT                                => 0xD7, 0x36
        SPLITQ                               => 0xD7, 0x37
        SREFS                                => 0xD7, 0x4A
        SREMPTY                              => 0xC7, 0x02
        SSKIPFIRST                           => 0xD7, 0x31
        SSKIPLAST                            => 0xD7, 0x33
        STB                                  => 0xCF, 0x13
        STBQ                                 => 0xCF, 0x1B
        STBR                                 => 0xCF, 0x17
        STBREF                               => 0xCF, 0x11
        STBREFQ                              => 0xCF, 0x19
        STBREFR                              => 0xCD
        STBREFRQ                             => 0xCF, 0x1D
        STBRQ                                => 0xCF, 0x1F
        STGRAMS                              => 0xFA, 0x02
        STDICT                               => 0xF4, 0x00
        STDICTS                              => 0xCE
        STI z = parse_const_u8_plus_one      => 0xCA, z
        STILE4                               => 0xCF, 0x28
        STILE8                               => 0xCF, 0x2A
        STIQ z = parse_const_u8_plus_one     => 0xCF, 0x0C, z
        STIR z = parse_const_u8_plus_one     => 0xCF, 0x0A, z
        STIRQ z = parse_const_u8_plus_one    => 0xCF, 0x0E, z
        STIX                                 => 0xCF, 0x00
        STIXQ                                => 0xCF, 0x04
        STIXR                                => 0xCF, 0x02
        STIXRQ                               => 0xCF, 0x06
        STONES                               => 0xCF, 0x41
        STOPTREF                             => 0xF4, 0x00
        STRDUMP                              => 0xFE, 0x14
        STRPRINT                             => 0xFE, 0x15
        STREF                                => 0xCC
        STREF2CONST                          => 0xCF, 0x21
        STREF3CONST                          => 0xCF, 0xE2
        STREFCONST                           => 0xCF, 0x20
        STREFQ                               => 0xCF, 0x18
        STREFR                               => 0xCF, 0x14
        STREFRQ                              => 0xCF, 0x1C
        STSAME                               => 0xCF, 0x42
        STSLICE                              => 0xCE
        STSLICEQ                             => 0xCF, 0x1A
        STSLICER                             => 0xCF, 0x16
        STSLICERQ                            => 0xCF, 0x1E
        STU z = parse_const_u8_plus_one      => 0xCB, z
        STULE4                               => 0xCF, 0x29
        STULE8                               => 0xCF, 0x2B
        STUQ z = parse_const_u8_plus_one     => 0xCF, 0x0D, z
        STUR z = parse_const_u8_plus_one     => 0xCF, 0x0B, z
        STURQ z = parse_const_u8_plus_one    => 0xCF, 0x0F, z
        STUX                                 => 0xCF, 0x01
        STUXQ                                => 0xCF, 0x05
        STUXR                                => 0xCF, 0x03
        STUXRQ                               => 0xCF, 0x07
        STVARINT16                           => 0xFA, 0x03
        STVARINT32                           => 0xFA, 0x07
        STVARUINT16                          => 0xFA, 0x02
        STVARUINT32                          => 0xFA, 0x06
        STZEROES                             => 0xCF, 0x40
        SUB                                  => 0xA1
        SUBDICTGET                           => 0xF4, 0xB1
        SUBDICTIGET                          => 0xF4, 0xB2
        SUBDICTUGET                          => 0xF4, 0xB3
        SUBDICTIRPGET                        => 0xF4, 0xB6
        SUBDICTRPGET                         => 0xF4, 0xB5
        SUBDICTURPGET                        => 0xF4, 0xB7
        SUBR                                 => 0xA2
        SUBSLICE                             => 0xD7, 0x34
        SWAP2                                => 0x5A
        TEN                                  => 0x7A
        THENRET                              => 0xED, 0xF6
        THENRETALT                           => 0xED, 0xF7
        THIRD                                => 0x6F, 0x12
        THROWANY                             => 0xF2, 0xF0
        THROWANYIF                           => 0xF2, 0xF2
        THROWANYIFNOT                        => 0xF2, 0xF4
        THROWARG n = parse_const_u11         => 0xF2, 0xC8 | ((n / 256) as u8), ((n % 256) as u8)
        THROWARGANY                          => 0xF2, 0xF1
        THROWARGANYIF                        => 0xF2, 0xF3
        THROWARGANYIFNOT                     => 0xF2, 0xF5
        THROWARGIF n = parse_const_u11       => 0xF2, 0xD8 | ((n / 256) as u8), ((n % 256) as u8)
        THROWARGIFNOT n = parse_const_u11    => 0xF2, 0xE8 | ((n / 256) as u8), ((n % 256) as u8)
        TLEN                                 => 0x6F, 0x88
        TPOP                                 => 0x6F, 0x8D
        TPUSH                                => 0x6F, 0x8C
        TRIPLE                               => 0x6F, 0x03
        TRUE                                 => 0x7F
        TRY                                  => 0xF2, 0xFF
        TRYARGS s1 = parse_const_u4;
                s2 = parse_const_u4          => 0xF3, (s1 << 4 | s2)
        TUCK                                 => 0x66
        TUPLE s = parse_const_u4             => 0x6F, s
        TUPLEVAR                             => 0x6F, 0x80
        TWO                                  => 0x72
        UBITSIZE                             => 0xB6, 0x03
        UFITS z = parse_const_u8_plus_one    => 0xB5, z
        UFITSX                               => 0xB6, 0x01
        UNCONS                               => 0x6F, 0x22
        UNPACKFIRST c = parse_const_u4       => 0x6F, 0x30 | c
        UNPACKFIRSTVAR                       => 0x6F, 0x83
        UNPAIR                               => 0x6F, 0x22
        UNSINGLE                             => 0x6F, 0x21
        UNTIL                                => 0xE6
        UNTILBRK                             => 0xE3, 0x16
        UNTILEND                             => 0xE7
        UNTILENDBRK                          => 0xE3, 0x17
        UNTRIPLE                             => 0x6F, 0x23
        UNTUPLE c = parse_const_u4           => 0x6F, 0x20 | c
        UNTUPLEVAR                           => 0x6F, 0x82
        WHILE                                => 0xE8
        WHILEBRK                             => 0xE3, 0x18
        WHILEEND                             => 0xE9
        WHILEENDBRK                          => 0xE3, 0x19
        XC2PU 
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4;
            s3 = parse_stack_register_u4     => 0x54, 0x10 | s1, (s2 << 4) | s3 
        XCHG2 
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4     => 0x50, (s1 << 4) | s2
        XCHG3 
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4;
            s3 = parse_stack_register_u4     => 0x40 | s1, (s2 << 4) | s3
        XCHGX                                => 0x67
        XCPU  
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4     => 0x51, (s1 << 4) | s2
        XCPU2 
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4;
            s3 = parse_stack_register_u4     => 0x54, 0x30 | s1, (s2 << 4) | s3
        XCPUXC 
            s1 = parse_stack_register_u4;
            s2 = parse_stack_register_u4;
            s3 = parse_stack_register_u4_minus_one
                                             => 0x54, 0x20 | s1, (s2 << 4) | s3
        XCTOS                                => 0xD7, 0x39
        XLOAD                                => 0xD7, 0x3A
        XLOADQ                               => 0xD7, 0x3B
        XOR                                  => 0xB2
        ZERO                                 => 0x70
    }

    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn new() -> Engine<T> {
        let mut ret = Engine::<T> {
            line_no: 1,
            char_no: 1,
            COMPILE_ROOT: HashMap::new(),
        };
        // Alphabetically sorted
        ret.COMPILE_ROOT.insert("-ROLL",          Engine::ROLLREV);
        ret.COMPILE_ROOT.insert("-ROLLX",         Engine::ROLLREVX);
        ret.COMPILE_ROOT.insert("-ROT",           Engine::ROTREV);
        ret.COMPILE_ROOT.insert("2DROP",          Engine::DROP2);
        ret.COMPILE_ROOT.insert("2DUP",           Engine::DUP2);
        ret.COMPILE_ROOT.insert("2OVER",          Engine::OVER2);
        ret.COMPILE_ROOT.insert("2ROT",           Engine::ROT2);
        ret.COMPILE_ROOT.insert("2SWAP",          Engine::SWAP2);
        ret.COMPILE_ROOT.insert("CALL",           compile_call);
        ret.COMPILE_ROOT.insert("CALLDICT",       compile_call);
        ret.COMPILE_ROOT.insert("CALLXARGS",      compile_callxargs);
        ret.COMPILE_ROOT.insert("BCHKBITS",       compile_bchkbits);
        ret.COMPILE_ROOT.insert("BCHKBITSQ",      compile_bchkbitsq);
        ret.COMPILE_ROOT.insert("DEBUGSTR",       compile_dumptosfmt);
        ret.COMPILE_ROOT.insert("DUMPTOSFMT",     compile_dumptosfmt);
        ret.COMPILE_ROOT.insert("JMPDICT",        Engine::JMP);
        ret.COMPILE_ROOT.insert("LOGSTR",         compile_logstr);
        ret.COMPILE_ROOT.insert("LSHIFT",         Div::<Signaling>::lshift);
        ret.COMPILE_ROOT.insert("LSHIFTDIV",      Div::<Signaling>::lshiftdiv);
        ret.COMPILE_ROOT.insert("LSHIFTDIVC",     Div::<Signaling>::lshiftdivc);
        ret.COMPILE_ROOT.insert("LSHIFTDIVMOD",   Div::<Signaling>::lshiftdivmod);
        ret.COMPILE_ROOT.insert("LSHIFTDIVMODC",  Div::<Signaling>::lshiftdivmodc);
        ret.COMPILE_ROOT.insert("LSHIFTDIVMODR",  Div::<Signaling>::lshiftdivmodr);
        ret.COMPILE_ROOT.insert("LSHIFTDIVR",     Div::<Signaling>::lshiftdivr);
        ret.COMPILE_ROOT.insert("LSHIFTMOD",      Div::<Signaling>::lshiftmod);
        ret.COMPILE_ROOT.insert("LSHIFTMODC",     Div::<Signaling>::lshiftmodc);
        ret.COMPILE_ROOT.insert("LSHIFTMODR",     Div::<Signaling>::lshiftmodr);
        ret.COMPILE_ROOT.insert("MODPOW2",        Div::<Signaling>::modpow2);
        ret.COMPILE_ROOT.insert("MODPOW2C",       Div::<Signaling>::modpow2c);
        ret.COMPILE_ROOT.insert("MODPOW2R",       Div::<Signaling>::modpow2r);
        ret.COMPILE_ROOT.insert("MULMODPOW2",     Div::<Signaling>::mulmodpow2);
        ret.COMPILE_ROOT.insert("MULMODPOW2C",    Div::<Signaling>::mulmodpow2c);
        ret.COMPILE_ROOT.insert("MULMODPOW2R",    Div::<Signaling>::mulmodpow2r);
        ret.COMPILE_ROOT.insert("MULRSHIFT",      Div::<Signaling>::mulrshift);
        ret.COMPILE_ROOT.insert("MULRSHIFTC",     Div::<Signaling>::mulrshiftc);
        ret.COMPILE_ROOT.insert("MULRSHIFTMOD",   Div::<Signaling>::mulrshiftmod);
        ret.COMPILE_ROOT.insert("MULRSHIFTMODC",  Div::<Signaling>::mulrshiftmodc);
        ret.COMPILE_ROOT.insert("MULRSHIFTMODR",  Div::<Signaling>::mulrshiftmodr);
        ret.COMPILE_ROOT.insert("MULRSHIFTR",     Div::<Signaling>::mulrshiftr);
        ret.COMPILE_ROOT.insert("POP",            compile_pop);
        ret.COMPILE_ROOT.insert("PRINTSTR",       compile_printstr);
        ret.COMPILE_ROOT.insert("PUSH",           compile_push);
        ret.COMPILE_ROOT.insert("PUSHCONT",       compile_pushcont);
        ret.COMPILE_ROOT.insert("PUSHINT",        compile_pushint);
        ret.COMPILE_ROOT.insert("PUSHSLICE",      compile_pushslice);
        ret.COMPILE_ROOT.insert("SETCONTARGS",    compile_setcontargs);
        ret.COMPILE_ROOT.insert("SWAP",           compile_xchg);
        ret.COMPILE_ROOT.insert("QLSHIFT",        Div::<Quiet>::lshift);
        ret.COMPILE_ROOT.insert("QLSHIFTDIV",     Div::<Quiet>::lshiftdiv);
        ret.COMPILE_ROOT.insert("QLSHIFTDIVC",    Div::<Quiet>::lshiftdivc);
        ret.COMPILE_ROOT.insert("QLSHIFTDIVMOD",  Div::<Quiet>::lshiftdivmod);
        ret.COMPILE_ROOT.insert("QLSHIFTDIVMODC", Div::<Quiet>::lshiftdivmodc);
        ret.COMPILE_ROOT.insert("QLSHIFTDIVMODR", Div::<Quiet>::lshiftdivmodr);
        ret.COMPILE_ROOT.insert("QLSHIFTDIVR",    Div::<Quiet>::lshiftdivr);
        ret.COMPILE_ROOT.insert("QLSHIFTMOD",     Div::<Quiet>::lshiftmod);
        ret.COMPILE_ROOT.insert("QLSHIFTMODC",    Div::<Quiet>::lshiftmodc);
        ret.COMPILE_ROOT.insert("QLSHIFTMODR",    Div::<Quiet>::lshiftmodr);
        ret.COMPILE_ROOT.insert("QMODPOW2",       Div::<Quiet>::modpow2);
        ret.COMPILE_ROOT.insert("QMODPOW2C",      Div::<Quiet>::modpow2c);
        ret.COMPILE_ROOT.insert("QMODPOW2R",      Div::<Quiet>::modpow2r);
        ret.COMPILE_ROOT.insert("QMULMODPOW2",    Div::<Quiet>::mulmodpow2);
        ret.COMPILE_ROOT.insert("QMULMODPOW2C",   Div::<Quiet>::mulmodpow2c);
        ret.COMPILE_ROOT.insert("QMULMODPOW2R",   Div::<Quiet>::mulmodpow2r);
        ret.COMPILE_ROOT.insert("QMULRSHIFT",     Div::<Quiet>::mulrshift);
        ret.COMPILE_ROOT.insert("QMULRSHIFTC",    Div::<Quiet>::mulrshiftc);
        ret.COMPILE_ROOT.insert("QMULRSHIFTMOD",  Div::<Quiet>::mulrshiftmod);
        ret.COMPILE_ROOT.insert("QMULRSHIFTMODC", Div::<Quiet>::mulrshiftmodc);
        ret.COMPILE_ROOT.insert("QMULRSHIFTMODR", Div::<Quiet>::mulrshiftmodr);
        ret.COMPILE_ROOT.insert("QMULRSHIFTR",    Div::<Quiet>::mulrshiftr);
        ret.COMPILE_ROOT.insert("QRSHIFT",        Div::<Quiet>::rshift);
        ret.COMPILE_ROOT.insert("QRSHIFTC",       Div::<Quiet>::rshiftc);
        ret.COMPILE_ROOT.insert("QRSHIFTMOD",     Div::<Quiet>::rshiftmod);
        ret.COMPILE_ROOT.insert("QRSHIFTMODC",    Div::<Quiet>::rshiftmodc);
        ret.COMPILE_ROOT.insert("QRSHIFTMODR",    Div::<Quiet>::rshiftmodr);
        ret.COMPILE_ROOT.insert("QRSHIFTR",       Div::<Quiet>::rshiftr);
        ret.COMPILE_ROOT.insert("RSHIFT",         Div::<Signaling>::rshift);
        ret.COMPILE_ROOT.insert("RSHIFTMOD",      Div::<Signaling>::rshiftmod);
        ret.COMPILE_ROOT.insert("RSHIFTMODC",     Div::<Signaling>::rshiftmodc);
        ret.COMPILE_ROOT.insert("RSHIFTMODR",     Div::<Signaling>::rshiftmodr);
        ret.COMPILE_ROOT.insert("RSHIFTR",        Div::<Signaling>::rshiftr);
        ret.COMPILE_ROOT.insert("RSHIFTC",        Div::<Signaling>::rshiftc);
        ret.COMPILE_ROOT.insert("SDBEGINS",       compile_sdbegins);
        ret.COMPILE_ROOT.insert("SDBEGINSQ",      compile_sdbeginsq);
        ret.COMPILE_ROOT.insert("SETCONTARGS",    compile_setcontargs);
        ret.COMPILE_ROOT.insert("STSLICECONST",   compile_stsliceconst);
        ret.COMPILE_ROOT.insert("THROW",          compile_throw);
        ret.COMPILE_ROOT.insert("THROWIF",        compile_throwif);
        ret.COMPILE_ROOT.insert("THROWIFNOT",     compile_throwifnot);
        ret.COMPILE_ROOT.insert("XCHG",           compile_xchg);
        // Add automatic commands
        for (command, handler) in Self::enumerate_simple_commands() {
            if ret.COMPILE_ROOT.insert(command, *handler).is_some() {
                panic!("Token {} was already registered.", command);
            }
        }
        ret
    }

    fn is_whitespace(x: char) -> bool {
        match x {
            ' ' => true,
            '\n' => true,
            '\r' => true,
            '\t' => true,
            _ => false,
        }
    }

    fn set_pos(&mut self, line_no: usize, char_no: usize) -> (usize, usize) {
        let (l, c) = (self.line_no, self.char_no);
        self.line_no = line_no;
        self.char_no = char_no;
        (l, c)
    }

    fn compile(&mut self, source: &str) -> Result<T, CompileError> {
        let mut ret = T::new();
        let mut par: Vec<(usize, usize, &str, bool)> = Vec::new();
        let mut acc = (0, 0);
        let mut expect_comma = false;
        let mut comma_found = false;
        let mut was_comma = false; // was comma before token
        let mut in_block = 0;
        let mut in_comment = false;
        let mut command_ctx = CommandContext {
            operation: String::new(),
            line_no_cmd: 0,
            char_no_cmd: 0,
            line_no_par: 0,
            char_no_par: 0,
            rule_option: None,
        };
        for ch in source.chars().chain(" ".chars()) {
            // Adjust line/char information
            let mut x = self.char_no;
            let y = self.line_no;
            if ch == '\n' {
                self.line_no += 1;
                self.char_no = 1
            } else {
                self.char_no += 1
            }
            let (s0, s1) = acc;
            let new_s1 = s1 + ch.len_utf8();
            // Process internal block if any
            if in_block > 0 {
                if ch == '{' {
                    in_block += 1
                } else if ch == '}' {
                    in_block -= 1
                }
                if in_block == 0 {
                    par.push((y, x, &source[s0..s1], comma_found));
                    acc = (new_s1, new_s1)
                } else {
                    acc = (s0, new_s1)
                }
                continue;
            }
            // Process comment if any
            if in_comment {
                if (ch == '\r') || (ch == '\n') {
                    in_comment = false;
                }
                acc = (new_s1, new_s1);
                continue;
            }
            // Analyze char
            if Engine::<T>::is_whitespace(ch) {
                acc = (new_s1, new_s1);
                if s0 == s1 {
                    continue;
                }
            } else if ch == ';' {
                acc = (new_s1, new_s1);
                in_comment = true;
                continue;
            } else if ch == ',' {
                if !expect_comma {
                    return Err(CompileError::Syntax(
                        Position {
                            line: y,
                            column: x
                        },
                        ",".to_string()
                    ));
                }
                acc = (new_s1, new_s1);
                expect_comma = false;
                comma_found = true;
                if s0 == s1 {
                    continue;
                }
            } else if ch == '{' {
                if expect_comma || !command_ctx.has_command() || !par.is_empty() {
                    return Err(CompileError::Syntax(
                        Position {
                            line: y,
                            column: x
                        },
                        ch.to_string()
                    ));
                }
                acc = (new_s1, new_s1);
                in_block = 1;
                command_ctx.line_no_par = self.line_no;
                command_ctx.char_no_par = self.char_no;
                continue;
            } else if ch == '}' {
                return Err(CompileError::Syntax(
                    Position {
                        line: y,
                        column: x
                    },
                    ch.to_string()
                ));
            } else if ch.is_ascii_alphanumeric() || (ch == '-') || (ch == '_') {
                acc = (s0, new_s1);
                if s0 == s1 { //start of new token
                    was_comma = comma_found;
                    comma_found = false;
                    expect_comma = true
                }
                continue;
            } else { // TODO: (message for the owner: please write descriptive explanation)
                return Err(CompileError::Syntax(
                    Position {
                        line: y,
                        column: x
                    },
                    "Bad char".to_string()
                ));
            }
            // Token extracted
            let token = source[s0..s1].to_ascii_uppercase();
            log::trace!(target: "tvm", "--> {}\n", token);
            x -= token.chars().count();
            match self.COMPILE_ROOT.get(&token[..]) {
                None => {
                    if command_ctx.has_command() {
                        par.push((y, x, &source[s0..s1], was_comma));
                        was_comma = false;
                        continue
                    } else {
                        return Err(CompileError::UnknownOperation(
                            Position {
                                line: y,
                                column: x
                            },
                            token
                        ));
                    }
                },
                Some(&new_rule) => {
                    if let Some(e) = command_ctx.compile(&mut ret, &mut par, self) {
                        e?;
                        return Ok(ret);
                    }
                    command_ctx.operation   = token;
                    command_ctx.line_no_cmd = y;
                    command_ctx.char_no_cmd = x;
                    command_ctx.rule_option = Some(new_rule);
                    expect_comma = false;
                    was_comma = false;
                }
            }
        }
        // Compile last pending command if any
        if let Some(e) = command_ctx.compile(&mut ret, &mut par, self) {
            e?;
        }
        Ok(ret)
    }

}

pub fn compile_code(code: &str) -> Result<SliceData, CompileError> {
    log::trace!(target: "tvm", "begin compile\n");
    Engine::<CodePage0>::new().compile(code).map(|code| code.finalize())
}


