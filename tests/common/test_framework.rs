/*
* Copyright (C) 2019-2024 EverX. All Rights Reserved.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific EVERX DEV software governing permissions and
* limitations under the License.
*/

#![allow(dead_code)]
use std::{convert::Into, ffi::CStr, os::raw::c_char, sync::Arc};
use ever_assembler::{CompileError, compile_code};
use ever_vm::{
    int,
    error::{tvm_exception_code, tvm_exception_or_custom_code, TvmError},
    executor::{Engine, gas::gas_state::Gas, IndexProvider, BehaviorModifiers}, smart_contract_info::SmartContractInfo,
    stack::{Stack, StackItem, integer::IntegerData, savelist::SaveList},
    types::Exception
};
use ever_block::{
    BuilderData, Cell, Error, Result, SliceData,
    types::ExceptionCode, HashmapE, HashmapType, BocWriter
};

pub type Bytecode = SliceData;

fn logger_init() {
    // do not init twice
    if log::log_enabled!(log::Level::Info) {
        return
    }
    let log_level = if cfg!(feature = "verbose") {
        log::LevelFilter::Trace
    } else {
        log::LevelFilter::Info
    };
    let encoder_boxed = Box::new(log4rs::encode::pattern::PatternEncoder::new("{m}"));
    let config = if cfg!(feature = "log_file") {
        let file = log4rs::append::file::FileAppender::builder()
            .encoder(encoder_boxed)
            .build("log/log.txt")
            .unwrap();
        log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("file", Box::new(file)))
            .build(log4rs::config::Root::builder().appender("file").build(log_level))
            .unwrap()
    } else {
        let console = log4rs::append::console::ConsoleAppender::builder()
            .encoder(encoder_boxed)
            .build();
        log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder().build("console", Box::new(console)))
            .build(log4rs::config::Root::builder().appender("console").build(log_level))
            .unwrap()
    };
    log4rs::init_config(config).ok();
}

pub struct TestCaseInputs {
    code: String,
    ctrls: SaveList,
    stack: Stack,
    refs: Vec<Cell>,
    gas: Option<Gas>,
    library: HashmapE,
    behavior_modifiers: Option<BehaviorModifiers>,
    capabilities: u64,
    block_version: u32,
    skip_fift_check: bool,
    index_provider: Option<Arc<dyn IndexProvider>>,
}

impl TestCaseInputs {

    pub fn new(
        code: String, 
        stack: Stack, 
        refs: Vec<Cell>, 
        capabilities: u64
    ) -> TestCaseInputs {
        logger_init();
        TestCaseInputs {
            code,
            ctrls: SaveList::new(),
            stack,
            refs,
            gas: None,
            library: HashmapE::with_bit_len(256),
            behavior_modifiers: None,
            capabilities,
            block_version: 0,
            skip_fift_check: false,
            index_provider: None,
        }
    }

    pub fn with_ref(mut self, cell: Cell) -> TestCaseInputs {
        assert!(self.refs.len() < 4);
        self.refs.push(cell);
        self
    }

    pub fn with_refs(mut self, refs: Vec<Cell>) -> TestCaseInputs {
        self.refs = refs;
        self
    }

    pub fn with_root_data(self, root_data: Cell) -> TestCaseInputs {
        self.with_ctrl(4, StackItem::Cell(root_data))
    }

    pub fn with_temp_data(self, temp_data: SmartContractInfo) -> TestCaseInputs {
        self.with_ctrl(7, temp_data.into_temp_data_item())
    }

    // do not run with stack - use refs, then do PUSHREF*
    pub fn with_stack(mut self, stack: Stack) -> TestCaseInputs {
        self.stack = stack;
        self
    }

    pub fn with_capability(mut self, capability: ever_block::GlobalCapabilities) -> TestCaseInputs {
        self.skip_fift_check = true;
        self.capabilities |= capability as u64;
        self
    }

    pub fn with_block_version(mut self, block_version: u32) -> TestCaseInputs {
        self.skip_fift_check = true;
        self.block_version = block_version;
        self
    }

    pub fn skip_fift_check(mut self, skip: bool) -> TestCaseInputs {
        if skip {
            self.skip_fift_check = skip;
        }
        self
    }

