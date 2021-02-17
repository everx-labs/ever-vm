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
    error::{tvm_exception_full, TvmError},
    executor::{
        continuation::switch, engine::{handlers::Handlers, storage::{swap, copy_to_var}}, 
        gas::gas_state::Gas, math::DivMode, microcode::{VAR, SAVELIST, CC, CTRL},
        types::{
            Ctx, Instruction, InstructionOptions, InstructionParameter, RegisterPair,
            RegisterTrio, LengthAndIndex, Undo, WhereToGetParams,
        }
    },
    stack::{
        Stack, StackItem, continuation::{ContinuationData, ContinuationType}, 
        integer::IntegerData, savelist::SaveList
    },
    smart_contract_info::SmartContractInfo, 
    types::{Exception, Failure, ResultMut, ResultRef, Status}
};
use std::{collections::HashSet, sync::Arc};
use ton_types::{
    BuilderData, Cell, CellType, error, GasConsumer, Result, SliceData, HashmapE,
    types::{ExceptionCode, UInt256}
};

pub(super) type ExecuteHandler = fn(&mut Engine) -> Failure;

pub struct Engine {
    pub(in crate::executor) cc: ContinuationData,
    pub(in crate::executor) cmd: Instruction,
    pub(in crate::executor) ctrls: SaveList,
    pub(in crate::executor) libraries: Vec<HashmapE>, // 256 bit dictionaries
    visited_cells: HashSet<UInt256>,
    cstate: CommittedState,
    handlers: Handlers,
    time: u64,
    gas: Gas,
    code_page: isize,
    debug_on: isize, // status of debug can be recursively incremented
    step: u32, // number of executable command
    debug_buffer: String,
    cmd_code: SliceData, // start of current cmd
    trace: u8,
    trace_callback: Option<Box<dyn Fn(&Engine, &EngineTraceInfo)>>,
    log_string: Option<&'static str>,
}

#[derive(Eq, Debug, PartialEq)]
pub enum EngineTraceInfoType {
    Start,
    Normal,
    Finish,
    Implicit,
    Exception,
    Dump,
}

pub struct EngineTraceInfo<'a> {
    pub info_type: EngineTraceInfoType,
    pub step: u32, // number of executable command
    pub cmd_str: String,
    pub cmd_code: SliceData, // start of current cmd
    pub stack: &'a Stack,
    pub gas_used: i64,
    pub gas_cmd: i64,
}

impl<'a> EngineTraceInfo<'a> {
    pub fn has_cmd(&self) -> bool {
        match self.info_type {
            EngineTraceInfoType::Normal | EngineTraceInfoType::Implicit => true,
            _ => false
        }
    }
}

#[derive(Debug)]
pub struct CommittedState {
    c4: StackItem,
    c5: StackItem,
    committed: bool
}

impl CommittedState {
    pub fn new_empty() -> CommittedState {
        CommittedState {
            c4: StackItem::None,
            c5: StackItem::None,
            committed: false
        }
    }
    pub fn with_params(c4: StackItem, c5: StackItem) -> CommittedState {
        if SaveList::can_put(4, &c4) && SaveList::can_put(5, &c5) {
            CommittedState {
                c4,
                c5,
                committed: true
            }
        } else {
            debug_assert!(false);
            CommittedState::new_empty()
        }
    }
    pub fn get_actions(&self) -> StackItem {
        self.c5.clone()
    }
    pub fn get_root(&self) -> StackItem {
        self.c4.clone()
    }
    pub fn is_committed(&self) -> bool {
        self.committed
    }
}

impl GasConsumer for Engine {
    fn finalize_cell(&mut self, builder: BuilderData) -> Result<Cell> {
        self.use_gas(Gas::finalize_price());
        builder.into_cell()
    }
    fn load_cell(&mut self, cell: Cell) -> Result<SliceData> {
        self.load_hashed_cell(cell, true)
    }
    fn finalize_cell_and_load(&mut self, builder: BuilderData) -> Result<SliceData> {
        let cell = self.finalize_cell(builder)?;
        self.load_hashed_cell(cell, true)
    }
}

impl Engine {
    pub const TRACE_CODE:  u8 = 0x01;
    pub const TRACE_GAS:   u8 = 0x02;
    pub const TRACE_STACK: u8 = 0x04;
    pub const TRACE_CTRLS: u8 = 0x08;
    pub const TRACE_ALL:   u8 = 0xFF;
    pub const TRACE_ALL_BUT_CTRLS:   u8 = 0x07;

    // External API ***********************************************************

    pub fn new() -> Engine {
        Engine {
            cc: ContinuationData::new_empty(),
            cmd: Instruction::new("NOP"),
            ctrls: SaveList::new(),
            libraries: Vec::new(),
            visited_cells: HashSet::new(),
            cstate: CommittedState::new_empty(),
            handlers: Handlers::new_code_page_0(),
            time: 0,
            gas: Gas::empty(),
            code_page: 0,
            debug_on: 1,
            step: 0,
            debug_buffer: String::new(),
            cmd_code: SliceData::default(),
            #[cfg(not(all(feature="verbose", feature="fift_check")))]
            trace: Engine::TRACE_ALL,
            #[cfg(all(feature="verbose", feature="fift_check"))]
            trace: Engine::TRACE_ALL_BUT_CTRLS,
            #[cfg(not(feature="verbose"))]
            trace_callback: None,
            #[cfg(all(feature="verbose", not(feature="fift_check")))]
            trace_callback: Some(Box::new(Self::defaul_trace_callback)),
            #[cfg(all(feature="verbose", feature="fift_check"))]
            trace_callback: Some(Box::new(Self::fift_trace_callback)),
            log_string: None,
        }
    }

