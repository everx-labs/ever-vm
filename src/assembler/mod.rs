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

use std::{collections::HashMap, ops::RangeInclusive};
use ton_types::{Cell, SliceData};

mod errors;
pub use errors::{
    CompileError, OperationError, ParameterError, Position, 
    ToOperationParameterError,
};

mod macros;
mod parse;
mod complex;
mod simple;

mod writer;
use writer::{CodePage0, Writer};

// Basic types *****************************************************************
/// Operation Compilation result
type CompileResult = Result<(), OperationError>;
type CompileHandler<T> = fn(&mut Engine<T>, &Vec<&str>, destination:&mut T) -> CompileResult;

// CompileError::Operation handlers ***********************************************************
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

impl<T: Writer> Default for CommandContext<T> {
    fn default() -> Self {
        Self {
            operation: Default::default(),
            line_no_cmd: 0,
            char_no_cmd: 0,
            line_no_par: 0,
            char_no_par: 0,
            rule_option: None,
        }
    }
    
}
impl<T: Writer> CommandContext<T> {
    fn new(operation: String, char_no_cmd: usize, line_no_cmd: usize, rule_option: Option<CompileHandler<T>>) -> Self {
        Self {
            operation,
            line_no_cmd,
            char_no_cmd,
            line_no_par: 0,
            char_no_par: 0,
            rule_option,
        }
    }
    fn abort<X>(&self, error: OperationError) -> Result<X, CompileError> {
        Err(CompileError::operation(self.line_no_cmd, self.char_no_cmd, self.operation.clone(), error))
    }
    fn has_command(&self) -> bool {
        self.rule_option.is_some()
    }
    fn compile(
        &mut self,
        destination: &mut T,
        par: &mut Vec<(usize, usize, &str, bool)>,
        engine: &mut Engine<T>,
    ) -> Result<(), CompileError> {
        let rule = match self.rule_option.as_ref() {
            Some(rule) => rule,
            None => return Ok(())
        };
        let (line_no, char_no) = engine.set_pos(self.line_no_par, self.char_no_par);
        let mut n = par.len();
        loop {
            let par = &par[0..n].iter().map(|(_, _, e, _)| *e).collect::<Vec<_>>();
            match rule(engine, par, destination) {
                Ok(_) => break,
                Err(OperationError::TooManyParameters) if n != 0 => {
                    n -= 1;
                }
                Err(e) => return self.abort(e)
            }
        }
        engine.set_pos(line_no, char_no);
        self.rule_option = None;
        // detecting some errors here if was
        if n > 1 {
            for (line, column, _, was_comma) in &par[1..n] {
                if !*was_comma {
                    return Err(CompileError::syntax(*line, *column, "Missing comma"))
                }
            }
        }
        par.drain(..n);
        if !par.is_empty() {
            let (line, column, token, was_comma) = par.remove(0);
            let position = Position { line, column };
            if was_comma {
                return Err(CompileError::Operation(
                    position,
                    self.operation.clone(),
                    OperationError::TooManyParameters,
                ))
            } else if n == 0 {
                // or CompileError::Operation
                return Err(CompileError::Operation(
                    position,
                    self.operation.clone(),
                    OperationError::TooManyParameters,
                ))
            } else {
                // or CompileError::Syntax "missing comma"
                return Err(CompileError::UnknownOperation(
                    position, token.into()
                ))
            }
        }
        Ok(())
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
    pub fn new() -> Engine<T> {
        let mut ret = Engine::<T> {
            line_no: 1,
            char_no: 1,
            COMPILE_ROOT: HashMap::new(),
        };
        ret.add_complex_commands();
        ret.add_simple_commands();
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
        let l = std::mem::replace(&mut self.line_no, line_no);
        let c = std::mem::replace(&mut self.char_no, char_no);
        (l, c)
    }

    fn compile(&mut self, source: &str) -> Result<T, CompileError> {
        let mut ret = T::new();
        let mut par: Vec<(usize, usize, &str, bool)> = Vec::new();
        let mut acc = (0, 0);
        let mut expect_comma = false;
        let mut comma_found = false;
        let mut was_comma = false; // was comma before token
        let mut was_newline = false; // was line break before token
        let mut in_block = 0;
        let mut in_comment = false;
        let mut command_ctx = CommandContext::default();
        for ch in source.chars().chain(" ".chars()) {
            let mut newline_found = false;
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
                    was_newline = true;
                }
                acc = (new_s1, new_s1);
                continue;
            }
            // Analyze char
            if Engine::<T>::is_whitespace(ch) {
                if (ch == '\r') || (ch == '\n') {
                    newline_found = true;
                    was_newline = true;
                }
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
                    return Err(CompileError::syntax(y, x, ","))
                }
                acc = (new_s1, new_s1);
                expect_comma = false;
                comma_found = true;
                if s0 == s1 {
                    continue;
                }
            } else if ch == '{' {
                if expect_comma || !command_ctx.has_command() || !par.is_empty() {
                    return Err(CompileError::syntax(y, x, ch))
                }
                acc = (new_s1, new_s1);
                in_block = 1;
                command_ctx.line_no_par = self.line_no;
                command_ctx.char_no_par = self.char_no;
                continue;
            } else if ch == '}' {
                return Err(CompileError::syntax(y, x, ch))
            } else if ch.is_ascii_alphanumeric() || (ch == '-') || (ch == '_') {
                acc = (s0, new_s1);
                if s0 == s1 { //start of new token
                    was_comma = comma_found;
                    comma_found = false;
                    expect_comma = true
                }
                continue;
            } else { // TODO: (message for the owner: please write descriptive explanation)
                return Err(CompileError::syntax(y, x, "Bad char"))
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
                        return Err(CompileError::unknown(y, x, &token))
                    }
                }
                Some(&new_rule) => {
                    match command_ctx.compile(&mut ret, &mut par, self) {
                        Ok(_) => {
                            command_ctx = CommandContext::new(token, x, y, Some(new_rule));
                            expect_comma = false;
                            was_comma = false;
                            was_newline = newline_found;
                        }
                        Err(e @ CompileError::Operation(_, _, OperationError::MissingRequiredParameters)) => {
                            if was_newline { // it seems realy new command - rturn correct missing params error
                                return Err(e)
                            } else {
                                par.push((y, x, &source[s0..s1], was_comma));
                                was_comma = false;
                            }
                        }
                        Err(e) => return Err(e)
                    }
                }
            }
        }
        // Compile last pending command if any
        command_ctx.compile(&mut ret, &mut par, self)?;
        Ok(ret)
    }

}

pub fn compile_code(code: &str) -> Result<SliceData, CompileError> {
    compile_code_to_cell(code).map(|code| code.into())
}

pub fn compile_code_to_cell(code: &str) -> Result<Cell, CompileError> {
    log::trace!(target: "tvm", "begin compile\n");
    Engine::<CodePage0>::new().compile(code).map(|code| code.finalize().into())
}