    pub fn with_index_provider(mut self, index_provider: Arc<dyn IndexProvider>) -> TestCaseInputs {
        self.index_provider = Some(index_provider);
        self
    }

    pub fn with_ctrl(mut self, ctrl: usize, mut item: StackItem) -> TestCaseInputs {
        self.ctrls.put(ctrl, &mut item)
            .expect("test arguments must be valid");
        self
    }

    pub fn with_gas(mut self, gas: Gas) -> TestCaseInputs {
        self.gas = Some(gas);
        self
    }

    pub fn with_gas_limit(self, gas_limit: i64) -> TestCaseInputs {
        self.with_gas(Gas::test_with_limit(gas_limit))
    }

    pub fn with_library(mut self, library: HashmapE) -> TestCaseInputs {
        self.library = library;
        self
    }

    pub fn with_behavior_modifiers(mut self, behavior_modifiers: BehaviorModifiers) -> TestCaseInputs {
        self.skip_fift_check = true;
        self.behavior_modifiers = Some(behavior_modifiers);
        self
    }

    pub fn expect_bytecode(self, bytecode: Vec<u8>) -> TestCaseInputs {
        self.expect_bytecode_extended(bytecode, None)
    }

    pub fn expect_bytecode_extended(self, bytecode: Vec<u8>, message: Option <&str>) -> TestCaseInputs {
        let inputcode = SliceData::new(bytecode);
        let compilation_result = compile_code(&self.code);
        match compilation_result {
            Ok(ref selfcode) => {
                let mut selfcode = selfcode.clone();
                let mut bytevec = vec![];
                while selfcode.remaining_bits() != 0 {
                    bytevec.append(&mut selfcode.get_bytestring(0));
                    if selfcode.remaining_references() > 0 {
                        selfcode = SliceData::load_cell(selfcode.reference(0).unwrap()).unwrap();
                    } else {
                        break;
                    }
                }
                bytevec.push(0x80);
                let selfcode = SliceData::new(bytevec);
                if !selfcode.eq(&inputcode) {
                    match message {
                        Some(msg) => panic!(
                            "{}Bytecode did not match:\n Expected: <{:x?}>\n But was: <{:x?}>",
                            msg, inputcode, selfcode),
                        None => panic!(
                            "Bytecode did not match:\n Expected: <{:x?}>\n But was: <{:x?}>",
                            inputcode, selfcode),
                    }
                };
            },
            Err(e) => {
                match message {
                    Some(msg) => panic!("{}{}", msg, e),
                    None => panic!("{}", e),
                }
            }
        }
        self
    }

    pub fn expect_compilation_failure(self, error: CompileError) -> TestCaseInputs {
        self.expect_compilation_failure_extended(error, None)
    }

    pub fn expect_compilation_failure_extended(self, error: CompileError, message: Option <&str>) -> TestCaseInputs {
        let compilation_result = compile_code(&self.code);
        match message {
            None => {
                let actual = compilation_result.expect_err(&format!("Error expected {}", error));
                assert_eq!(
                    error, actual,
                    "Expected (left): <{}>, but was (right): <{}>.",
                    error, actual
                )
            },
            Some(msg) => {
                let actual = compilation_result.expect_err(&format!("{}. Error expected {}", msg, error));
                assert_eq!(
                    error, actual,
                    "{}\nExpected (left): <{}>, but was (right): <{}>.",
                    msg, error, actual
                )
            },
        }
        self
    }
}

impl From<TestCaseInputs> for TestCase {
    fn from(inputs: TestCaseInputs) -> Self {
        TestCase::new(inputs)
    }
}

pub struct TestCase {
    executor: Option<Engine>,
    compilation_result: std::result::Result<Bytecode, CompileError>,
    execution_result: Result<i32>,
}

impl TestCase {
    fn executor(&self, message: Option<&str>) -> &Engine {
        match self.executor {
            Some(ref exectuor) => exectuor,
            None => {
                let err = self.compilation_result.as_ref().unwrap_err();
                match message {
                    Some(msg) => panic!("{}No executor was created, because of bytecode compilation error {:?}", msg, err),
                    None => panic!("No executor was created, because of bytecode compilation error {:?}", err)
                }
            }
        }
    }
}

