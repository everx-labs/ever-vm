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
    error::TvmError, 
    executor::{Mask, engine::Engine, types::{Ctx, Instruction, InstructionOptions}},
    stack::StackItem, types::{Exception, Failure}
};
use ton_types::{error, Result, SliceData, types::ExceptionCode};
use std::{cmp, str, sync::Arc};

const STR:   u8 = 0x01;
const HEX:   u8 = 0x02;
const BIN:   u8 = 0x04;
const DEPTH: u8 = 0x08; // integer 1..15
const INDEX: u8 = 0x10; // integer 0..15
const FLUSH: u8 = 0x20; // flush

fn dump_var(item: &StackItem, how: u8) -> String {
    if how.bit(HEX) {
        match item {
            StackItem::None            => String::new(),
            StackItem::Builder(x)      => format!("BC<{:X}>", Arc::as_ref(&x)),
            StackItem::Cell(x)         => format!("C<{:X}>", x),
            StackItem::Continuation(x) => format!("R<{:X}>", x.code().cell()),
            StackItem::Integer(x)      => format!("{:X}", Arc::as_ref(&x)),
            StackItem::Slice(x)        => format!("CS<{:X}>({}..{})", &x.cell(), x.pos(), x.pos() + x.remaining_bits()),
            StackItem::Tuple(x)        => format!("({})", x.iter().map(|v| dump_var(v, how)).collect::<Vec<_>>().join(", ")),
        }
    } else if how.bit(BIN) {
        match item {
            StackItem::None            => String::new(),
            StackItem::Builder(x)      => format!("BC<{:b}>", Arc::as_ref(&x)),
            StackItem::Cell(x)         => format!("C<{:b}>", x),
            StackItem::Continuation(x) => format!("R<{:b}>", x.code().cell()),
            StackItem::Integer(x)      => format!("{:b}", Arc::as_ref(&x)),
            StackItem::Slice(x)        => format!("CS<{:b}>({}..{})", x.cell(), x.pos(), x.pos() + x.remaining_bits()),
            StackItem::Tuple(x)        => format!("({})", x.iter().map(|v| dump_var(v, how)).collect::<Vec<_>>().join(", ")),
        }
    } else if how.bit(STR) {
        let string = match item {
            StackItem::None            => return String::new(),
            StackItem::Builder(x)      => x.data().to_vec(),
            StackItem::Cell(x)         => SliceData::from(x).get_bytestring(0),
            StackItem::Continuation(x) => x.code().get_bytestring(0),
            StackItem::Integer(x)      => return format!("{}", Arc::as_ref(&x)),
            StackItem::Slice(x)        => x.get_bytestring(0),
            StackItem::Tuple(x)        => return format!("({})", x.iter().map(|v| dump_var(v, how)).collect::<Vec<_>>().join(", ")),
        };
        String::from_utf8(string).unwrap_or_else(|_| String::new())
    } else {
        match item {
            StackItem::None            => String::new(),
            StackItem::Builder(x)      => format!("BC<{:X}>", Arc::as_ref(&x)),
            StackItem::Cell(x)         => format!("C<{:X}>", x),
            StackItem::Continuation(x) => format!("R<{:X}>", x.code().cell()),
            StackItem::Integer(x)      => format!("{}", Arc::as_ref(&x)),
            StackItem::Slice(x)        => format!("CS<{:X}>({}..{})", x.cell(), x.pos(), x.pos() + x.remaining_bits()),
            StackItem::Tuple(x)        => format!("({})", x.iter().map(|v| dump_var(v, how)).collect::<Vec<_>>().join(", ")),
        }
    }
}
/// dumps stack vars using internal fn dump_var 
fn dump_stack(ctx: Ctx, depth: usize, print_depth: bool) -> Result<Ctx> {
    for i in 0..depth {
        let dump = dump_var(ctx.engine.cc.stack.get(i), 0) + "\n";
        ctx.engine.dump(dump);
    }
    if print_depth {
        ctx.engine.dump(format!("{}\n", depth));
    }
    ctx.engine.flush();
    Ok(ctx)
}
/// internal dump with how and closure
fn internal_dump<F>(engine: &mut Engine, name: &'static str, how: u8, op: F) -> Failure
where F: FnOnce(Ctx) -> Result<Ctx> {
    let mut instruction = Instruction::new(name);
    if how.bit(DEPTH) {
        instruction = instruction.set_opts(InstructionOptions::Integer(1..15))
    }
    if how.bit(INDEX) {
        instruction = instruction.set_opts(InstructionOptions::Integer(0..15))
    }
    engine.load_instruction(instruction)
    .and_then(|ctx| if ctx.engine.debug() {
        op(ctx)
    } else {
        Ok(ctx)
    })
    .map(|ctx| if how.bit(FLUSH) {
        ctx.engine.flush();
    })
    .err()
}
/// dumps all the stack 
pub(crate) fn execute_dump_stack(engine: &mut Engine) -> Failure {
    internal_dump(engine, "DUMPSTK", FLUSH, |ctx| {
        let depth = cmp::min(ctx.engine.cc.stack.depth(), 255);
        dump_stack(ctx, depth, true)
    })
}
/// dumps al least top 1..15 registers
pub(crate) fn execute_dump_stack_top(engine: &mut Engine) -> Failure {
    internal_dump(engine, "DUMPSTKTOP", FLUSH | DEPTH, |ctx| {
        let depth = cmp::min(ctx.engine.cc.stack.depth(), ctx.engine.cmd.integer() as usize);
        dump_stack(ctx, depth, false)
    })
}
/// buffers s0 as hex
pub(crate) fn execute_print_hex(engine: &mut Engine) -> Failure {
    internal_dump(engine, "HEXPRINT", 0, |ctx| {
        if ctx.engine.cc.stack.depth() > 0 {
            let dump = dump_var(ctx.engine.cc.stack.get(0), HEX);
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// buffers s0 as binary
pub(crate) fn execute_print_bin(engine: &mut Engine) -> Failure {
    internal_dump(engine, "BINPRINT", 0, |ctx| {
        if ctx.engine.cc.stack.depth() > 0 {
            let dump = dump_var(ctx.engine.cc.stack.get(0), BIN);
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// buffers s0 as string
pub(crate) fn execute_print_str(engine: &mut Engine) -> Failure {
    internal_dump(engine, "STRPRINT", 0, |ctx| {
        if ctx.engine.cc.stack.depth() > 0 {
            let dump = dump_var(ctx.engine.cc.stack.get(0), STR);
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// dumps s0 as hex
pub(crate) fn execute_dump_hex(engine: &mut Engine) -> Failure {
    internal_dump(engine, "HEXDUMP", FLUSH, |ctx| {
        if ctx.engine.cc.stack.depth() > 0 {
            let dump = dump_var(ctx.engine.cc.stack.get(0), HEX) + "\n";
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// dumps s0 as binary
pub(crate) fn execute_dump_bin(engine: &mut Engine) -> Failure {
    internal_dump(engine, "BINDUMP", FLUSH, |ctx| {
        if ctx.engine.cc.stack.depth() > 0 {
            let dump = dump_var(ctx.engine.cc.stack.get(0), BIN) + "\n";
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// dumps s0 as string
pub(crate) fn execute_dump_str(engine: &mut Engine) -> Failure {
    internal_dump(engine, "STRDUMP", FLUSH, |ctx| {
        if ctx.engine.cc.stack.depth() > 0 {
            let dump = dump_var(ctx.engine.cc.stack.get(0), STR) + "\n";
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// turns debug output on
pub(crate) fn execute_debug_on(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("DEBUGON"))
    .map(|ctx| ctx.engine.switch_debug(true))
    .err()
}
/// turns debug output off
pub(crate) fn execute_debug_off(engine: &mut Engine) -> Failure {
    engine.load_instruction(Instruction::new("DEBUGOFF"))
    .map(|ctx| ctx.engine.switch_debug(true))
    .err()
}
/// dumps s(n)
pub(crate) fn execute_dump_var(engine: &mut Engine) -> Failure {
    internal_dump(engine, "DUMP", FLUSH | INDEX, |ctx| {
        let index = ctx.engine.cmd.integer() as usize;
        if index < ctx.engine.cc.stack.depth() {
            let dump = format!("{}\n", ctx.engine.cc.stack.get(index));
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}
/// prints s(n)
pub(crate) fn execute_print_var(engine: &mut Engine) -> Failure {
    internal_dump(engine, "PRINT", INDEX, |ctx| {
        let index = ctx.engine.cmd.integer() as usize;
        if index < ctx.engine.cc.stack.depth() {
            let dump = format!("{}\n", ctx.engine.cc.stack.get(index));
            ctx.engine.dump(dump);
        }
        Ok(ctx)
    })
}

fn internal_dump_string<F>(engine: &mut Engine, name: &'static str, how: u8, op: F) -> Failure
where F: FnOnce(Ctx, String) -> Result<Ctx> {
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::Bytestring(12, 0, 4, 1))
    )
    .and_then(|ctx| if let Ok(string) = String::from_utf8(ctx.engine.cmd.slice().get_bytestring(8)) {
        if ctx.engine.debug() {
            op(ctx, string)
        } else {
            Ok(ctx)
        }
    } else {
        err!(ExceptionCode::InvalidOpcode)
    })
    .map(|ctx| if how.bit(FLUSH) {
        ctx.engine.flush();
    })
    .err()
}

pub(crate) fn execute_dump_string(engine: &mut Engine) -> Failure {
    let length = 1 + (0x0F & engine.cc.last_cmd() as usize);
    match engine.cc.next_cmd() {
        Ok(0) if length == 1 => internal_dump_string(engine, "LOGFLUSH", FLUSH, |ctx, _string| {
            Ok(ctx)
        }),
        Ok(0) => internal_dump_string(engine, "LOGSTR", 0, |ctx, string| {
            ctx.engine.dump(string);
            Ok(ctx)
        }),
        Ok(1) => internal_dump_string(engine, "PRINTSTR", FLUSH, |ctx, string| {
            ctx.engine.dump(string);
            Ok(ctx)
        }),
        // TODO: dump s0 as TL-B supported type
        Ok(_) => internal_dump_string(engine, "DUMPTOSFMT", 0, |ctx, string| {
            ctx.engine.dump(string);
            Ok(ctx)
        }),
        Err(err) => Some(err)
    }
}

