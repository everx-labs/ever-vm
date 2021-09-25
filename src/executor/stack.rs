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
    types::{Exception, Status}
};
use std::{cmp, sync::Arc};
use ton_types::{error, fail, types::ExceptionCode};

// Stack manipulation *********************************************************

// (xi ... x1 - )
pub(super) fn execute_blkdrop(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("BLKDROP").set_opts(InstructionOptions::Length(0..16))
    )?;
    engine.cc.stack.drop_range(0..engine.cmd.length())?;
    Ok(())
}

pub(super) fn execute_blkdrop2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("BLKDROP2").set_opts(InstructionOptions::LengthAndIndex)
    )?;
    let length = engine.cmd.length_and_index().length;
    let index = engine.cmd.length_and_index().index;
    engine.cc.stack.drop_range(index..index + length)?;
    Ok(())
}

// (x(j) ... - x(j) ... { x(j) } i times)
pub(super) fn execute_blkpush(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("BLKPUSH").set_opts(InstructionOptions::LengthAndIndex)
    )?;
    let length = engine.cmd.length_and_index().length;
    let index = engine.cmd.length_and_index().index;
    if engine.cc.stack.depth() <= index {
        err!(ExceptionCode::StackUnderflow)
    } else {
        for _ in 0..length {
            engine.cc.stack.push_copy(index)?;
        }
        Ok(())
    }
}

// (a(j+i-1)...a(j) a(j-1)...a(0) - a(j-1)...a(0) a(j+i-1)..a(j))
// Example: BLKSWAP 2, 4:
// (8 7 6 {5 4} {3 2 1 0} - 8 7 6 {3 2 1 0} {5 4})
pub(super) fn execute_blkswap(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("BLKSWAP").set_opts(
            InstructionOptions::LengthMinusOneAndIndexMinusOne
        )
    )?;
    let i = engine.cmd.length_and_index().length;
    let j = engine.cmd.length_and_index().index;
    engine.cc.stack.block_swap(i, j)?;
    Ok(())
}

// (a(j+i+1)...a(j+2) a(j+1)...a(2) j i - a(j+1)...a(2) a(j+i+1)...a(j+2))
pub(super) fn execute_blkswx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("BLKSWX")
    )?;
    fetch_stack(engine, 2)?;
    let j = engine.cmd.var(0).as_integer()?.into(1..=255)?;
    let i = engine.cmd.var(1).as_integer()?.into(1..=255)?;
    engine.cc.stack.block_swap(i, j)?;
    Ok(())
}

// (i - ), throws exception if depth < i
pub(super) fn execute_chkdepth(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("CHKDEPTH")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    if engine.cc.stack.depth() < i {
        return err!(ExceptionCode::StackUnderflow)
    }
    Ok(())
}

// ( - stack_depth)
pub(super) fn execute_depth(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("DEPTH")
    )?;
    let data = engine.cc.stack.depth();
    engine.cc.stack.push(int!(data));
    Ok(())
}

// (a(i)...a(1) i - )
pub(super) fn execute_dropx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("DROPX")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    if engine.cc.stack.depth() < i {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.drop_top(i);
    Ok(())
}

// (a b - )
pub(super) fn execute_drop2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("DROP2")
    )?;
    if engine.cc.stack.depth() < 2 {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.drop_top(2);
    Ok(())
}

// (a b - a b a b)
pub(super) fn execute_dup2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("DUP2")
    )?;
    if engine.cc.stack.depth() < 2 {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(1)?;
    engine.cc.stack.push_copy(1)?;
    Ok(())
}

// ( ... a(i)...a(1) i - a(i)...a(1))
pub(super) fn execute_onlytopx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ONLYTOPX")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    let depth = engine.cc.stack.depth();
    if depth < i {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.drop_range(i..depth)?;
    Ok(())
}

// (a(depth)...a(depth-i+1) ... i - a(depth)...a(depth-i+1))
pub(super) fn execute_onlyx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ONLYX")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    let depth = engine.cc.stack.depth();
    if depth < i {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.drop_top(depth - i);
    Ok(())
}