fn compare_with_fift(
    bytecode: SliceData,
    library: HashmapE,
    code: String,
    executor: &Engine,
    execution_result: &Result<i32>,
    gas_remaining: i32
) {
    #[cfg(windows)]
    let lib_name = "vm_run_shared.dll";
    #[cfg(not(windows))]
    let lib_name = "./vm_run_shared.so";
    if let Ok(lib) = libloading::Library::new(lib_name) {
        let mut data = vec![];
        assert!(bytecode.pos() == 0);
        let mut roots = vec![bytecode.cell_opt().unwrap().clone()];
        if let Some(root) = library.data() {
            roots.push(root.clone());
        }
        let bag = BocWriter::with_roots(roots).unwrap();
        bag.write(&mut data).unwrap();
        // code is written to BOC and can be checked with FIFT
        // "fift.boc" file>B B>boc <s 1000000 0x48 runvmx .s
        // std::fs::write("../target/check/fift.boc", data.as_slice()).ok();
        let size = data.len() * 8;
        let fift_result;
        unsafe {
            let run_boc: libloading::Symbol<
                unsafe extern "C" fn(*const u8, i32, i32) -> *mut c_char
            > = lib.get(b"run_vm_boc_with_gas_and_commit").unwrap();
            let free_mem: libloading::Symbol<
                unsafe extern "C" fn(*const c_char) -> *mut c_char
            > = lib.get(b"free_mem").unwrap();
            let res = run_boc(data.as_ptr(), size as i32, gas_remaining);
            fift_result = CStr::from_ptr(res).to_string_lossy().into_owned().trim().to_string();
            free_mem(res);
        }
        let tvm_result = match execution_result {
            Ok(ref result) => {
                let stack = executor.get_stack_result_fift();
                match stack.is_empty() {
                    true => format!("{} {}{}", result, executor.gas_used(), executor.get_committed_state_fift()),
                    false => format!("{} {} {}{}", stack, result, executor.gas_used(), executor.get_committed_state_fift())
                }
            }
            Err(ref err) => {
                if let Some(ExceptionCode::OutOfGas) = tvm_exception_code(err) {
                    let gas = executor.gas_used();
                    format!("{} {} {}{}", gas, !(ExceptionCode::OutOfGas as i32), gas, executor.get_committed_state_fift())
                } else {
                    let err = tvm_exception_or_custom_code(err);
                    format!("0 {} {}{}", err, executor.gas_used(), executor.get_committed_state_fift())
                }
            }
        };
        if tvm_result != fift_result {
            log::info!("bytecode: {}\n", &StackItem::Slice(bytecode).dump_as_fift());
            log::info!("code:\n{}\n", code);
            assert_eq!(tvm_result, fift_result);
        }
    } else {
        panic!("no shared dll found")
    }
}

impl TestCase {
    pub(super) fn new(args: TestCaseInputs) -> TestCase {
        match compile_code(&args.code) {
            Ok(bytecode) => {
                let code = if args.refs.is_empty() {
                    bytecode.clone()
                } else if bytecode.remaining_references() + args.refs.len() <= BuilderData::references_capacity() {
                    let mut builder = bytecode.as_builder();
                    args.refs.iter().rev().for_each(|reference| {
                        builder.checked_prepend_reference(reference.clone()).unwrap();
                    });
                    SliceData::load_builder(builder).unwrap()
                } else {
                    log::error!(target: "compile", "Cannot use 4 refs with long code");
                    bytecode.clone()
                };
                log::trace!(target: "compile", "code: {}\n", code);
                let mut executor = Engine::with_capabilities(args.capabilities)
                    .setup_with_libraries(
                        code.clone(),
                        Some(args.ctrls.clone()),
                        Some(args.stack.clone()),
                        args.gas.clone(),
                        vec![args.library.clone()]
                    );
                executor.set_block_version(args.block_version);
                if let Some(modifiers) = args.behavior_modifiers {
                    executor.modify_behavior(modifiers);
                }
                if let Some(index_provider) = args.index_provider.clone() {
                    executor.set_index_provider(index_provider)
                }
                let execution_result = executor.execute();
                if cfg!(feature = "fift_check") && args.stack.is_empty() && args.ctrls.is_empty() && !args.skip_fift_check {
                    let gas = args.gas.map(|gas| gas.get_gas_remaining() as i32).unwrap_or(1000000);
                    compare_with_fift(code, args.library, args.code, &executor, &execution_result, gas)
                }
                TestCase {
                    executor: Some(executor),
                    compilation_result: Ok(bytecode),
                    execution_result,
                }
            }
            Err(e) => TestCase {
                executor: None,
                compilation_result: Err(e),
                execution_result: Ok(-1),
            }
        }
    }

