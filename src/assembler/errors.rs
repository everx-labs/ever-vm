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

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

pub type OperationName = String;
pub type ParameterName = String;
pub type Explanation = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterError {
    UnexpectedType,
    NotSupported,
    OutOfRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationError {
    Parameter(ParameterName, ParameterError),
    TooManyParameters,
    LogicErrorInParameters(&'static str),
    MissingRequiredParameters,
    MissingBlock,
    Nested(Box<CompileError>),
    NotFitInSlice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    Syntax(Position, Explanation),
    UnknownOperation(Position, OperationName),
    Operation(Position, OperationName, OperationError),
}

impl CompileError {
    pub fn syntax<S: ToString>(line: usize, column: usize, explanation: S) -> Self {
        CompileError::Syntax(Position{line, column}, explanation.to_string())
    }
    pub fn unknown<S: ToString>(line: usize, column: usize, name: S) -> Self {
        CompileError::UnknownOperation(Position{line, column}, name.to_string())
    }
    pub fn operation<S: ToString>(line: usize, column: usize, name: S, error: OperationError) -> Self {
        CompileError::Operation(Position{line, column}, name.to_string(), error)
    }
    pub fn missing_params<S: ToString>(line: usize, column: usize, name: S) -> Self {
        CompileError::Operation(Position{line, column}, name.to_string(), OperationError::MissingRequiredParameters)
    }
    pub fn missing_block<S: ToString>(line: usize, column: usize, name: S) -> Self {
        CompileError::Operation(Position{line, column}, name.to_string(), OperationError::MissingBlock)
    }
    pub fn too_many_params<S: ToString>(line: usize, column: usize, name: S) -> Self {
        CompileError::Operation(Position{line, column}, name.to_string(), OperationError::TooManyParameters)
    }
    pub fn out_of_range<S1: ToString, S2: ToString>(line: usize, column: usize, name: S1, param: S2) -> Self {
        let operation = OperationError::Parameter(param.to_string(), ParameterError::OutOfRange);
        CompileError::Operation(Position{line, column}, name.to_string(), operation)
    }
}

pub trait ToOperationParameterError<T>
where
    T: Into<ParameterName>,
{
    type Output;
    fn parameter(self, name: T) -> Self::Output;
}

impl<T, S> ToOperationParameterError<S> for Result<T, ParameterError>
where
    S: Into<ParameterName>,
{
    type Output = Result<T, OperationError>;

    fn parameter(self, name: S) -> Result<T, OperationError> {
        self.map_err(|e| e.parameter(name))
    }
}

impl<S> ToOperationParameterError<S> for ParameterError
where
    S: Into<ParameterName>,
{
    type Output = OperationError;
    fn parameter(self, name: S) -> OperationError {
        OperationError::Parameter(name.into(), self)
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{}:{}]", self.line, self.column)
    }
}

impl fmt::Display for ParameterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParameterError::UnexpectedType => write!(f, "Unexpected parameter type."),
            ParameterError::NotSupported => write!(
                f,
                "Parameter value is correct, however it's not supported yet."
            ),
            ParameterError::OutOfRange => write!(f, "Parameter value is out of range"),
        }
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OperationError::Parameter(name, error) => write!(
                f,
                "Operation parameter {} has the following problem: {}",
                name, error
            ),
            OperationError::TooManyParameters => write!(f, "Operation has too many parameters."),
            OperationError::LogicErrorInParameters(ref error) => write!(f,
                "Logic error {}", error
            ),
            OperationError::MissingRequiredParameters => {
                write!(f, "Operation requires more parameters.")
            }
            OperationError::MissingBlock => {
                write!(f, "Operation requires block in {{}} braces.")
            }
            OperationError::Nested(error) => write!(f, "Operation error. Internal: {}", error),
            OperationError::NotFitInSlice => write!(f, "Command bytecode is too long for single slice"),
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompileError::Syntax(position, explanation) => {
                write!(f, "{} Syntax error: {}", position, explanation)
            }
            CompileError::UnknownOperation(position, name) => write!(f, "{} Unknown operation {}", position, name),
            CompileError::Operation(position, name, error) => {
                write!(f, "{} Operation {} error: {}", position, name, error)
            }
        }
    }
}