// (a b c d - a b c d a b)
pub(super) fn execute_over2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("OVER2")
    )?;
    if engine.cc.stack.depth() < 4 {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(3)?;
    engine.cc.stack.push_copy(3)?;
    Ok(())
}

// (i - s(i))
pub(super) fn execute_pick(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PICK")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    if engine.cc.stack.depth() <= i {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.push_copy(i)?;
        Ok(())
    }
}

// (x ... y - y ...)
fn execute_pop_internal(engine: &mut Engine, name: &'static str) -> Status {
    let cmd = engine.cc.last_cmd();
    let range = if (cmd & 0xF0) == 0x30 {
        0..16
    } else if cmd == 0x57 {
        0..256
    } else {
        fail!("execute_pop cmd: {:X}", cmd)
    };
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::StackRegister(range))
    )?;
    engine.cc.stack.swap(0, engine.cmd.sreg())?;
    engine.cc.stack.drop(0)?;
    Ok(())
}

pub(super) fn execute_drop(engine: &mut Engine) -> Status {
    execute_pop_internal(engine, "DROP")
}

pub(super) fn execute_nip(engine: &mut Engine) -> Status {
    execute_pop_internal(engine, "NIP")
}

pub(super) fn execute_pop(engine: &mut Engine) -> Status {
    execute_pop_internal(engine, "POP")
}


// (x - ), c[i] = x
pub(super) fn execute_popctr(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("POPCTR").set_opts(InstructionOptions::ControlRegister)
    )?;
    fetch_stack(engine, 1)?;
    let creg = engine.cmd.creg();
    swap(engine, var!(0), ctrl!(creg))
}

// (x i - ), c[i] = x
pub(super) fn execute_popctrx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("POPCTRX")
    )?;
    fetch_stack(engine, 2)?;
    let creg = engine.cmd.var(0).as_small_integer()?;
    swap(engine, var!(0), ctrl!(creg))
}

// (x - ), c[0].savelist[i] = c[i], c[i] = x,
pub(super) fn execute_popsave(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("POPSAVE").set_opts(InstructionOptions::ControlRegister)
    )?;
    fetch_stack(engine, 1)?;
    let creg = engine.cmd.creg();
    swap(engine, var!(0), ctrl!(creg))?;
    swap(engine, var!(0), savelist!(ctrl!(0), 0))
}

// (x ... y ... z ... a - a... y ... z ... z y x)
// PU2XC s(i), s(j-1), s(k-2), equal to PUSH s(i); SWAP; PUSH s(j); SWAP; XCHG s(k)
pub(super) fn execute_pu2xc(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PU2XC")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusTwo))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() + 1 < cmp::max(rc, cmp::max(ra + 2, rb + 1)) {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(ra)?;
    engine.cc.stack.swap(0, 1)?;
    engine.cc.stack.push_copy(rb)?;
    engine.cc.stack.swap(0, 1)?;
    engine.cc.stack.swap(0, rc)?;
    Ok(())
}

// (x ... - x ... x)
fn execute_push_internal(engine: &mut Engine, name: &'static str) -> Status {
    let cmd = engine.cc.last_cmd();
    let range = if (cmd & 0xF0) == 0x20 {
        0..16
    } else if cmd == 0x56 {
        0..256
    } else {
        fail!("execute_push: cmd {:X}", cmd)
    };
    engine.load_instruction(
        Instruction::new(name).set_opts(InstructionOptions::StackRegister(range))
    )?;
    let ra = engine.cmd.sreg();
    if engine.cc.stack.depth() <= ra {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(ra)?;
    Ok(())
}

pub(super) fn execute_dup(engine: &mut Engine) -> Status {
    execute_push_internal(engine, "DROP")
}

pub(super) fn execute_over(engine: &mut Engine) -> Status {
    execute_push_internal(engine, "OVER")
}

pub(super) fn execute_push(engine: &mut Engine) -> Status {
    execute_push_internal(engine, "PUSH")
}

// (x ... y ... - x ... y ... x y)
pub(super) fn execute_push2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSH2")
            .set_opts(
                InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
            )
    )?;
    let ra = engine.cmd.sregs().ra;
    let rb = engine.cmd.sregs().rb;
    if engine.cc.stack.depth() <= cmp::max(ra, rb) {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.push_copy(ra)?;
        engine.cc.stack.push_copy(rb + 1)?;
        Ok(())
    }
}