    pub fn assert_ctrl(&self, ctrl: usize, item: &StackItem) -> &Engine {
        match self.ctrls.get(ctrl) {
            Some(x) => assert!(Stack::eq_item(x, item)),
            None => assert!(false),
        }
        self
    }

    pub fn assert_stack(&self, stack: &Stack) -> &Engine {
        assert!(self.cc.stack.eq(stack));
        self
    }

    pub fn eq_stack(&self, stack: &Stack) -> bool {
        self.cc.stack.eq(stack)
    }

    pub fn stack(&self) -> &Stack {
        &self.cc.stack
    }

    pub fn try_use_gas(&mut self, gas: i64) -> Result<()> {
        self.gas.try_use_gas(gas)?;
        Ok(())
    }

    pub fn use_gas(&mut self, gas: i64) -> i64 {
        self.gas.use_gas(gas)
    }

    pub fn gas_used(&self) -> i64 {
        self.gas.get_gas_used_full()
    }

    pub fn gas_remaining(&self) -> i64 {
        self.gas.get_gas_remaining()
    }

    pub fn withdraw_stack(&mut self) -> Stack {
        std::mem::replace(&mut self.cc.stack, Stack::new())
    }

    pub fn get_stack_result_fift(&self) -> String {
        self.cc.stack.iter().map(|item| item.dump_as_fift()).collect::<Vec<_>>().join(" ")
    }

    pub fn get_committed_state_fift(&self) -> String {
        format!(" {} {}", self.cstate.c4.dump_as_fift(), self.cstate.c5.dump_as_fift())
    }

    pub fn commit(&mut self) {
        self.cstate = CommittedState::with_params(self.get_root(), self.get_actions());
    }

    pub fn steps(&self) -> u32 {
        self.step
    }

    fn trace_info(&self, info_type: EngineTraceInfoType, gas: i64, log_string: Option<String>) {
        if let Some(callback) = &self.trace_callback {
            let cmd_str = log_string.or_else(|| self.cmd.dump_with_params()).unwrap_or_else(|| String::new());
            let info = EngineTraceInfo {
                info_type,
                step: self.step,
                cmd_str,
                cmd_code: self.cmd_code.clone(),
                stack: &self.cc.stack,
                gas_used: self.gas_used(),
                gas_cmd: self.gas_used() - gas,
            };
            callback(&self, &info);
        }
    }

    #[allow(dead_code)]
    fn defaul_trace_callback(&self, info: &EngineTraceInfo) {
        if self.trace_bit(Engine::TRACE_CODE) && info.has_cmd() {
            log::trace!(
                target: "tvm", 
                "{}: {}\n{}\n", 
                info.step,
                info.cmd_str,
                info.cmd_code.to_hex_string(),
            );
        }
        if self.trace_bit(Engine::TRACE_GAS) {
            log::trace!(
                target: "tvm", 
                "Gas: {} ({})\n", 
                info.gas_used, 
                info.gas_cmd
            );
        }
        if self.trace_bit(Engine::TRACE_STACK) {
            log::trace!(target: "tvm", "{}", self.dump_stack("Stack trace", false));
        }
        if self.trace_bit(Engine::TRACE_CTRLS) {
            log::trace!(target: "tvm", "{}", self.dump_ctrls(true));
        }
    }

    #[allow(dead_code)]
    fn fift_trace_callback(&self, info: &EngineTraceInfo) {
        if info.info_type == EngineTraceInfoType::Dump {
            log::info!(target: "tvm", "{}", info.cmd_str);
        } else if info.info_type == EngineTraceInfoType::Start {
            if self.trace_bit(Engine::TRACE_CTRLS) {
                log::trace!(target: "tvm", "{}", self.dump_ctrls(true));
            }
            if self.trace_bit(Engine::TRACE_STACK) {
                log::info!(target: "tvm", " [ {} ] \n", self.get_stack_result_fift());
            }
            if self.trace_bit(Engine::TRACE_GAS) {
                log::info!(target: "tvm", "gas - {}\n", info.gas_used);
            }
        } else if info.info_type == EngineTraceInfoType::Exception {
            if self.trace_bit(Engine::TRACE_CODE) {
                log::info!(target: "tvm", "{}\n", info.cmd_str);
            }
            if self.trace_bit(Engine::TRACE_STACK) {
                log::info!(target: "tvm", " [ {} ] \n", self.get_stack_result_fift());
            }
            if self.trace_bit(Engine::TRACE_CTRLS) {
                log::trace!(target: "tvm", "{}", self.dump_ctrls(true));
            }
            if self.trace_bit(Engine::TRACE_GAS) {
                log::info!(target: "tvm", "gas - {}\n", info.gas_used);
            }
        } else if info.has_cmd() {
            if self.trace_bit(Engine::TRACE_CODE) {
                log::info!(target: "tvm", "execute {}\n", info.cmd_str);
            }
            if self.trace_bit(Engine::TRACE_STACK) {
                log::info!(target: "tvm", " [ {} ] \n", self.get_stack_result_fift());
            }
            if self.trace_bit(Engine::TRACE_CTRLS) {
                log::trace!(target: "tvm", "{}", self.dump_ctrls(true));
            }
            if self.trace_bit(Engine::TRACE_GAS) {
                log::info!(target: "tvm", "gas - {}\n", info.gas_used);
            }
        }
    }