    // TODO: call this from fn new
    pub fn with_bytecode(
        code: Bytecode, 
        ctrls: Option<SaveList>, 
        stack: Option<Stack>, 
        library: HashmapE
    ) -> TestCase {
        logger_init();

        let mut executor = Engine::with_capabilities(0).setup_with_libraries(
            code.clone(), 
            ctrls.clone(),
            stack.clone(),
            None, 
            vec![library.clone()]
        );
        let execution_result = executor.execute();
        if cfg!(feature = "fift_check") && stack.is_none() && ctrls.is_none() {
            compare_with_fift(
                code.clone(), 
                library, 
                format!("{:x}", code), 
                &executor, 
                &execution_result,  
                1000000
            )
        }
        log::trace!("bytecode: {}", code);
        TestCase {
            executor: Some(executor),
            compilation_result: Ok(code),
            execution_result,
        }
    }

    pub fn get_root(&self) -> Option<Cell> {
        if let Some(ref eng) = self.executor {
            if let StackItem::Cell(c) = eng.get_committed_state().get_root() {
                return Some(c.clone())
            }
        }
        None
    }

    pub fn get_actions(&self) -> Option<Cell> {
        if let Some(ref eng) = self.executor {
            if let StackItem::Cell(c) = eng.get_committed_state().get_actions() {
                return Some(c.clone())
            }
        }
        None
    }
}

pub trait Expects {
    fn expect_stack(self, stack: &Stack) -> TestCase;
    fn expect_stack_extended(self, stack: &Stack, message: Option<&str>) -> TestCase;
    fn expect_empty_stack(self) -> TestCase;
    fn expect_int_stack(self, stack_contents: &[i32]) -> TestCase;
    fn expect_item(self, stack_item: StackItem) -> TestCase;
    fn expect_item_extended(self, stack_item: StackItem, message: Option<&str>) -> TestCase;
    fn expect_success(self) -> TestCase;
    fn expect_success_extended(self, message: Option <&str>) -> TestCase;
    fn expect_ctrl(self, ctrl: usize, item: &StackItem) -> TestCase;
    fn expect_ctrl_extended(self, ctrl: usize, item: &StackItem, message: Option<&str>) -> TestCase;
    fn expect_failure(self, exception_code: ExceptionCode) -> TestCase;
    fn expect_custom_failure(self, custom_code: i32) -> TestCase;
    fn expect_custom_failure_extended<F : Fn(&Exception) -> bool>(self, op: F, exc_name: &str, message: Option <&str>) -> TestCase;
    fn expect_failure_extended(self, exception_code: ExceptionCode, message: Option <&str>) -> TestCase;
    fn expect_root_data(self, cell: Cell) -> TestCase;
    fn expect_same_results(self, other: Self);
    fn expect_gas(self, max_gas_limit: i64, gas_limit: i64, gas_credit: i64, gas_remaining: i64) -> TestCase;
    fn expect_steps(self, steps: u32) -> TestCase;
    fn stack(self) -> Stack;
}

impl<T: Into<TestCase>> Expects for T {
    fn expect_stack(self, stack: &Stack) -> TestCase {
        self.expect_stack_extended(stack, None)
    }

