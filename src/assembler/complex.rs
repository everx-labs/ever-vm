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

use crate::{error::tvm_exception_code, stack::integer::IntegerData};
use std::{marker::PhantomData, ops::Range};
use ton_types::{SliceData, types::ExceptionCode};

use super::errors::{
    OperationError, ParameterError,
};

use super::{
    CompileResult, Engine, EnsureParametersCountInRange,
    errors::ToOperationParameterError,
    parse::*,
    writer::Writer,
};

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
    if engine.line_no == 0 && engine.char_no == 0 {
        return Err(OperationError::MissingBlock)
    }
    par.assert_len(1)?;
    let cont = engine
        .compile(par[0])
        .map_err(|e| OperationError::Nested(Box::new(e)))?
        .finalize();
    let refs = cont.references().len() as u8;
    if refs > 0 {
        destination.write_composite_command(
            &[0x8E as u8 | ((refs & 0x2) >> 1), (refs & 0x1) << 0x7], 
            cont
        )
    } else {
        let n = cont.data().len();
        if n <= 15 {
            let mut command = vec![0x90 | n as u8];
            command.extend_from_slice(cont.data());
            destination.write_command(command.as_slice())
        } else if n <= 125 {
            let mut command = vec![0x8E, n as u8];
            command.extend_from_slice(cont.data());
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
                Err(err) => {
                    let err = match tvm_exception_code(&err) {
                        None | Some(ExceptionCode::TypeCheckError) => ParameterError::UnexpectedType,
                        Some(ExceptionCode::IntegerOverflow) =>  ParameterError::OutOfRange,
                        _ => ParameterError::NotSupported
                    };
                    return Err(err.parameter("arg 0"))
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

pub(super) fn compile_slice(par: &str, mut prefix: Vec<u8>, offset: usize, r: usize, x: usize)
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

// Compilation engine *********************************************************

#[cfg_attr(rustfmt, rustfmt_skip)]
impl<T: Writer> Engine<T> {

    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn add_complex_commands(&mut self) {
        // Alphabetically sorted
        self.COMPILE_ROOT.insert("-ROLL",          Engine::ROLLREV);
        self.COMPILE_ROOT.insert("-ROLLX",         Engine::ROLLREVX);
        self.COMPILE_ROOT.insert("-ROT",           Engine::ROTREV);
        self.COMPILE_ROOT.insert("2DROP",          Engine::DROP2);
        self.COMPILE_ROOT.insert("2DUP",           Engine::DUP2);
        self.COMPILE_ROOT.insert("2OVER",          Engine::OVER2);
        self.COMPILE_ROOT.insert("2ROT",           Engine::ROT2);
        self.COMPILE_ROOT.insert("2SWAP",          Engine::SWAP2);
        self.COMPILE_ROOT.insert("CALL",           compile_call);
        self.COMPILE_ROOT.insert("CALLDICT",       compile_call);
        self.COMPILE_ROOT.insert("CALLXARGS",      compile_callxargs);
        self.COMPILE_ROOT.insert("BCHKBITS",       compile_bchkbits);
        self.COMPILE_ROOT.insert("BCHKBITSQ",      compile_bchkbitsq);
        self.COMPILE_ROOT.insert("DEBUGSTR",       compile_dumptosfmt);
        self.COMPILE_ROOT.insert("DUMPTOSFMT",     compile_dumptosfmt);
        self.COMPILE_ROOT.insert("JMPDICT",        Engine::JMP);
        self.COMPILE_ROOT.insert("LOGSTR",         compile_logstr);
        self.COMPILE_ROOT.insert("LSHIFT",         Div::<Signaling>::lshift);
        self.COMPILE_ROOT.insert("LSHIFTDIV",      Div::<Signaling>::lshiftdiv);
        self.COMPILE_ROOT.insert("LSHIFTDIVC",     Div::<Signaling>::lshiftdivc);
        self.COMPILE_ROOT.insert("LSHIFTDIVMOD",   Div::<Signaling>::lshiftdivmod);
        self.COMPILE_ROOT.insert("LSHIFTDIVMODC",  Div::<Signaling>::lshiftdivmodc);
        self.COMPILE_ROOT.insert("LSHIFTDIVMODR",  Div::<Signaling>::lshiftdivmodr);
        self.COMPILE_ROOT.insert("LSHIFTDIVR",     Div::<Signaling>::lshiftdivr);
        self.COMPILE_ROOT.insert("LSHIFTMOD",      Div::<Signaling>::lshiftmod);
        self.COMPILE_ROOT.insert("LSHIFTMODC",     Div::<Signaling>::lshiftmodc);
        self.COMPILE_ROOT.insert("LSHIFTMODR",     Div::<Signaling>::lshiftmodr);
        self.COMPILE_ROOT.insert("MODPOW2",        Div::<Signaling>::modpow2);
        self.COMPILE_ROOT.insert("MODPOW2C",       Div::<Signaling>::modpow2c);
        self.COMPILE_ROOT.insert("MODPOW2R",       Div::<Signaling>::modpow2r);
        self.COMPILE_ROOT.insert("MULMODPOW2",     Div::<Signaling>::mulmodpow2);
        self.COMPILE_ROOT.insert("MULMODPOW2C",    Div::<Signaling>::mulmodpow2c);
        self.COMPILE_ROOT.insert("MULMODPOW2R",    Div::<Signaling>::mulmodpow2r);
        self.COMPILE_ROOT.insert("MULRSHIFT",      Div::<Signaling>::mulrshift);
        self.COMPILE_ROOT.insert("MULRSHIFTC",     Div::<Signaling>::mulrshiftc);
        self.COMPILE_ROOT.insert("MULRSHIFTMOD",   Div::<Signaling>::mulrshiftmod);
        self.COMPILE_ROOT.insert("MULRSHIFTMODC",  Div::<Signaling>::mulrshiftmodc);
        self.COMPILE_ROOT.insert("MULRSHIFTMODR",  Div::<Signaling>::mulrshiftmodr);
        self.COMPILE_ROOT.insert("MULRSHIFTR",     Div::<Signaling>::mulrshiftr);
        self.COMPILE_ROOT.insert("POP",            compile_pop);
        self.COMPILE_ROOT.insert("PRINTSTR",       compile_printstr);
        self.COMPILE_ROOT.insert("PUSH",           compile_push);
        self.COMPILE_ROOT.insert("PUSHCONT",       compile_pushcont);
        self.COMPILE_ROOT.insert("PUSHINT",        compile_pushint);
        self.COMPILE_ROOT.insert("PUSHSLICE",      compile_pushslice);
        self.COMPILE_ROOT.insert("SETCONTARGS",    compile_setcontargs);
        self.COMPILE_ROOT.insert("SWAP",           compile_xchg);
        self.COMPILE_ROOT.insert("QLSHIFT",        Div::<Quiet>::lshift);
        self.COMPILE_ROOT.insert("QLSHIFTDIV",     Div::<Quiet>::lshiftdiv);
        self.COMPILE_ROOT.insert("QLSHIFTDIVC",    Div::<Quiet>::lshiftdivc);
        self.COMPILE_ROOT.insert("QLSHIFTDIVMOD",  Div::<Quiet>::lshiftdivmod);
        self.COMPILE_ROOT.insert("QLSHIFTDIVMODC", Div::<Quiet>::lshiftdivmodc);
        self.COMPILE_ROOT.insert("QLSHIFTDIVMODR", Div::<Quiet>::lshiftdivmodr);
        self.COMPILE_ROOT.insert("QLSHIFTDIVR",    Div::<Quiet>::lshiftdivr);
        self.COMPILE_ROOT.insert("QLSHIFTMOD",     Div::<Quiet>::lshiftmod);
        self.COMPILE_ROOT.insert("QLSHIFTMODC",    Div::<Quiet>::lshiftmodc);
        self.COMPILE_ROOT.insert("QLSHIFTMODR",    Div::<Quiet>::lshiftmodr);
        self.COMPILE_ROOT.insert("QMODPOW2",       Div::<Quiet>::modpow2);
        self.COMPILE_ROOT.insert("QMODPOW2C",      Div::<Quiet>::modpow2c);
        self.COMPILE_ROOT.insert("QMODPOW2R",      Div::<Quiet>::modpow2r);
        self.COMPILE_ROOT.insert("QMULMODPOW2",    Div::<Quiet>::mulmodpow2);
        self.COMPILE_ROOT.insert("QMULMODPOW2C",   Div::<Quiet>::mulmodpow2c);
        self.COMPILE_ROOT.insert("QMULMODPOW2R",   Div::<Quiet>::mulmodpow2r);
        self.COMPILE_ROOT.insert("QMULRSHIFT",     Div::<Quiet>::mulrshift);
        self.COMPILE_ROOT.insert("QMULRSHIFTC",    Div::<Quiet>::mulrshiftc);
        self.COMPILE_ROOT.insert("QMULRSHIFTMOD",  Div::<Quiet>::mulrshiftmod);
        self.COMPILE_ROOT.insert("QMULRSHIFTMODC", Div::<Quiet>::mulrshiftmodc);
        self.COMPILE_ROOT.insert("QMULRSHIFTMODR", Div::<Quiet>::mulrshiftmodr);
        self.COMPILE_ROOT.insert("QMULRSHIFTR",    Div::<Quiet>::mulrshiftr);
        self.COMPILE_ROOT.insert("QRSHIFT",        Div::<Quiet>::rshift);
        self.COMPILE_ROOT.insert("QRSHIFTC",       Div::<Quiet>::rshiftc);
        self.COMPILE_ROOT.insert("QRSHIFTMOD",     Div::<Quiet>::rshiftmod);
        self.COMPILE_ROOT.insert("QRSHIFTMODC",    Div::<Quiet>::rshiftmodc);
        self.COMPILE_ROOT.insert("QRSHIFTMODR",    Div::<Quiet>::rshiftmodr);
        self.COMPILE_ROOT.insert("QRSHIFTR",       Div::<Quiet>::rshiftr);
        self.COMPILE_ROOT.insert("RSHIFT",         Div::<Signaling>::rshift);
        self.COMPILE_ROOT.insert("RSHIFTMOD",      Div::<Signaling>::rshiftmod);
        self.COMPILE_ROOT.insert("RSHIFTMODC",     Div::<Signaling>::rshiftmodc);
        self.COMPILE_ROOT.insert("RSHIFTMODR",     Div::<Signaling>::rshiftmodr);
        self.COMPILE_ROOT.insert("RSHIFTR",        Div::<Signaling>::rshiftr);
        self.COMPILE_ROOT.insert("RSHIFTC",        Div::<Signaling>::rshiftc);
        self.COMPILE_ROOT.insert("SDBEGINS",       compile_sdbegins);
        self.COMPILE_ROOT.insert("SDBEGINSQ",      compile_sdbeginsq);
        self.COMPILE_ROOT.insert("SETCONTARGS",    compile_setcontargs);
        self.COMPILE_ROOT.insert("STSLICECONST",   compile_stsliceconst);
        self.COMPILE_ROOT.insert("THROW",          compile_throw);
        self.COMPILE_ROOT.insert("THROWIF",        compile_throwif);
        self.COMPILE_ROOT.insert("THROWIFNOT",     compile_throwifnot);
        self.COMPILE_ROOT.insert("XCHG",           compile_xchg);
        // Add automatic commands
    }
}