// (x ... y ... z ...  - x ... y ... z... x y z)
pub(super) fn execute_push3(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSH3")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() <= cmp::max(cmp::max(ra, rb), rc) {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.push_copy(ra)?;
        engine.cc.stack.push_copy(rb + 1)?;
        engine.cc.stack.push_copy(rc + 2)?;
        Ok(())
    }
}

fn execute_pushcont(engine: &mut Engine, opts: InstructionOptions) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHCONT").set_opts(opts)
    )?;
    let slice = engine.cmd.slice().clone();
    engine.cc.stack.push_cont(ContinuationData::with_code(slice));
    Ok(())
}

// ( - continuation)
pub(super) fn execute_pushcont_short(engine: &mut Engine) -> Status {
    execute_pushcont(engine, InstructionOptions::Bytestring(7, 2, 7, 0))
}

// ( - continuation)
pub(super) fn execute_pushcont_long(engine: &mut Engine) -> Status {
    execute_pushcont(engine, InstructionOptions::Bytestring(4, 0, 4, 0))
}

// ( - c[i])
pub(super) fn execute_pushctr(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHCTR").set_opts(InstructionOptions::ControlRegister)
    )?;
    let creg = engine.cmd.creg();
    copy_to_var(engine, ctrl!(creg))?;
    engine.cc.stack.push(engine.cmd.pop_var()?);
    Ok(())
}

// (i - c[i])
pub(super) fn execute_pushctrx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHCTRX")
    )?;
    fetch_stack(engine, 1)?;
    let creg = engine.cmd.var(0).as_small_integer()?;
    copy_to_var(engine, ctrl!(creg))?;
    engine.cc.stack.push(engine.cmd.pop_var()?);
    Ok(())
}

// ( - int)
pub(super) fn execute_pushint(engine: &mut Engine) -> Status {
    let cmd = engine.cc.last_cmd();
    let range = if (cmd & 0xF0) == 0x70 {
        -5..11
    } else if cmd == 0x80 {
        -128..128
    } else if cmd == 0x81 {
        -32768..32768
    } else {
        return err!(ExceptionCode::InvalidOpcode);
    };
    engine.load_instruction(
        Instruction::new("PUSHINT").set_opts(InstructionOptions::Integer(range))
    )?;
    let num = engine.cmd.integer();
    engine.cc.stack.push(int!(num));
    Ok(())
}

// ( - int)
pub(super) fn execute_pushint_big(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHINT").set_opts(InstructionOptions::BigInteger)
    )?;
    let num = engine.cmd.biginteger_mut();
    engine.cc.stack.push(StackItem::Integer(Arc::new(num.withdraw())));
    Ok(())
}

// ( - NaN)
pub(super) fn execute_pushnan(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHNAN")
    )?;
    engine.cc.stack.push(int!(nan));
    Ok(())
}

// ( - int = -2^(x+1))
pub(super) fn execute_pushnegpow2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHNEGPOW2")
            .set_opts(
                InstructionOptions::LengthMinusOne(0..256)
            )
    )?;
    let power = engine.cmd.length();
    engine.cc.stack.push(StackItem::Integer(Arc::new(
        IntegerData::minus_one()
            .shl::<Signaling>(power)?
    )));
    Ok(())
}

// ( - 2^(x+1))
pub(super) fn execute_pushpow2(engine: &mut Engine) -> Status {
    let power = engine.cc.last_cmd();
    engine.load_instruction(
        Instruction::new("PUSHPOW2")
    )?;
    engine.cc.stack.push(StackItem::Integer(Arc::new(
        IntegerData::one().shl::<Signaling>(power as usize + 1)?
    )));
    Ok(())
}