    pub fn execute(&mut self) -> Result<i32> {
        self.trace_info(EngineTraceInfoType::Start, 0, None);
        let result = loop {
            if let Some(result) = self.seek_next_cmd()? {
                if self.gas.get_gas_credit() != 0 &&
                    self.gas.get_gas_remaining() < self.gas.get_gas_credit() {
                    break result//return err!(ExceptionCode::OutOfGas)
                } else {
                    break result
                }
            }
            let gas = self.gas_used();
            self.cmd_code = self.cc.code().clone();
            self.cmd = Instruction::new("");
            let execution_result = match self.handlers.get_handler(&mut self.cc) {
                Err(err) => {
                    // it is simple value and doesn't correspond to Durov's code
                    // but it is difficult in our engine to calc sharp
                    // hope it will not be a problem
                    self.basic_use_gas(8);
                    Some(err)
                }
                Ok(handler) => handler(self).or_else(|| self.gas.check_gas_remaining().err()),
            };
            self.trace_info(EngineTraceInfoType::Normal, gas, None);
            self.cmd.ictx.clear();
            self.raise_exception(execution_result, true)?;
        };
        self.trace_info(EngineTraceInfoType::Finish, self.gas_used(), Some(format!("NORMAL TERMINATION")));
        self.commit();
        Ok(result)
    }

    fn step_next_ref(&mut self, reference: Cell) -> Result<Option<i32>> {
        self.step += 1;
        self.log_string = Some("IMPLICIT JMPREF");
        self.try_use_gas(Gas::implicit_jmp_price())?;
        let code = self.load_hashed_cell(reference, true)?;
        *self.cc.code_mut() = code;
        Ok(None)
    }
    fn step_ordinary(&mut self) -> Result<Option<i32>> {
        self.try_use_gas(Gas::implicit_ret_price())?;
        if self.ctrls.get(0).is_none() {
            return Ok(Some(0))
        }
        self.step += 1;
        self.log_string = Some("implicit RET");
        switch(Ctx::new(self), ctrl!(0))?;
        Ok(None)
    }
    fn step_pushint(&mut self, code: i32) -> Result<Option<i32>> {
        self.step += 1;
        self.log_string = Some("implicit PUSHINT");
        self.cc.stack.push(int!(code));
        switch(Ctx::new(self), ctrl!(0))?;
        Ok(None)
    }
    fn step_try_catch(&mut self) -> Result<Option<i32>> {
        self.step += 1;
        self.log_string = Some("IMPLICIT RET FROM TRY-CATCH");
        self.try_use_gas(Gas::implicit_ret_price())?;
        self.ctrls.remove(2).unwrap();
        switch(Ctx::new(self), ctrl!(0))?;
        Ok(None)
    }
    fn step_while_loop(&mut self, body: SliceData, cond: SliceData) -> Result<Option<i32>> {
        match self.check_while_loop_condition() {
            Ok(true) => {
                self.log_string = Some("NEXT WHILE ITERATION");
                let n = self.cmd.var_count();
                let body = ContinuationData::with_code(body);
                let cond = ContinuationData::with_code(cond);
                self.cmd.push_var(StackItem::Continuation(Arc::new(body)));
                self.cmd.push_var(StackItem::Continuation(Arc::new(cond)));
                copy_to_var(Ctx::new(self), CC)
                .and_then(|ctx| swap(ctx, savelist!(var!(n + 2), 0), ctrl!(0)))     // ec_while.savelist[0] = c[0]
                .and_then(|ctx| swap(ctx, savelist!(var!(n + 1), 0), var!(n + 2)))  // cond.savelist[0] = ec_while
                .and_then(|ctx| swap(ctx, savelist!(var!(n), 0), var!(n + 1)))      // body.savelist[0] = cond
                .and_then(|ctx| switch(ctx, var!(n)))?;
            }
            Ok(false) => {
                self.log_string = Some("RET FROM WHILE");
                switch(Ctx::new(self), ctrl!(0))?;
            }
            Err(err) => return Err(err)
        }
        Ok(None)
    }
    fn step_repeat_loop(&mut self, body: SliceData) -> Result<Option<i32>> {
        if let ContinuationType::RepeatLoopBody(_, ref mut counter) = self.cc.type_of {
            if *counter > 1 {
                *counter -= 1;
                self.log_string = Some("NEXT REPEAT ITERATION");
                let n = self.cmd.var_count();
                let body = ContinuationData::with_code(body);
                self.cmd.push_var(StackItem::Continuation(Arc::new(body)));
                copy_to_var(Ctx::new(self), CC)
                .and_then(|ctx| swap(ctx, savelist!(var!(n + 1), 0), ctrl!(0))) // ec_repeat.savelist[0] = cc
                .and_then(|ctx| swap(ctx, savelist!(var!(n), 0), var!(n + 1))) // body.savelist[0] = ec_repeat
                .and_then(|ctx| switch(ctx, var!(n)))?;
            } else {
                self.log_string = Some("RET FROM REPEAT");
                switch(Ctx::new(self), ctrl!(0))?;
            }
        }
        Ok(None)
    }
    fn step_unil_loop(&mut self, body: SliceData) -> Result<Option<i32>> {
        match self.check_until_loop_condition() {
            Ok(true) => {
                self.log_string = Some("NEXT UNTIL ITERATION");
                let n = self.cmd.var_count();
                let body = ContinuationData::with_code(body);
                self.cmd.push_var(StackItem::Continuation(Arc::new(body)));
                copy_to_var(Ctx::new(self), CC)
                .and_then(|ctx| swap(ctx, savelist!(var!(n + 1), 0), ctrl!(0)) )    // until.savelist[0] = c[0]
                .and_then(|ctx| swap(ctx, savelist!(var!(n), 0), var!(n + 1)))      // body.savelist[0] = until
                .and_then(|ctx| switch(ctx, var!(n)))?;
            }
            Ok(false) => {
                self.log_string = Some("RET FROM UNTIL");
                switch(Ctx::new(self), ctrl!(0))?;
            }
            Err(err) => return Err(err)
        }
        Ok(None)
    }
    fn step_again_loop(&mut self, body: SliceData) -> Result<Option<i32>> {
        self.log_string = Some("NEXT AGAIN ITERATION");
        let n = self.cmd.var_count();
        let body = ContinuationData::with_code(body);
        self.cmd.push_var(StackItem::Continuation(Arc::new(body)));
        copy_to_var(Ctx::new(self), CC)
        .and_then(|ctx| swap(ctx, savelist!(var!(n), 0), var!(n + 1)) ) // body.savelist[0] = ec_again
        .and_then(|ctx| switch(ctx, var!(n)))?;
        Ok(None)
    }