    fn expect_stack_extended(self, stack: &Stack, message: Option<&str>) -> TestCase {
        let test_case: TestCase = self.into();
        let executor = test_case.executor(message);
        match test_case.execution_result {
            Ok(_) => {
                if !executor.eq_stack(stack) {
                    if let Some(msg) = message {
                        log::info!("{}", msg)
                    }
                    log::info!(target: "tvm", "\nExpected stack: \n{}", stack);
                    log::info!(
                        target: "tvm",
                        "\n{}\n",
                        executor.dump_stack("Actual Stack:", false)
                    );
                    panic!("Stack is not expected")
                }
            }
            // TODO this is not quite right: execution may fail but still produce a stack
            Err(ref e) => {
                log::info!(target: "tvm", "\nExpected stack: \n{}", stack);
                print_failed_detail_extended(&test_case, e, message);
                panic!("Execution error: {:?}", e)
            }
        }
        test_case
    }

    fn expect_empty_stack(self) -> TestCase {
        self.expect_stack(&Stack::new())
    }

    // Order of items in array like in spec docs right item is top item
    fn expect_int_stack(self, stack_contents: &[i32]) -> TestCase {
        let mut stack = Stack::new();
        for element in stack_contents {
            stack.push(int!(*element));
        }
        self.expect_stack(&stack)
    }

    fn expect_item(self, stack_item: StackItem) -> TestCase {
        self.expect_item_extended(stack_item, None)
    }

    fn expect_item_extended(self, stack_item: StackItem, message: Option<&str>) -> TestCase {
        self.expect_stack_extended(Stack::new().push(stack_item), message)
    }

    fn expect_success(self) -> TestCase {
       self.expect_success_extended(None)
    }

    fn expect_success_extended(self, message: Option <&str>) -> TestCase {
        let test_case: TestCase = self.into();
        let executor = test_case.executor(message);
        print_stack(&test_case, executor);
        if let Err(ref e) = test_case.execution_result {
            match message {
                None => {
                    print_failed_detail_extended(&test_case, e, message);
                    panic!("Execution error: {:?}", e);
                }
                Some(msg) => {
                    print_failed_detail_extended(&test_case, e, message);
                    panic!("{}\nExecution error: {:?}", msg, e);
                }
            }
        }
        test_case
    }

    fn expect_ctrl(self, ctrl: usize, item: &StackItem) -> TestCase {
        self.expect_ctrl_extended(ctrl, item, None)
    }

    fn expect_ctrl_extended(self, ctrl: usize, item: &StackItem, message: Option<&str>) -> TestCase {
        let test_case: TestCase = self.into();
        let executor = test_case.executor(message);
        match test_case.execution_result {
            Ok(_) => executor.assert_ctrl(ctrl, item),
            Err(ref e) => {
                print_failed_detail_extended(&test_case, e, message);
                panic!("Execution error: {}", e);
            }
        };
        test_case
    }

    fn expect_failure(self, exception_code: ExceptionCode) -> TestCase {
        self.expect_failure_extended(exception_code, None)
    }

    fn expect_custom_failure_extended<F : Fn(&Exception) -> bool>(
        self, 
        op: F, 
        exc_name: &str, 
        message: Option <&str>
    ) -> TestCase {
        let test_case: TestCase = self.into();
        let executor = test_case.executor(message);
        match test_case.execution_result {
            Ok(_) => {
                log::info!(
                    target: "tvm",
                    "Expected failure: {}, however execution succeeded.",
                    exc_name
                );
                print_stack(&test_case, executor);
                match message {
                    None => panic!(
                        "Expected failure: {}, however execution succeeded.", 
                        exc_name
                    ),
                    Some(msg) => panic!(
                        "{}.\nExpected failure: {}, however execution succeeded.", 
                        msg, exc_name
                    )
                }
            }
            Err(ref e) => {
                if let Some(TvmError::TvmExceptionFull(e, msg2)) = e.downcast_ref() {
                    if op(e) {
                        match message {
                            Some(msg) => panic!(
                                "{} - {}\nNon expected exception: {}, expected: {}", 
                                msg2, msg, e, exc_name
                            ),
                            None => panic!(
                                "{}\nNon expected exception: {}, expected: {}", 
                                msg2, e, exc_name
                            )
                        }
                    }
                } else {
                    let code = e.downcast_ref::<ExceptionCode>();
                    match code {
                        Some(code) => {
                            let e = Exception::from(*code);
                            if op(&e) {
                                panic!("Non expected exception: {}, expected: {}", e, exc_name)
                            }
                        }
                        None => {
                            if op(&Exception::from(ExceptionCode::FatalError)) {
                                panic!("Non expected exception: {}, expected: {}", e, exc_name)
                            }
                        }
                    }
                }
            }
        }
        test_case
    }