// ( - int = 2^(x+1)-1)
pub(super) fn execute_pushpow2dec(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHPOW2DEC")
            .set_opts(
                InstructionOptions::LengthMinusOne(0..256)
            )
    )?;
    let power = engine.cmd.length();
    engine.cc.stack.push(StackItem::Integer(Arc::new(
        IntegerData::one()
            .shl::<Signaling>(power - 1)?
            .sub::<Signaling>(&IntegerData::one())?
            .shl::<Signaling>(1)?
            .add::<Signaling>(&IntegerData::one())?
    )));
    Ok(())
}

fn fetch_ref(engine: &mut Engine, name: &'static str, to: u16) -> Status {
    engine.load_instruction(
        Instruction::new(name)
    )?;
    fetch_reference(engine, CC)?;
    if to != CELL {
        convert(engine, var!(0), to, CELL)?;
    }
    engine.cc.stack.push(engine.cmd.vars.remove(0));
    Ok(())
}

// ( - Cell) from cc references[0]
pub(super) fn execute_pushref(engine: &mut Engine) -> Status {
    fetch_ref(engine, "PUSHREF", CELL)
}

// ( - Continuation) from cc references[0]
pub(super) fn execute_pushrefcont(engine: &mut Engine) -> Status {
    fetch_ref(engine, "PUSHREFCONT", CONTINUATION)
}

// ( - Slice) from cc references[0]
pub(super) fn execute_pushrefslice(engine: &mut Engine) -> Status {
    fetch_ref(engine, "PUSHREFSLICE", SLICE)
}

fn execute_pushslice(engine: &mut Engine, opts: InstructionOptions) -> Status {
    engine.load_instruction(
        Instruction::new("PUSHSLICE").set_opts(opts)
    )?;
    let slice = engine.cmd.slice().clone();
    engine.cc.stack.push(StackItem::Slice(slice));
    Ok(())
}

// ( - slice)
pub(super) fn execute_pushslice_short(engine: &mut Engine) -> Status {
    execute_pushslice(engine, InstructionOptions::Bitstring(8, 0, 4, 0))
}

// ( - slice)
pub(super) fn execute_pushslice_mid(engine: &mut Engine) -> Status {
    execute_pushslice(engine, InstructionOptions::Bitstring(8, 2, 5, 1))
}

// ( - slice)
pub(super) fn execute_pushslice_long(engine: &mut Engine) -> Status {
    execute_pushslice(engine, InstructionOptions::Bitstring(8, 3, 7, 0))
}

// (x ... y ... a - a ... y ... y x)
// PUXC s(i), s(j-1), equivalent to PUSH s(i); SWAP; XCHG s(j)
pub(super) fn execute_puxc(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUXC")
            .set_opts(
                InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
            )
    )?;
    let ra = engine.cmd.sregs().ra;
    let rb = engine.cmd.sregs().rb;
    if engine.cc.stack.depth() < cmp::max(ra + 1, rb) {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(ra)?;
    engine.cc.stack.swap(0, 1)?;
    engine.cc.stack.swap(0, rb)?;
    Ok(())
}

// (x ... y ... z ... a b - a ... b ... z ... z y x)
// PUXC2 s(i), s(j-1), s(k-1): equivalent to PUSH s(i); XCHG s2; XCHG2 s(j), s(k)
pub(super) fn execute_puxc2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUXC2")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusOne))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() < cmp::max(2, cmp::max(cmp::max(ra + 1, rb), rc)) {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(ra)?;
    engine.cc.stack.swap(2, 0)?;
    engine.cc.stack.swap(1, rb)?;
    engine.cc.stack.swap(0, rc)?;
    Ok(())
}