    // return Ok(Some(exit_code)) - if you want to stop execution
    pub(in crate::executor) fn seek_next_cmd(&mut self) -> Result<Option<i32>> {
        while self.cc.code().remaining_bits() == 0 {
            let gas = self.gas_used();
            self.log_string = None;
            let result = if let Some(reference) = self.cc.code().reference_opt(0) {
                self.step_next_ref(reference)
            } else {
                match self.cc.type_of.clone() {
                    ContinuationType::Ordinary => self.step_ordinary(),
                    ContinuationType::PushInt(code) => self.step_pushint(code),
                    ContinuationType::Quit(exit_code) => Ok(Some(exit_code)),
                    ContinuationType::TryCatch => self.step_try_catch(),
                    ContinuationType::WhileLoopCondition(body, cond) => self.step_while_loop(body, cond),
                    ContinuationType::RepeatLoopBody(code, _counter) => self.step_repeat_loop(code),
                    ContinuationType::UntilLoopCondition(body) => self.step_unil_loop(body),
                    ContinuationType::AgainLoopBody(slice) => self.step_again_loop(slice),
                }
            };
            if let Some(log_string) = self.log_string {
                self.trace_info(EngineTraceInfoType::Implicit, gas, Some(log_string.to_string()));
            }
            match self.gas.check_gas_remaining().and(result) {
                Ok(None) => (),
                Ok(Some(exit_code)) => return Ok(Some(exit_code)),
                Err(err) => self.raise_exception(Some(err), false)?
            }
        }
        Ok(None)
    }


    pub fn load_library_cell(&mut self, cell: Cell) -> Result<Cell> {
        let mut hash = SliceData::from(cell);
        hash.move_by(8)?;
        for library in self.libraries.clone() {
            if let Some(lib) = library.get_with_gas(hash.clone(), self)? {
                return lib.reference(0)
            }
        }
        err!(ExceptionCode::CellUnderflow, "Libraries do not contain code with hash {}", hash.to_hex_string())
    }

    /// Loads cell to slice cheking in precashed map
    pub fn load_hashed_cell(&mut self, cell: Cell, check_special: bool) -> Result<SliceData> {
        let first = self.visited_cells.insert(cell.repr_hash());
        self.use_gas(Gas::load_cell_price(first));
        if check_special {
            match cell.cell_type() {
                CellType::Ordinary => {
                    Ok(cell.into())
                }
                CellType::LibraryReference => {
                    let cell = self.load_library_cell(cell)?;
                    self.load_hashed_cell(cell, true)
                }
                cell_type => err!(ExceptionCode::CellUnderflow, "Wrong cell type {}", cell_type)
            }
        } else {
            Ok(cell.into())
        }
    }

    pub fn get_committed_state(&self) -> &CommittedState {
        &self.cstate
    }

    pub fn get_actions(&self) -> StackItem {
        match self.ctrls.get(5) {
            Some(x) => x.clone(),
            None => StackItem::None,
        }
    }

    fn get_root(&self) -> StackItem {
        match self.ctrls.get(4) {
            Some(x) => x.clone(),
            None => StackItem::None,
        }
    }

    pub fn ctrl(&self, index: usize) -> ResultRef<StackItem> {
        Ok(self.ctrls.get(index).ok_or(ExceptionCode::RangeCheckError)?)
    }

    pub fn ctrl_mut(&mut self, index: usize) -> ResultMut<StackItem> {
        Ok(self.ctrls.get_mut(index).ok_or(ExceptionCode::RangeCheckError)?)
    }

    fn dump_msg(message: &'static str, data: String) -> String {
        format!("--- {} {:-<4$}\n{}\n{:-<40}\n", message, "", data, "", 35-message.len())
    }

    pub fn dump_ctrls(&self, short: bool) -> String {
        Self::dump_msg("Control registers", (0..16)
            .filter_map(|i| self.ctrls.get(i).map(|item| if !short {
                format!("{}: {}", i, item)
            } else if i == 3 {
                "3: copy of CC".to_string()
            } else if i == 7 {
                "7: SmartContractInfo".to_string()
            } else if let StackItem::Continuation(x) = item {
                format!("{}: {:?}", i, x.type_of)
            } else {
                format!("{}: {}", i, item.dump_as_fift())
            })).collect::<Vec<_>>().join("\n")
        )
    }

    pub fn dump_stack(&self, message: &'static str, short: bool) -> String {
        Self::dump_msg(message, self.cc.stack.iter()
            .map(|item| if !short {
                format!("{}", item)
            } else {
                item.dump_as_fift()
            })
            .collect::<Vec<_>>().join("\n")
        )
    }

    // TODO: check if it should be in SmartContractInfo
    pub fn set_local_time(&mut self, time: u64) {
        self.time = time
    }

    pub fn set_trace(&mut self, trace_mask: u8) {
        self.trace = trace_mask
    }

