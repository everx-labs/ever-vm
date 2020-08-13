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
    executor::{
        engine::{
            Engine, data::convert, 
            storage::{fetch_stack, fetch_reference, copy_to_var, swap}
        },
        microcode::{CTRL, CC, CELL, SAVELIST, VAR, SLICE, CONTINUATION}, 
        types::{WhereToGetParams, InstructionOptions, Instruction}
    },
    stack::{
        StackItem, continuation::ContinuationData, 
        integer::{IntegerData, behavior::Signaling}
    },
    types::{Exception, Failure}
};
use std::{cmp, usize, sync::Arc};
use ton_types::{error, types::ExceptionCode};

// Stack manipulation *********************************************************

// (xi ... x1 - )
pub(super) fn execute_blkdrop(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("BLKDROP").set_opts(InstructionOptions::Length(0..16))
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.drop_range(0..ctx.engine.cmd.length())?;
        Ok(ctx)
    })
    .err()
}

pub(super) fn execute_blkdrop2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("BLKDROP2").set_opts(InstructionOptions::LengthAndIndex)
    )
    .and_then(|ctx| {
        let length = ctx.engine.cmd.length_and_index().length;
        let index = ctx.engine.cmd.length_and_index().index;
        ctx.engine.cc.stack.drop_range(index..index + length)?;
        Ok(ctx)
    })
    .err()
}

// (x(j) ... - x(j) ... { x(j) } i times)
pub(super) fn execute_blkpush(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("BLKPUSH").set_opts(InstructionOptions::LengthAndIndex)
    )
    .and_then(|ctx| {
        let length = ctx.engine.cmd.length_and_index().length;
        let index = ctx.engine.cmd.length_and_index().index;
        if ctx.engine.cc.stack.depth() <= index {
            err!(ExceptionCode::StackUnderflow)
        } else {
            for _ in 0..length {
                ctx.engine.cc.stack.push_copy(index)?;
            }
            Ok(ctx)
        }
    })
    .err()
}

// (a(j+i-1)...a(j) a(j-1)...a(0) - a(j-1)...a(0) a(j+i-1)..a(j))
// Example: BLKSWAP 2, 4:
// (8 7 6 {5 4} {3 2 1 0} - 8 7 6 {3 2 1 0} {5 4})
pub(super) fn execute_blkswap(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("BLKSWAP").set_opts(
            InstructionOptions::LengthMinusOneAndIndexMinusOne
        )
    ).and_then(|ctx| {
        let i = ctx.engine.cmd.length_and_index().length;
        let j = ctx.engine.cmd.length_and_index().index;
        ctx.engine.cc.stack.block_swap(i, j)?;
        Ok(ctx)
    })
    .err()
}

// (a(j+i+1)...a(j+2) a(j+1)...a(2) j i - a(j+1)...a(2) a(j+i+1)...a(j+2))
pub(super) fn execute_blkswx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("BLKSWX")
    )
    .and_then(|ctx| fetch_stack(ctx, 2) )
    .and_then(|ctx| {
        let j = ctx.engine.cmd.var(0).as_integer()?.into(1..=usize::MAX)?;
        let i = ctx.engine.cmd.var(1).as_integer()?.into(1..=usize::MAX)?;
        ctx.engine.cc.stack.block_swap(i, j)?;
        Ok(ctx)
    })
    .err()
}

// (i - ), throws exception if depth < i
pub(super) fn execute_chkdepth(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("CHKDEPTH")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        if ctx.engine.cc.stack.depth() < i {
            return err!(ExceptionCode::StackUnderflow)
        }
        Ok(ctx)
    })
    .err()
}

// ( - stack_depth)
pub(super) fn execute_depth(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("DEPTH")
    )
    .and_then(|ctx| {
        let data = ctx.engine.cc.stack.depth();
        ctx.engine.cc.stack.push(int!(data));
        Ok(ctx)
    })
    .err()
}

// (a(i)...a(1) i - )
pub(super) fn execute_dropx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("DROPX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        if ctx.engine.cc.stack.depth() < i {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.drop_top(i);
        Ok(ctx)
    })
    .err()
}

// (a b - )
pub(super) fn execute_drop2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("DROP2")
    )
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 2 {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.drop_top(2);
        Ok(())
    })
    .err()
}