// (x ... y ... z ... a - x ... a ... z ... z y x)
// PUXCPU s(i), s(j-1), s(k-1): equivalent to PUSH s(i); SWAP; XCHG s(j); PUSH s(k)
pub(super) fn execute_puxcpu(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("PUXCPU")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOneMinusOne))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() < cmp::max(rc, cmp::max(ra + 1, rb)) {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(ra)?;
    engine.cc.stack.swap(0, 1)?;
    engine.cc.stack.swap(0, rb)?;
    engine.cc.stack.push_copy(rc)?;
    Ok(())
}

// (a(j+i-1)...a(j) ... - a(j)...a(j+i-1) ...)
pub(super) fn execute_reverse(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("REVERSE").set_opts(
            InstructionOptions::LengthMinusTwoAndIndex
        )
    )?;
    let i = engine.cmd.length_and_index().length;
    let j = engine.cmd.length_and_index().index;
    engine.cc.stack.reverse_range(j..j + i)?;
    Ok(())
}

// (a(j+i+1)...a(j+2) ... j i - a(j+2)...a(j+i+1) ...)
pub(super) fn execute_revx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("REVX")
    )?;
    fetch_stack(engine, 2)?;
    let j = engine.cmd.var(0).as_small_integer()?;
    let i = engine.cmd.var(1).as_small_integer()?;
    engine.cc.stack.reverse_range(j..j + i)?;
    Ok(())
}

// (x a(i)...a(1) i - a(i)...a(1) x)
pub(super) fn execute_roll(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ROLLX")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    let x = engine.cc.stack.drop(i)?;
    engine.cc.stack.push(x);
    Ok(())
}

// (a(i+1)...a(2) x i - x a(i+1)...a(2))
pub(super) fn execute_rollrev(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ROLLREVX")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    if engine.cc.stack.depth() <= i {
        err!(ExceptionCode::StackUnderflow)
    } else {
        let x = engine.cc.stack.drop(0)?;
        engine.cc.stack.insert(i, x);
        Ok(())
    }
}

// (a b c - b c a)
pub(super) fn execute_rot(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ROT")
    )?;
    let top = engine.cc.stack.drop(2)?;
    engine.cc.stack.push(top);
    Ok(())
}

// (a b c - c a b)
pub(super) fn execute_rotrev(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("ROTREV")
    )?;
    if engine.cc.stack.depth() < 3 {
        err!(ExceptionCode::StackUnderflow)
    } else {
        let top = engine.cc.stack.drop(0)?;
        engine.cc.stack.insert(2, top);
        Ok(())
    }
}

// (a b c d - c d a b)
pub(super) fn execute_swap2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("SWAP2")
    )?;
    if engine.cc.stack.depth() < 4 {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.block_swap(2, 2)
    }
}

// (x y - y x y)
pub(super) fn execute_tuck(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("TUCK")
    )?;
    if engine.cc.stack.depth() < 2 {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.push_copy(0)?;
    engine.cc.stack.swap(1, 2)?;
    Ok(())
}

// (x ... y ... z ... a b - x ... a ... b ... z y x)
// XC2PU s(i), s(j), s(k): equivalent to XCHG2 s(i), s(j); PUSH s(k)
pub(super) fn execute_xc2pu(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XC2PU")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() <= cmp::max(1, cmp::max(ra, cmp::max(rb, rc))) {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.swap(1, ra)?;
        engine.cc.stack.swap(0, rb)?;
        engine.cc.stack.push_copy(rc)?;
        Ok(())
    }
}

// (x ... y ... - y ... x ...)
pub(super) fn execute_xchg(engine: &mut Engine, name: &'static str, opts: InstructionOptions) -> Status {
    engine.load_instruction(
        Instruction::new(name).set_opts(opts)
    )?;
    let ra = engine.cmd.sregs().ra;
    let rb = engine.cmd.sregs().rb;
    engine.cc.stack.swap(ra, rb)?;
    Ok(())
}

// SWAP
pub(super) fn execute_swap(engine: &mut Engine) -> Status {
    execute_xchg(
        engine,
        "SWAP",
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromLastByte)
    )
}

// XCHG addressing via the same instruction byte
pub(super) fn execute_xchg_simple(engine: &mut Engine) -> Status {
    execute_xchg(
        engine,
        "XCHG",
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromLastByte)
    )
}