    pub fn set_trace_callback(&mut self, callback: impl Fn(&Engine, &EngineTraceInfo) + 'static) {
        self.trace_callback = Some(Box::new(callback));
    }

    fn trace_bit(&self, trace_mask: u8) -> bool {
        (self.trace & trace_mask) == trace_mask
    }

    pub fn setup(self, code: SliceData, ctrls: Option<SaveList>, stack: Option<Stack>, gas: Option<Gas>) -> Self {
        self.setup_with_libraries(code, ctrls, stack, gas, vec![])
    }

    pub fn setup_with_libraries(
        mut self,
        code: SliceData,
        mut ctrls: Option<SaveList>,
        stack: Option<Stack>,
        gas: Option<Gas>,
        libraries: Vec<HashmapE>
    ) -> Self {
        *self.cc.code_mut() = code.clone();
        self.cmd_code = code.clone();
        if let Some(stack) = stack {
            self.cc.stack = stack;
        }
        self.gas = gas.unwrap_or(Gas::test());
        self.ctrls.put(0, &mut StackItem::Continuation(Arc::new(ContinuationData::with_type(
            ContinuationType::Quit(ExceptionCode::NormalTermination as i32)
        )))).unwrap();
        self.ctrls.put(1, &mut StackItem::Continuation(Arc::new(ContinuationData::with_type(
            ContinuationType::Quit(ExceptionCode::AlternativeTermination as i32)
        )))).unwrap();
        self.ctrls.put(3, &mut StackItem::Continuation(Arc::new(
            ContinuationData::with_code(code)
        ))).unwrap();
        self.ctrls.put(4, &mut StackItem::Cell(BuilderData::default().into())).unwrap();
        self.ctrls.put(5, &mut StackItem::Cell(BuilderData::default().into())).unwrap();
        self.ctrls.put(7, &mut SmartContractInfo::default().into_temp_data()).unwrap();
        if let Some(ref mut ctrls) = ctrls {
            self.apply_savelist(ctrls).unwrap();
        }
        self.libraries = libraries;
        self
    }

    // Internal API ***********************************************************

    pub(in crate::executor) fn apply_savelist(&mut self, savelist: &mut SaveList) -> Status {
        for (k, v) in savelist.iter_mut() {
            self.ctrls.put(*k, v)?;
        }
        savelist.clear();
        Ok(())
    }

    #[allow(dead_code)]
    pub(in crate::executor) fn local_time(&mut self) -> u64 {
        self.time += 1;
        self.time
    }

    // Implementation *********************************************************

    pub(in crate::executor) fn load_instruction(&mut self, cmd: Instruction) -> Result<Ctx> {
        self.cmd = cmd;
        self.step += 1;
        self.extract_instruction().map(move |_| Ctx::new(self))
        // let result = self.extract_instruction();
        // old formula for command with refs and data bits
        // let refs = self.cmd_code.remaining_references() - self.cc.code().remaining_references();
        // let bits = self.cmd_code.remaining_bits() - self.cc.code().remaining_bits();
        // self.use_gas(Gas::basic_gas_price(bits, refs));
        // result.map(move |_| Ctx::new(self))
    }

    pub(in crate::executor) fn switch_debug(&mut self, on_off: bool) {
        self.debug_on += if on_off {1} else {-1}
    }

    pub(in crate::executor) fn debug(&self) -> bool {
        self.debug_on > 0
    }

    pub(in crate::executor) fn dump(&mut self, dump: String) {
        self.debug_buffer += &dump;
    }

    pub(in crate::executor) fn flush(&mut self) {
        if self.debug_on > 0 {
            let buffer = std::mem::replace(&mut self.debug_buffer, String::new());
            self.trace_info(EngineTraceInfoType::Dump, 0, Some(buffer));
        }
        self.debug_buffer = String::new()
    }

    ///Get gas state
    pub fn get_gas(&self) -> &Gas {
        &self.gas
    }
    ///Set gas state
    pub fn set_gas(&mut self, gas: Gas) {
        self.gas = gas
    }   
    ///Interface to gas state set_gas_limit method
    pub fn new_gas_limit(&mut self, gas: i64) {
        self.gas.new_gas_limit(gas)
    }    
    
    fn check_while_loop_condition(&mut self) -> Result<bool> {
        let x = self.cc.stack.drop(0)?;
        let y = x.as_integer()?;
        Ok(!y.is_zero())
    }

    fn check_until_loop_condition(&mut self) -> Result<bool> {
        self.check_while_loop_condition().map(|r| !r)
    }

    fn extract_slice(&mut self, offset: usize, r: usize, x: usize, mut refs: usize, mut bytes: usize) -> Result<SliceData> {
        let mut code = self.cmd_code.clone();
        let mut slice = code.clone();
        if offset >= slice.remaining_bits() {
            return err!(ExceptionCode::InvalidOpcode)
        }
        slice.shrink_data(offset..);
        if r != 0 {
            refs += slice.get_next_int(r)? as usize;
        }
        if x != 0 {
            bytes += slice.get_next_int(x)? as usize;
        }
        let mut shift = 8 * bytes + offset + r + x + 7;
        let remainder = shift % 8;
        shift -= remainder;
        if (slice.remaining_bits() < shift - r - x - offset) || (slice.remaining_references() < refs) {
            return err!(ExceptionCode::InvalidOpcode)
        }
        code.shrink_data(shift..);
        code.shrink_references(refs..);
        *self.cc.code_mut() = code;

        slice.shrink_data(..shift - r - x - offset);
        slice.shrink_references(..refs);

        Ok(slice)
    }