// (a b - a b a b)
pub(super) fn execute_dup2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("DUP2")
    )
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 2 {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.push_copy(1)?;
        ctx.engine.cc.stack.push_copy(1)?;
        Ok(())
    })
    .err()
}

// ( ... a(i)...a(1) i - a(i)...a(1))
pub(super) fn execute_onlytopx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ONLYTOPX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        let depth = ctx.engine.cc.stack.depth();
        if depth < i {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.drop_range(i..depth)?;
        Ok(ctx)
    })
    .err()
}

// (a(depth)...a(depth-i+1) ... i - a(depth)...a(depth-i+1))
pub(super) fn execute_onlyx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ONLYX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        let depth = ctx.engine.cc.stack.depth();
        if depth < i {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.drop_top(depth - i);
        Ok(ctx)
    })
    .err()
}

// (a b c d - a b c d a b)
pub(super) fn execute_over2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("OVER2")
    )
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 4 {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.push_copy(3)?;
        ctx.engine.cc.stack.push_copy(3)?;
        Ok(ctx)
    })
    .err()
}

// (i - s(i))
pub(super) fn execute_pick(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PICK")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        if ctx.engine.cc.stack.depth() <= i {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.push_copy(i)?;
            Ok(ctx)
        }
    })
    .err()
}

// (x ... y - y ...)
pub(super) fn execute_pop(engine: &mut Engine) -> Failure {
    let cmd = engine.cc.last_cmd();
    let range = if (cmd & 0xF0) == 0x30 {
        0..16
    } else if cmd == 0x57 {
        0..256
    } else {
        return Some(error!("execute_pop cmd: {:X}", cmd))
    };
    engine.load_instruction(
        Instruction::new("POP").set_opts(InstructionOptions::StackRegister(range))
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.swap(0, ctx.engine.cmd.sreg())?;
        ctx.engine.cc.stack.drop(0)?;
        Ok(ctx)
    })
    .err()
}

// (x - ), c[i] = x
pub(super) fn execute_popctr(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("POPCTR").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|ctx| {
        let creg = ctx.engine.cmd.creg();
        swap(ctx, var!(0), ctrl!(creg))
    })
    .err()
}

// (x i - ), c[i] = x
pub(super) fn execute_popctrx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("POPCTRX")
    )
    .and_then(|ctx| fetch_stack(ctx, 2))
    .and_then(|ctx| {
        let creg = ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?;
        swap(ctx, var!(0), ctrl!(creg))
    })
    .err()
}

// (x - ), c[0].savelist[i] = c[i], c[i] = x, 
pub(super) fn execute_popsave(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("POPSAVE").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|mut ctx| {
        let creg = ctx.engine.cmd.creg();
        ctx = swap(ctx, var!(0), ctrl!(creg))?;
        swap(ctx, var!(0), savelist!(ctrl!(0), 0))
    })
    .err()
}

// (x ... y ... z ... a - a... y ... z ... z y x)
// PU2XC s(i), s(j-1), s(k-2), equal to PUSH s(i); SWAP; PUSH s(j); SWAP; XCHG s(k)
pub(super) fn execute_pu2xc(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PU2XC")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusTwo))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc; 
        if ctx.engine.cc.stack.depth() + 1 < cmp::max(rc, cmp::max(ra + 2, rb + 1)) {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.push_copy(ra)?;
        ctx.engine.cc.stack.swap(0, 1)?;
        ctx.engine.cc.stack.push_copy(rb)?;
        ctx.engine.cc.stack.swap(0, 1)?;
        ctx.engine.cc.stack.swap(0, rc)?;
        Ok(ctx)
    })
    .err()
}

// (x ... - x ... x)
pub(super) fn execute_push(engine: &mut Engine) -> Failure {
    let cmd = engine.cc.last_cmd();
    let range = if (cmd & 0xF0) == 0x20 {
        0..16
    } else if cmd == 0x56 {
        0..256
    } else {
        return Some(error!("execute_push: cmd {:X}", cmd))
    };
    engine.load_instruction(
        Instruction::new("PUSH").set_opts(InstructionOptions::StackRegister(range))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sreg();
        if ctx.engine.cc.stack.depth() <= ra {
            return err!(ExceptionCode::StackUnderflow)
        } 
        ctx.engine.cc.stack.push_copy(ra)?;
        Ok(ctx)
    })
    .err()
}