// XCHG addressing via the next instruction byte
pub(super) fn execute_xchg_std(engine: &mut Engine) -> Status {
    execute_xchg(
        engine,
        "XCHG",
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
    )
}

// XCHG addressing via the next instruction byte, long index
pub(super) fn execute_xchg_long(engine: &mut Engine) -> Status {
    execute_xchg(
        engine,
        "XCHG",
        InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByteLong)
    )
}

// (x ... y ... a b - a ... b ... x y)
// XCHG s(1),s(i); XCHG s(0),s(j).
pub(super) fn execute_xchg2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCHG2").set_opts(
            InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
        )
    )?;
    let ra = engine.cmd.sregs().ra;
    let rb = engine.cmd.sregs().rb;
    if engine.cc.stack.depth() <= cmp::max(1, cmp::max(ra, rb)) {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.swap(1, ra)?;
        engine.cc.stack.swap(0, rb)?;
        Ok(())
    }
}

// (x ... y ... z ... a b c - c ... b ... a ... z y x)
// XCHG s(2), s(i); XCHG s(1) s(j); XCHG s(0), s(k)
pub(super) fn execute_xchg3(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCHG3")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() <= cmp::max(2, cmp::max(rc, cmp::max(ra, rb))) {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.swap(2, ra)?;
        engine.cc.stack.swap(1, rb)?;
        engine.cc.stack.swap(0, rc)?;
        Ok(())
    }
}

// (a(i+1)...a(1) i - a(1)...a(i+1))
pub(super) fn execute_xchgx(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCHGX")
    )?;
    fetch_stack(engine, 1)?;
    let i = engine.cmd.var(0).as_small_integer()?;
    engine.cc.stack.swap(0, i)?;
    Ok(())
}

// (x ... y ... a - x ... a ... y x)
// XCHG s(i), PUSH s(j)
pub(super) fn execute_xcpu(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCPU").set_opts(
            InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromNextByte)
        )
    )?;
    let ra = engine.cmd.sregs().ra;
    let rb = engine.cmd.sregs().rb;
    if engine.cc.stack.depth() <= cmp::max(ra, rb) {
        err!(ExceptionCode::StackUnderflow)
    } else {
        engine.cc.stack.swap(0, ra)?;
        engine.cc.stack.push_copy(rb)?;
        Ok(())
    }
}

// (x ... y ... z ... a - x ... y ... a ... z y x)
// XCHG s(i), PUSH s(j), PUSH s(k+1)
pub(super) fn execute_xcpu2(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCPU2")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByte))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() <= cmp::max(1, cmp::max(ra, cmp::max(rb, rc))) {
        return err!(ExceptionCode::StackUnderflow);
    }
    engine.cc.stack.swap(0, ra)?;
    engine.cc.stack.push_copy(rb)?;
    engine.cc.stack.push_copy(rc + 1)?;
    Ok(())
}

// (x ... y ... z ... a b - b ... y ... a ... z y x)
// XCPUXC s(i), s(j), s(k-1): equavalent to XCHG s(1), s(i); PUSH s(j); SWAP; XCHG s(k)
pub(super) fn execute_xcpuxc(engine: &mut Engine) -> Status {
    engine.load_instruction(
        Instruction::new("XCPUXC")
            .set_opts(InstructionOptions::StackRegisterTrio(WhereToGetParams::GetFromNextByteMinusOne))
    )?;
    let ra = engine.cmd.sregs3().ra;
    let rb = engine.cmd.sregs3().rb;
    let rc = engine.cmd.sregs3().rc;
    if engine.cc.stack.depth() < cmp::max(2, cmp::max(rc, cmp::max(ra, rb) + 1)) {
        return err!(ExceptionCode::StackUnderflow)
    }
    engine.cc.stack.swap(1, ra)?;
    engine.cc.stack.push_copy(rb)?;
    engine.cc.stack.swap(0, 1)?;
    engine.cc.stack.swap(0, rc)?;
    Ok(())
}