    fn basic_use_gas(&mut self, mut bits: usize) -> i64 {
        debug_assert_eq!(self.cmd_code.cell().repr_hash(), self.cmd_code.cell().repr_hash());
        bits += self.cc.code().pos().checked_sub(self.cmd_code.pos()).unwrap_or(0);
        self.use_gas(Gas::basic_gas_price(bits, 0))
    }

    fn extract_instruction(&mut self) -> Status {
        match self.cmd.opts {
            Some(InstructionOptions::ArgumentConstraints) => {
                let param = self.cc.next_cmd()?;
                self.basic_use_gas(0);
                self.cmd.ictx.params.push(
                    InstructionParameter::Pargs(((param >> 4) & 0x0F) as usize)
                );
                self.cmd.ictx.params.push(
                    InstructionParameter::Nargs(
                        if (param & 0x0F) == 15 {
                            -1
                        } else {
                            (param & 0x0F) as isize
                        }
                    )
                )
            },
            Some(InstructionOptions::ArgumentAndReturnConstraints) => {
                let param = self.cc.next_cmd()?;
                self.basic_use_gas(0);
                self.cmd.ictx.params.push(
                    InstructionParameter::Pargs(((param >> 4) & 0x0F) as usize)
                );
                self.cmd.ictx.params.push(
                    InstructionParameter::Rargs((param & 0x0F) as usize)
                )
            },
            Some(InstructionOptions::BigInteger) => {
                self.basic_use_gas(5);

                let bigint = IntegerData::from_big_endian_octet_stream(|| self.cc.next_cmd())?;
                self.cmd.ictx.params.push(InstructionParameter::BigInteger(bigint))
            }
            Some(InstructionOptions::ControlRegister) => {
                self.basic_use_gas(0);
                self.cmd.ictx.params.push(
                    InstructionParameter::ControlRegister((self.cc.last_cmd() & 0x0F) as usize)
                )
            },
            Some(InstructionOptions::DivisionMode) => {
                let mode = DivMode::with_flags(self.cc.next_cmd()?);
                if mode.shift_parameter() {
                    self.cmd.ictx.params.push(
                        InstructionParameter::Length(self.cc.next_cmd()? as usize + 1)
                    )
                }
                self.basic_use_gas(0);
                self.cmd.name = mode.command_name()?;
                self.cmd.ictx.params.push(InstructionParameter::DivisionMode(mode));
            },
            Some(InstructionOptions::Integer(ref range)) => {
                let number = if *range == (-32768..32768) {
                    self.basic_use_gas(16);
                    (((self.cc.next_cmd()? as i16) << 8) | (self.cc.next_cmd()? as i16)) as isize
                } else if *range == (-128..128) {
                    self.basic_use_gas(8);
                    (self.cc.next_cmd()? as i8) as isize
                } else if *range == (-5..11) {
                    self.basic_use_gas(0);
                    match self.cc.last_cmd() & 0x0F {
                        value @ 0..=10 => value as isize,
                        value @ _ => value as isize - 16
                    }
                } else if *range == (0..32) {
                    self.basic_use_gas(0);
                    (self.cc.last_cmd() & 0x1F) as isize
                } else if *range == (0..64) {
                    self.basic_use_gas(0);
                    (self.cc.last_cmd() % 64) as isize
                } else if *range == (0..2048) {
                    self.basic_use_gas(8);
                    let hi = (self.cc.last_cmd() as i16) & 0x07;
                    let lo = self.cc.next_cmd()? as i16;
                    (hi * 256 + lo) as isize
                } else if *range == (0..16384) {
                    self.basic_use_gas(8);
                    let hi = (self.cc.last_cmd() as i16) & 0x3F;
                    let lo = self.cc.next_cmd()? as i16;
                    (hi * 256 + lo) as isize
                } else if *range == (0..256) {
                    self.basic_use_gas(8);
                    self.cc.next_cmd()? as isize
                } else if *range == (0..15) {
                    self.basic_use_gas(0);
                    match self.cc.last_cmd() & 0x0F {
                        15 => return err!(ExceptionCode::RangeCheckError),
                        value @ _ => value as isize
                    }
                } else if *range == (1..15) {
                    self.basic_use_gas(0);
                    match self.cc.last_cmd() & 0x0F {
                        0 | 15 => return err!(ExceptionCode::RangeCheckError),
                        value @ _ => value as isize
                    }
                } else if *range == (-15..240) {
                    self.basic_use_gas(0);
                    match self.cc.last_cmd() {
                        value @ 0..=240 => value as isize,
                        value @ 0xF1..=0xFF => value as isize - 256,
                    }
                } else {
                    return err!(ExceptionCode::RangeCheckError)
                };
                self.cmd.ictx.params.push(InstructionParameter::Integer(number))
            },
            Some(InstructionOptions::Length(ref range)) => {
                if *range == (0..16) {
                    self.cmd.ictx.params.push(
                        InstructionParameter::Length((self.cc.last_cmd() & 0x0F) as usize)
                    )
                } else if *range == (0..4) {
                    let length = self.cc.last_cmd() & 3;
                    self.cmd.ictx.params.push(InstructionParameter::Length(length as usize))
                } else if *range == (1..32) {
                    let length = self.cc.last_cmd() & 0x1F;
                    self.cmd.ictx.params.push(InstructionParameter::Length(length as usize))
                }
                else {
                    return err!(ExceptionCode::RangeCheckError)
                }
                self.basic_use_gas(0);
            },
            Some(InstructionOptions::LengthAndIndex) => {
                self.basic_use_gas(0);
                // This is currently needed only for special-case BLKPUSH command and works the same way
                // as InstructionOptions::StackRegisterPair(WhereToGetParams::GetFromLastByte)
                let params = self.cc.last_cmd();
                let (length, index) = (params >> 4, params & 0x0F);
                self.cmd.ictx.params.push(
                    InstructionParameter::LengthAndIndex(
                        LengthAndIndex {
                            length: length as usize,
                            index: index as usize
                        }
                    )
                )
            },
            Some(InstructionOptions::LengthMinusOne(ref range)) => {
                self.cmd.ictx.params.push(
                    InstructionParameter::Length(
                        1 + if *range == (0..8) {
                            self.cc.last_cmd() & 0x07
                        } else if *range == (0..256) {
                            self.cc.next_cmd()?
                        } else {
                            return err!(ExceptionCode::RangeCheckError)
                        } as usize
                    )
                );
                self.basic_use_gas(0);
            },
            Some(InstructionOptions::LengthMinusOneAndIndexMinusOne) => {
                let params = self.cc.next_cmd()?;
                self.basic_use_gas(0);
                let (l_minus_1, i_minus_1) = (params >> 4, params & 0x0F);
                self.cmd.ictx.params.push(
                    InstructionParameter::LengthAndIndex(
                        LengthAndIndex {
                            length: (l_minus_1 + 1) as usize,
                            index: (i_minus_1 + 1) as usize
                        }
                    )
                )
            },
            Some(InstructionOptions::LengthMinusTwoAndIndex) => {
                let params = self.cc.next_cmd()?;
                self.basic_use_gas(0);
                let (l_minus_2, i) = (params >> 4, params & 0x0F);
                self.cmd.ictx.params.push(
                    InstructionParameter::LengthAndIndex(
                        LengthAndIndex {
                            length: (l_minus_2 + 2) as usize,
                            index: i as usize
                        }
                    )
                )
            },
            Some(InstructionOptions::Pargs(ref range)) => {
                if *range == (0..16) {
                    self.cmd.ictx.params.push(
                        InstructionParameter::Pargs((self.cc.last_cmd() & 0x0F) as usize)
                    )
                } else {
                    return err!(ExceptionCode::RangeCheckError)
                }
                self.basic_use_gas(0);
            },
            Some(InstructionOptions::Rargs(ref range)) => {
                if *range == (0..16) {
                    self.cmd.ictx.params.push(
                        InstructionParameter::Rargs((self.cc.last_cmd() & 0x0F) as usize)
                    )
                } else {
                    return err!(ExceptionCode::RangeCheckError)
                }
                self.basic_use_gas(0);
            },
            Some(InstructionOptions::StackRegister(ref range)) => {
                if *range == (0..16) {
                    self.cmd.ictx.params.push(
                        InstructionParameter::StackRegister((self.cc.last_cmd() & 0x0F) as usize)
                    )
                } else if *range == (0..256) {
                    self.cmd.ictx.params.push(
                        InstructionParameter::StackRegister(self.cc.next_cmd()? as usize)
                    )
                } else {
                    return err!(ExceptionCode::RangeCheckError)
                }
                self.basic_use_gas(0);
            },
            Some(InstructionOptions::StackRegisterPair(ref place)) => {
                let (ra, rb) = match place {
                    WhereToGetParams::GetFromLastByte2Bits => {
                        let opcode_ra_rb = self.cc.last_cmd();
                        ((opcode_ra_rb >> 2) & 0x03, opcode_ra_rb & 0x03)
                    }
                    WhereToGetParams::GetFromLastByte => {
                        let opcode_ra_rb = self.cc.last_cmd();
                        ((opcode_ra_rb & 0xF0) >> 4, opcode_ra_rb & 0x0F)
                    }
                    WhereToGetParams::GetFromNextByte => {
                        let ra_rb = self.cc.next_cmd()?;
                        ((ra_rb & 0xF0) >> 4, ra_rb & 0x0F)
                    }
                    WhereToGetParams::GetFromNextByteLong => {
                        let rb = self.cc.next_cmd()?;
                        (0,rb)
                    }
                    _ => (0, 0)
                };
                self.basic_use_gas(0);
                self.cmd.ictx.params.push(
                    InstructionParameter::StackRegisterPair(
                        RegisterPair {
                            ra: ra as usize,
                            rb: rb as usize
                        }
                    )
                )
            },
            Some(InstructionOptions::StackRegisterTrio(ref place)) => {
                let last = self.cc.last_cmd();
                let (ra, rb, rc) = match place {
                    WhereToGetParams::GetFromLastByte2Bits => {
                        // INDEX3 2 bits per index
                        ((last >> 4) & 0x03, (last >> 2) & 0x03, last & 0x03)
                    }
                    _ => {
                        // Three-arguments functions are 2-byte 4ijk XCHG3 instructions
                        // And 54[0-7]ijk long-form XCHG3 - PUSH3
                        // We assume that in the second case 0x54 byte is already consumed,
                        // and we have to deal with *ijk layout for arguments
                        let rb_rc = self.cc.next_cmd()?;
                        (last & 0x0F, rb_rc >> 4, rb_rc & 0x0F)
                    }
                };
                self.basic_use_gas(0);
                self.cmd.ictx.params.push(
                    InstructionParameter::StackRegisterTrio(
                        RegisterTrio {
                            ra: ra as usize,
                            rb: rb as usize,
                            rc: rc as usize
                        }
                    )
                )
            }
            Some(InstructionOptions::Dictionary(offset, bits)) => {
                self.use_gas(Gas::basic_gas_price(offset + 1 + bits, 0));
                let mut code = self.cmd_code.clone();
                code.shrink_data(offset..);
                // TODO: need to check this failure case
                let slice = code.get_dictionary_opt().unwrap_or_else(|| SliceData::default());
                self.cmd.ictx.params.push(InstructionParameter::Slice(slice));
                let length = code.get_next_int(bits)? as usize;
                *self.cc.code_mut() = code;
                self.cmd.ictx.params.push(InstructionParameter::Length(length))
            }
            Some(InstructionOptions::Bytestring(offset, r, x, bytes)) => {
                self.use_gas(Gas::basic_gas_price(offset + r + x, 0));
                let slice = self.extract_slice(offset, r, x, 0, bytes)?;
                if slice.remaining_bits() % 8 != 0 {
                    return err!(ExceptionCode::InvalidOpcode)
                }
                self.cmd.ictx.params.push(InstructionParameter::Slice(slice))
            }
            Some(InstructionOptions::Bitstring(offset, r, x, refs)) => {
                self.use_gas(Gas::basic_gas_price(offset + r + x, 0));
                let mut slice = self.extract_slice(offset, r, x, refs, 0)?;
                slice.trim_right();
                self.cmd.ictx.params.push(InstructionParameter::Slice(slice));
            }
            None => { self.basic_use_gas(0); }
        }
        Ok(())
    }