// (x ... y ... - x ... y ... x y)
pub(super) fn execute_push2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSH2")
            .set_opts(
                InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
            )
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs().ra;
        let rb = ctx.engine.cmd.sregs().rb;
        if ctx.engine.cc.stack.depth() <= cmp::max(ra, rb) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.push_copy(ra)?;
            ctx.engine.cc.stack.push_copy(rb + 1)?;
            Ok(ctx)
        }
    })
    .err()
}

// (x ... y ... z ...  - x ... y ... z... x y z)
pub(super) fn execute_push3(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSH3")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc;
        if ctx.engine.cc.stack.depth() <= cmp::max(cmp::max(ra, rb), rc) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.push_copy(ra)?;
            ctx.engine.cc.stack.push_copy(rb + 1)?;
            ctx.engine.cc.stack.push_copy(rc + 2)?;
            Ok(ctx)
        }
    })
    .err()
}

fn execute_pushcont(engine: &mut Engine, opts: InstructionOptions) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHCONT").set_opts(opts)
    )
    .and_then(|ctx| {
        let slice = ctx.engine.cmd.slice().clone();
        ctx.engine.cc.stack.push_cont(ContinuationData::with_code(slice));
        Ok(ctx)
    })
    .err()
}

// ( - continuation)
pub(super) fn execute_pushcont_short(engine: &mut Engine) -> Failure {
    execute_pushcont(engine, InstructionOptions::Bytestring(7, 2, 7, 0))
}

// ( - continuation)
pub(super) fn execute_pushcont_long(engine: &mut Engine) -> Failure {
    execute_pushcont(engine, InstructionOptions::Bytestring(4, 0, 4, 0))
}

// ( - c[i])
pub(super) fn execute_pushctr(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHCTR").set_opts(InstructionOptions::ControlRegister)
    )
    .and_then(|mut ctx| {
        let creg = ctx.engine.cmd.creg();
        ctx = copy_to_var(ctx, ctrl!(creg))?;
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
        Ok(ctx)
    })
    .err()
}

// (i - c[i])
pub(super) fn execute_pushctrx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHCTRX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1))
    .and_then(|mut ctx| {
        let creg = ctx.engine.cmd.var(0).as_integer()?.into(0..=255)?;
        ctx = copy_to_var(ctx, ctrl!(creg))?;
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.pop().unwrap());
        Ok(ctx)
    })
    .err()
}

// ( - int)
pub(super) fn execute_pushint(engine: &mut Engine) -> Failure {
    let cmd = engine.cc.last_cmd();
    let range = if (cmd & 0xF0) == 0x70 {
        -5..11
    } else if cmd == 0x80 {
        -128..128
    } else if cmd == 0x81 {
        -32768..32768
    } else {
        return err_opt!(ExceptionCode::InvalidOpcode);
    };
    engine.load_instruction(
        Instruction::new("PUSHINT").set_opts(InstructionOptions::Integer(range))
    )
    .and_then(|ctx| {
        let num = ctx.engine.cmd.integer();
        ctx.engine.cc.stack.push(int!(num));
        Ok(ctx)
    })
    .err()
}

// ( - int)
pub(super) fn execute_pushint_big(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHINT").set_opts(InstructionOptions::BigInteger)
    )
    .and_then(|ctx| {
        let num = ctx.engine.cmd.biginteger_mut();
        ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(num.withdraw())));
        Ok(ctx)
    })
    .err()
}

// ( - NaN)
pub(super) fn execute_pushnan(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHNAN")
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(int!(nan));
        Ok(ctx)
    })
    .err()
}

// ( - int = -2^(x+1))
pub(super) fn execute_pushnegpow2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHNEGPOW2")
            .set_opts(
                InstructionOptions::LengthMinusOne(0..256)
            )	
    )
    .and_then(|ctx| {
        let power = ctx.engine.cmd.length();
        ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(
            IntegerData::minus_one()
                .shl::<Signaling>(power)?
        )));
        Ok(ctx)
    })
    .err()
}

// ( - 2^(x+1))
pub(super) fn execute_pushpow2(engine: &mut Engine) -> Failure {
    let power = engine.cc.last_cmd();
    engine.load_instruction(
        Instruction::new("PUSHPOW2")
    )
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(
            IntegerData::one().shl::<Signaling>(power as usize + 1)?
        )));
        Ok(ctx)
    })
    .err()
}