    fn expect_custom_failure(self, custom_code: i32) -> TestCase {
        self.expect_custom_failure_extended(
            |e| e.custom_code() != Some(custom_code), 
            "custom exception", 
            None
        )
    }

    fn expect_failure_extended(
        self, 
        exception_code: ExceptionCode, 
        message: Option <&str>
    ) -> TestCase {
       self.expect_custom_failure_extended(
           |e| e.exception_code() != Some(exception_code),
           &format!("{}", exception_code),
           message
       )
    }

    fn expect_root_data(self, cell: Cell) -> TestCase {
        self.expect_ctrl(4, &StackItem::Cell(cell))
    }

    fn expect_same_results(self, other: Self) {
        let case1 = self.expect_success();
        let case2 = other.expect_success();
        let stack = case2.executor.unwrap().withdraw_stack();
        case1.expect_stack_extended(&stack, Some("results are not the same!"));
    }

    fn expect_gas(
        self,
        max_gas_limit: i64,
        gas_limit: i64,
        gas_credit: i64,
        gas_remaining: i64
    ) -> TestCase {
        let test_case: TestCase = self.into();
        let gas = test_case.executor(None).get_gas();
        assert_eq!(gas.get_gas_limit_max(), max_gas_limit);
        assert_eq!(gas.get_gas_limit(), gas_limit);
        assert_eq!(gas.get_gas_credit(), gas_credit);
        assert_eq!(gas.get_gas_remaining(), gas_remaining);
        test_case
    }

    fn expect_steps(self, steps: u32) -> TestCase {
        let test_case: TestCase = self.into();
        assert_eq!(test_case.executor(None).steps(), steps);
        test_case
    }

    fn stack(self) -> Stack {
        let test_case: TestCase = self.into();
        test_case.executor(None).stack().clone()
    }
}

fn print_stack(test_case: &TestCase, executor: &Engine) {
    if test_case.execution_result.is_ok() {
        log::info!(target: "tvm", "Post-execution:\n");
        log::info!(target: "tvm", "{}", executor.dump_stack("Post-execution stack state", false));
        log::info!(target: "tvm", "{}", executor.dump_ctrls(false));
    }
}

#[allow(dead_code)]
fn print_failed_detail(case: &TestCase, exception: &Error) {
    print_failed_detail_extended(case, exception, None)
}

fn print_failed_detail_extended(case: &TestCase, exception: &Error, message: Option <&str>) {
    log::info!(target: "tvm", "exception: {:?}\n", exception);
    let msg2 = if let Some(TvmError::TvmExceptionFull(_e, msg2)) = exception.downcast_ref() {
        msg2.clone()
    } else {
        String::new()
    };
    match message {
        Some(ref msg) => log::info!(
            target: "tvm",
            "{} failed with {} {}.\nBytecode: {:x?}\n",
            msg, exception, msg2, case.compilation_result
        ),
        None => log::info!(
            target: "tvm",
            "failed with {} {}.\nBytecode: {:x?}\n",
            exception, msg2, case.compilation_result
        )
    }
}

pub fn test_case_with_refs(code: &str, references: Vec<Cell>) -> TestCaseInputs {
    TestCaseInputs::new(code.to_string(), Stack::new(), references, 0)
}

pub fn test_case_with_ref(code: &str, reference: Cell) -> TestCaseInputs {
    TestCaseInputs::new(code.to_string(), Stack::new(), vec![reference], 0)
}

pub fn test_case(code: impl ToString) -> TestCaseInputs {
    TestCaseInputs::new(code.to_string(), Stack::new(), vec![], 0)
}

pub fn test_case_with_bytecode(code: Bytecode) -> TestCase {
    TestCase::with_bytecode(code, None, None, Default::default())
}