    // raises the exception and tries to dispatch it via c(2).
    // If c(2) is not set, returns that exception, otherwise, returns None
    fn raise_exception(&mut self, err_opt: Option<failure::Error>, _undo: bool) -> Status {
        let (err, exception) = if let Some(err) = err_opt {
            if let Some(exception) = tvm_exception_full(&err) {
                (err, exception)
            } else {
                log::trace!(target: "tvm", "BAD CODE: {}\n", self.cmd_code);
                return Err(err)
            }
        } else {
            return Ok(())
        };
        // if undo {
        //     self.undo();
        // }
        if exception.exception_code() == Some(ExceptionCode::OutOfGas) {
            self.step += 1;
            log::trace!(target: "tvm", "OUT OF GAS CODE: {}\n", self.cmd_code);
            return Err(err)
        }
        self.try_use_gas(Gas::exception_price())?;
        if let Err(err) = self.gas.check_gas_remaining() {
            return self.raise_exception(Some(err), false)
        }
        let n = self.cmd.vars.len();
        // self.trace_info(EngineTraceInfoType::Exception, self.gas_used(), Some(format!("EXCEPTION: {}", err)));
        if let Some(c2) = self.ctrls.get_mut(2) {
            self.cc.stack.push(exception.value.clone());
            self.cc.stack.push(int!(exception.exception_or_custom_code()));
            c2.as_continuation_mut().map(|cdata| cdata.nargs = 2)?;
            switch(Ctx::new(self), ctrl!(2))?;
        } else if let Some(number) = exception.is_normal_termination() {
            let cont = ContinuationData::with_type(ContinuationType::Quit(number));
            self.cmd.push_var(StackItem::Continuation(Arc::new(cont)));
            self.cc.stack.push(exception.value.clone());
            self.cmd.vars[n].as_continuation_mut().map(|cdata| cdata.nargs = 1)?;
            switch(Ctx::new(self), var!(n))?;
        } else {
            self.trace_info(EngineTraceInfoType::Exception, self.gas_used(), Some(format!("UNHANDLED EXCEPTION: {}", err)));
            log::trace!(target: "tvm", "BAD CODE: {}\n", self.cmd_code);
            return Err(err)
        }
        Ok(())
    }