// ( - int = 2^(x+1)-1)
pub(super) fn execute_pushpow2dec(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHPOW2DEC")
            .set_opts(
                InstructionOptions::LengthMinusOne(0..256)
            )	
    )
    .and_then(|ctx| {
        let power = ctx.engine.cmd.length();
        ctx.engine.cc.stack.push(StackItem::Integer(Arc::new(
            IntegerData::one()
                .shl::<Signaling>(power - 1)?
                .sub::<Signaling>(&IntegerData::one())?
                .shl::<Signaling>(1)?
                .add::<Signaling>(&IntegerData::one())?
        )));
        Ok(ctx)
    })
    .err()
}

fn fetch_ref(engine: &mut Engine, name: &'static str, to: u16) -> Failure {
    engine.load_instruction(
        Instruction::new(name)
    )
    .and_then(|ctx| fetch_reference(ctx, CC))
    .and_then(|ctx| if to != CELL {
        convert(ctx, var!(0), to, CELL)
    } else {
        Ok(ctx)
    })
    .and_then(|ctx| {
        ctx.engine.cc.stack.push(ctx.engine.cmd.vars.remove(0));
        Ok(ctx)
    })
    .err()
}

// ( - Cell) from cc references[0]
pub(super) fn execute_pushref(engine: &mut Engine) -> Failure {
    fetch_ref(engine, "PUSHREF", CELL)
}

// ( - Continuation) from cc references[0] 
pub(super) fn execute_pushrefcont(engine: &mut Engine) -> Failure {
    fetch_ref(engine, "PUSHREFCONT", CONTINUATION)
}

// ( - Slice) from cc references[0]
pub(super) fn execute_pushrefslice(engine: &mut Engine) -> Failure {
    fetch_ref(engine, "PUSHREFSLICE", SLICE)
}

fn execute_pushslice(engine: &mut Engine, opts: InstructionOptions) -> Failure {
    engine.load_instruction(
        Instruction::new("PUSHSLICE").set_opts(opts)
    )
    .and_then(|ctx| {
        let slice = ctx.engine.cmd.slice().clone();
        ctx.engine.cc.stack.push(StackItem::Slice(slice));
        Ok(ctx)
    })
    .err()
}

// ( - slice)
pub(super) fn execute_pushslice_short(engine: &mut Engine) -> Failure {
    execute_pushslice(engine, InstructionOptions::Bitstring(8, 0, 4, 0))
}

// ( - slice)
pub(super) fn execute_pushslice_mid(engine: &mut Engine) -> Failure {
    execute_pushslice(engine, InstructionOptions::Bitstring(8, 2, 5, 1))
}

// ( - slice)
pub(super) fn execute_pushslice_long(engine: &mut Engine) -> Failure {
    execute_pushslice(engine, InstructionOptions::Bitstring(8, 3, 7, 0))
}

// (x ... y ... a - a ... y ... y x)
// PUXC s(i), s(j-1), equivalent to PUSH s(i); SWAP; XCHG s(j)
pub(super) fn execute_puxc(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUXC")
            .set_opts(
                InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
            )
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs().ra;
        let rb = ctx.engine.cmd.sregs().rb;
        if ctx.engine.cc.stack.depth() < cmp::max(ra + 1, rb) {
            return err!(ExceptionCode::StackUnderflow)
        } 
        ctx.engine.cc.stack.push_copy(ra)?;
        ctx.engine.cc.stack.swap(0, 1)?;
        ctx.engine.cc.stack.swap(0, rb)?;
        Ok(ctx)
    })
    .err()
}

// (x ... y ... z ... a b - a ... b ... z ... z y x)
// PUXC2 s(i), s(j-1), s(k-1): equivalent to PUSH s(i); XCHG s2; XCHG2 s(j), s(k)
pub(super) fn execute_puxc2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUXC2")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusOne))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc;
        if ctx.engine.cc.stack.depth() < cmp::max(2, cmp::max(cmp::max(ra + 1, rb), rc)) {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.push_copy(ra)?;
        ctx.engine.cc.stack.swap(2, 0)?;
        ctx.engine.cc.stack.swap(1, rb)?;
        ctx.engine.cc.stack.swap(0, rc)?;
        Ok(ctx)
    })
    .err()
}

// (x ... y ... z ... a - x ... a ... z ... z y x)
// PUXCPU s(i), s(j-1), s(k-1): equivalent to PUSH s(i); SWAP; XCHG s(j); PUSH s(k)
pub(super) fn execute_puxcpu(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("PUXCPU")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusOne))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc; 
        if ctx.engine.cc.stack.depth() < cmp::max(rc, cmp::max(ra + 1, rb)) {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.push_copy(ra)?;
        ctx.engine.cc.stack.swap(0, 1)?;
        ctx.engine.cc.stack.swap(0, rb)?;
        ctx.engine.cc.stack.push_copy(rc)?;
        Ok(ctx)
    })
    .err()
}

// (a(j+i-1)...a(j) ... - a(j)...a(j+i-1) ...)
pub(super) fn execute_reverse(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("REVERSE").set_opts(
            InstructionOptions::LengthMinusTwoAndIndex
        )
    )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.length_and_index().length;
        let j = ctx.engine.cmd.length_and_index().index;
        ctx.engine.cc.stack.reverse_range(j..j + i)?;
        Ok(ctx)
    })
    .err()
}

// (a(j+i+1)...a(j+2) ... j i - a(j+2)...a(j+i+1) ...)
pub(super) fn execute_revx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("REVX")
    )
    .and_then(|ctx| fetch_stack(ctx, 2) )
    .and_then(|ctx| {
        let j = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        let i = ctx.engine.cmd.var(1).as_integer()?.into(2..=usize::MAX)?;
        ctx.engine.cc.stack.reverse_range(j..j + i)?;
        Ok(ctx)
    })
    .err()
}

// (x a(i)...a(1) i - a(i)...a(1) x)
pub(super) fn execute_roll(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ROLLX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(1..=usize::MAX)?;
        if ctx.engine.cc.stack.depth() <= i {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let x = ctx.engine.cc.stack.drop(i)?;
            ctx.engine.cc.stack.push(x);
            Ok(ctx)
        }
    })
    .err()
}

// (a(i+1)...a(2) x i - x a(i+1)...a(2))
pub(super) fn execute_rollrev(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ROLLREVX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(1..=usize::MAX)?;
        if ctx.engine.cc.stack.depth() <= i {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let x = ctx.engine.cc.stack.drop(0)?;
            ctx.engine.cc.stack.insert(i, x);
            Ok(ctx)
        }
    })
    .err()
}

// (a b c - b c a)
pub(super) fn execute_rot(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ROT")
    )
    .and_then(|ctx| {
        let top = ctx.engine.cc.stack.drop(2)?;
        ctx.engine.cc.stack.push(top);
        Ok(ctx)
    })
    .err()
}

// (a b c - c a b)
pub(super) fn execute_rotrev(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("ROTREV")
    )
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 3 {
            err!(ExceptionCode::StackUnderflow)
        } else {
            let top = ctx.engine.cc.stack.drop(0)?;
            ctx.engine.cc.stack.insert(2, top);
            Ok(ctx)
        }
    })
    .err()
}

// (a b c d - c d a b)
pub(super) fn execute_swap2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("SWAP2")
    )
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 4 {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.block_swap(2, 2)
        }
    })
    .err()
}

// (x y - y x y)
pub(super) fn execute_tuck(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("TUCK")
    )
    .and_then(|ctx| {
        if ctx.engine.cc.stack.depth() < 2 {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.push_copy(0)?;
        ctx.engine.cc.stack.swap(1, 2)?;
        Ok(ctx)
    })
    .err()
}

// (x ... y ... z ... a b - x ... a ... b ... z y x)
// XC2PU s(i), s(j), s(k): equivalent to XCHG2 s(i), s(j); PUSH s(k)
pub(super) fn execute_xc2pu(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XC2PU")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc;
        if ctx.engine.cc.stack.depth() <= cmp::max(1, cmp::max(ra, cmp::max(rb, rc))) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.swap(1, ra)?;
            ctx.engine.cc.stack.swap(0, rb)?;
            ctx.engine.cc.stack.push_copy(rc)?;
            Ok(ctx)
        }
    })
    .err()
}

// (x ... y ... - y ... x ...)
pub(super) fn execute_xchg(engine: &mut Engine, opts: InstructionOptions) -> Failure {
    engine.load_instruction(
        Instruction::new("XCHG").set_opts(opts)
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs().ra;
        let rb = ctx.engine.cmd.sregs().rb;
        ctx.engine.cc.stack.swap(ra, rb)?;
        Ok(ctx)
    })
    .err()
}

// XCHG addressing via the same instruction byte
pub(super) fn execute_xchg_simple(engine: &mut Engine) -> Failure {
    execute_xchg(
        engine, 
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromLastByte)
    )
}