    /// Set code page for interpret bytecode. now only code page 0 is supported
    pub(in crate::executor) fn code_page_mut(&mut self) -> &mut isize {
        &mut self.code_page
    }

    pub(in crate::executor) fn config_param(&self, index: usize) -> ResultRef<StackItem> {
        let tuple = self.ctrl(7)?.as_tuple()?;
        let tuple = tuple.first().ok_or(ExceptionCode::RangeCheckError)?.as_tuple()?;
        Ok(tuple.get(index).ok_or(ExceptionCode::RangeCheckError)?)
    }

    pub(in crate::executor) fn rand(&self) -> ResultRef<IntegerData> {
        self.config_param(6)?.as_integer()
    }

    pub(in crate::executor) fn set_rand(&mut self, rand: IntegerData) -> Status {
        let mut tuple = self.ctrl_mut(7)?.as_tuple_mut()?;
        let mut t1 = tuple.first_mut().ok_or(ExceptionCode::RangeCheckError)?.as_tuple_mut()?;
        *t1.get_mut(6).ok_or(ExceptionCode::RangeCheckError)? = StackItem::Integer(Arc::new(rand));
        self.use_gas(Gas::tuple_gas_price(t1.len()));
        *tuple.first_mut().ok_or(ExceptionCode::RangeCheckError)? = StackItem::Tuple(t1);
        self.use_gas(Gas::tuple_gas_price(tuple.len()));
        *self.ctrl_mut(7)? = StackItem::Tuple(tuple);
        Ok(())
    }

    #[allow(dead_code)]
    fn undo(&mut self) {
        while let Some(undo) = self.cmd.undo.pop() {
            let mut ctx = Ctx::new(self);
            match undo {
                Undo::WithCode(f, c) => f(&mut ctx, c),
                Undo::WithCodePair(f, c1, c2) => f(&mut ctx, c1, c2),
                Undo::WithCodeTriplet(f, c1, c2, c3) => f(&mut ctx, c1, c2, c3),
                Undo::WithAddressAndNargs(f, a, n) => f(&mut ctx, a, n),
                Undo::WithSaveList(f, l) => f(&mut ctx, l),
                Undo::WithSize(f, i) => f(&mut ctx, i),
                Undo::WithSizeDataAndCode(f, i, v, a) => f(&mut ctx, i, v, a),
            }
        }
    }

}