// XCHG addressing via the next instruction byte
pub(super) fn execute_xchg_std(engine: &mut Engine) -> Failure {
    execute_xchg(
        engine, 
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
    )
}

// XCHG addressing via the next instruction byte, long index
pub(super) fn execute_xchg_long(engine: &mut Engine) -> Failure {
    execute_xchg(
        engine,  
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByteLong)
    )
}

// (x ... y ... a b - a ... b ... x y)
// XCHG s(1),s(i); XCHG s(0),s(j).
pub(super) fn execute_xchg2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XCHG2").set_opts(
            InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
        )
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs().ra;
        let rb = ctx.engine.cmd.sregs().rb;
        if ctx.engine.cc.stack.depth() <= cmp::max(1, cmp::max(ra, rb)) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.swap(1, ra)?;
            ctx.engine.cc.stack.swap(0, rb)?;
            Ok(ctx)
        }
    })
    .err()
}

// (x ... y ... z ... a b c - c ... b ... a ... z y x)
// XCHG s(2), s(i); XCHG s(1) s(j); XCHG s(0), s(k)
pub(super) fn execute_xchg3(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XCHG3")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc;
        if ctx.engine.cc.stack.depth() <= cmp::max(2, cmp::max(rc, cmp::max(ra, rb))) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.swap(2, ra)?;
            ctx.engine.cc.stack.swap(1, rb)?;
            ctx.engine.cc.stack.swap(0, rc)?;
            Ok(ctx)
        }
    })
    .err()
}

// (a(i+1)...a(1) i - a(1)...a(i+1))
pub(super) fn execute_xchgx(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XCHGX")
    )
    .and_then(|ctx| fetch_stack(ctx, 1) )
    .and_then(|ctx| {
        let i = ctx.engine.cmd.var(0).as_integer()?.into(0..=usize::MAX)?;
        ctx.engine.cc.stack.swap(0, i)?;
        Ok(ctx)
    })
    .err()
}

// (x ... y ... a - x ... a ... y x)
// XCHG s(i), PUSH s(j)
pub(super) fn execute_xcpu(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XCPU").set_opts(
            InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
        )
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs().ra;
        let rb = ctx.engine.cmd.sregs().rb;
        if ctx.engine.cc.stack.depth() <= cmp::max(ra, rb) {
            err!(ExceptionCode::StackUnderflow)
        } else {
            ctx.engine.cc.stack.swap(0, ra)?;
            ctx.engine.cc.stack.push_copy(rb)?;
            Ok(ctx)
        }
    })
    .err()
}

// (x ... y ... z ... a - x ... y ... a ... z y x)
// XCHG s(i), PUSH s(j), PUSH s(k+1)
pub(super) fn execute_xcpu2(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XCPU2")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc;
        if ctx.engine.cc.stack.depth() <= cmp::max(1, cmp::max(ra, cmp::max(rb, rc))) {
            return err!(ExceptionCode::StackUnderflow);
        }
        ctx.engine.cc.stack.swap(0, ra)?;
        ctx.engine.cc.stack.push_copy(rb)?;
        ctx.engine.cc.stack.push_copy(rc + 1)?;
        Ok(ctx)
    })
    .err()
}

// (x ... y ... z ... a b - b ... y ... a ... z y x)
// XCPUXC s(i), s(j), s(k-1): equavalent to XCHG s(1), s(i); PUSH s(j); SWAP; XCHG s(k)
pub(super) fn execute_xcpuxc(engine: &mut Engine) -> Failure {
    engine.load_instruction(
        Instruction::new("XCPUXC")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOne))
    )
    .and_then(|ctx| {
        let ra = ctx.engine.cmd.sregs3().ra;
        let rb = ctx.engine.cmd.sregs3().rb;
        let rc = ctx.engine.cmd.sregs3().rc;
        if ctx.engine.cc.stack.depth() < cmp::max(2, cmp::max(rc, cmp::max(ra, rb) + 1)) {
            return err!(ExceptionCode::StackUnderflow)
        }
        ctx.engine.cc.stack.swap(1, ra)?;
        ctx.engine.cc.stack.push_copy(rb)?;
        ctx.engine.cc.stack.swap(0, 1)?;
        ctx.engine.cc.stack.swap(0, rc)?;
        Ok(ctx)
    })
    .err()
}
