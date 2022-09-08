/*
* Copyright (C) 2019-2022 TON Labs. All Rights Reserved.
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
        engine::{storage::fetch_stack, Engine}, gas::gas_state::Gas, types::Instruction
    },
    stack::StackItem, types::{Exception, Status},
    utils::{
        bytes_to_string, pack_data_to_cell, unpack_data_from_cell
    }
};
use std::io::{Cursor, Read};
use std::time::{Duration, Instant};
use ton_block::GlobalCapabilities;
use ton_types::{error, ExceptionCode, Result, SliceData};
use crate::error::tvm_exception_code;
use crate::executor::Mask;

const ZIP:          u8 = 0x01; // unzip before process and zip after process
const BINARY:       u8 = 0x02; // use binary version functions instead of utf8
const IGNORE_ERROR: u8 = 0x04; // ignore errors

const DIFF_TIMEOUT: Duration = Duration::from_millis(300);

fn ignore_error(engine: &mut Engine, result: Status) -> Status {
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            let exception_code = tvm_exception_code(&err);
            if exception_code == Some(ExceptionCode::OutOfGas) {
                return err!(ExceptionCode::OutOfGas)
            }
            engine.cc.stack.push(StackItem::None);
            Ok(())
        }
    }
}

fn get_two_slices_from_stack(engine: &mut Engine, name: &'static str) -> Result<(SliceData, SliceData)> {
    engine.load_instruction(Instruction::new(name))?;
    fetch_stack(engine, 2)?;
    let s0 = engine.cmd.var(1).as_cell()?.clone().into();
    let s1 = engine.cmd.var(0).as_cell()?.clone().into();
    Ok((s0, s1))
}

fn process_input_slice(s: SliceData, engine: &mut Engine, how: u8) -> Result<Vec<u8>> {
    let s = unpack_data_from_cell(s, engine)?;
    let s = if how.bit(ZIP) {
        unzip(engine, &s)?
    } else {
        s
    };
    Ok(s)
}

fn process_output_slice(s: &[u8], engine: &mut Engine, how: u8) -> Status {
    let cell = if how.bit(ZIP) {
        pack_data_to_cell(&zip(engine, s)?, engine)?
    } else {
        pack_data_to_cell(s, engine)?
    };
    engine.cc.stack.push(StackItem::cell(cell));
    Ok(())
}

// Keep for experiments 
fn _diff_diffy_lib(engine: &mut Engine, fst: &str, snd: &str) -> Result<String> {
    engine.try_use_gas(Gas::diff_fee_for_line(
        fst.lines().count(),
        snd.lines().count(),
    ))?;

    let mut options = diffy::DiffOptions::default();
    options.set_context_len(0);
    let patch = options.create_patch(fst, snd);
    let result = patch.to_string();
    let result = result
        .strip_prefix("--- original\n+++ modified\n")
        .unwrap_or(&result);

    engine.try_use_gas(Gas::diff_fee_for_count_patches(patch.hunks().len()))?;

    Ok(result.to_string())
}

fn diff_similar_lib(engine: &mut Engine, fst: &str, snd: &str) -> Result<String> {
    engine.try_use_gas(Gas::diff_fee_for_line(
        fst.lines().count(),
        snd.lines().count(),
    ))?;

    let mut config = similar::TextDiffConfig::default();
    let current_time = Instant::now();
    config
        .algorithm(similar::Algorithm::Myers)
        .deadline(current_time + DIFF_TIMEOUT);
    let diff = config.diff_lines(fst, snd);
    if current_time.elapsed() >= DIFF_TIMEOUT - Duration::from_millis(1) {
        return err!(ExceptionCode::OutOfGas);
    }
    let mut output = diff.unified_diff();
    let result = output.context_radius(0).to_string();

    engine.try_use_gas(Gas::diff_fee_for_count_patches(diff.grouped_ops(0).len()))?;

    Ok(result)
}

fn patch_diffy_lib(engine: &mut Engine, str: &str, patch: &str) -> Result<String> {
    let str_lines = str.lines().count() as i64;
    engine.try_use_gas(Gas::diff_patch_fee_for_line(str_lines))?;

    let patch = match diffy::Patch::from_str(patch) {
        Ok(patch) => patch,
        Err(err) => {
            return Err(exception!(
                ExceptionCode::TypeCheckError,
                "Incorrect diff patch: {}",
                err
            ));
        }
    };

    let count_patches = patch.hunks().len() as i64;
    engine.try_use_gas(Gas::diff_patch_fee_for_count_patches(count_patches))?;

    let result = match diffy::apply(str, &patch) {
        Ok(result) => result,
        Err(err) => {
            return Err(exception!(
                ExceptionCode::TypeCheckError,
                "Cannot apply patch to file with error: {}",
                err
            ));
        }
    };

    Ok(result)
}

fn patch_binary_diffy_lib(engine: &mut Engine, str: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    engine.try_use_gas(Gas::diff_bytes_patch_fee_for_byte(str.len() as i64))?;

    let patch = match diffy::Patch::from_bytes(patch) {
        Ok(patch) => patch,
        Err(err) => {
            return Err(exception!(
                ExceptionCode::TypeCheckError,
                "Incorrect diff binary patch: {}",
                err
            ));
        }
    };

    let count_patches = patch.hunks().len() as i64;
    engine.try_use_gas(Gas::diff_patch_fee_for_count_patches(count_patches))?;

    let result = match diffy::apply_bytes(str, &patch) {
        Ok(result) => result,
        Err(err) => {
            return Err(exception!(
                ExceptionCode::TypeCheckError,
                "Cannot apply patch to file with error: {}",
                err
            ));
        }
    };

    Ok(result)
}

fn zip(engine: &mut Engine, data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(vec![]);
    }
    engine.try_use_gas(Gas::zip_fee_for_byte(data.len() as i64))?;

    let mut compressed = Vec::new();
    zstd::stream::copy_encode(&mut Cursor::new(data), &mut compressed, 3)
        .map_err(|err| exception!(ExceptionCode::UnknownError, "Cannot compress data: {}", err))?;

    engine.try_use_gas(Gas::zip_fee_for_byte(compressed.len() as i64))?;
    Ok(compressed)
}

fn unzip(engine: &mut Engine, data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(vec![]);
    }
    engine.try_use_gas(Gas::unzip_fee_for_byte(data.len() as i64))?;

    let mut cursor = Cursor::new(data);
    let mut decoder = zstd::stream::Decoder::new(&mut cursor).map_err(|err| {
        exception!(
            ExceptionCode::UnknownError,
            "Cannot uncompress data: {}",
            err
        )
    })?;

    let mut buffer = vec![0; 10000]; // in heap in order to don't exceed stack
    let mut result = Vec::new();
    loop {
        let res_len = decoder.read(&mut buffer).map_err(|err| {
            exception!(
                ExceptionCode::UnknownError,
                "Cannot uncompress data: {}",
                err
            )
        })?;
        if res_len == 0 {
            break;
        }
        engine.try_use_gas(Gas::unzip_fee_for_byte(res_len as i64))?;

        result.extend_from_slice(&buffer[0..res_len]);
    }

    Ok(result)
}

fn execute_diff_with_options(engine: &mut Engine, s0: SliceData, s1: SliceData, how: u8) -> Status {
    let fst = process_input_slice(s0, engine, how)?;
    let fst = bytes_to_string(fst)?;
    let snd = process_input_slice(s1, engine, how)?;
    let snd = bytes_to_string(snd)?;

    let result = diff_similar_lib(engine, &fst, &snd)?;

    process_output_slice(result.as_bytes(), engine, how)
}

fn execute_patch_with_options(name: &'static str, engine: &mut Engine, how: u8) -> Status {
    engine.check_capability(GlobalCapabilities::CapDiff)?;

    let (s0, s1) = get_two_slices_from_stack(engine, name)?;

    let result = if how.bit(BINARY) {
        (|| {
            let str = process_input_slice(s0, engine, how)?;
            let p = process_input_slice(s1, engine, how)?;
            let result = patch_binary_diffy_lib(engine, &str, &p)?;
            process_output_slice(&result, engine, how)
        })()
    } else {
        (|| {
            let str = process_input_slice(s0, engine, how)?;
            let p = process_input_slice(s1, engine, how)?;
            let str = bytes_to_string(str)?;
            let p = bytes_to_string(p)?;
            let result = patch_diffy_lib(engine, &str, &p)?;
            process_output_slice(result.as_bytes(), engine, how)
        })()
    };

    if how.bit(IGNORE_ERROR) {
        ignore_error(engine, result)
    } else {
        result
    }
}

/// ZIP (s – c), zip string
pub(super) fn execute_zip(engine: &mut Engine) -> Status {
    if !engine.check_capabilities(GlobalCapabilities::CapDiff as u64) {
        return Status::Err(ExceptionCode::InvalidOpcode.into());
    }

    engine.load_instruction(Instruction::new("ZIP"))?;
    fetch_stack(engine, 1)?;
    let s = engine.cmd.var(0).as_cell()?.clone().into();
    let data = unpack_data_from_cell(s, engine)?;

    let compressed = zip(engine, &data)?;

    let cell = pack_data_to_cell(&compressed, engine)?;
    engine.cc.stack.push(StackItem::cell(cell));
    Ok(())
}

/// UNZIP (s – c), zip string
pub(super) fn execute_unzip(engine: &mut Engine) -> Status {
    if !engine.check_capabilities(GlobalCapabilities::CapDiff as u64) {
        return Status::Err(ExceptionCode::InvalidOpcode.into());
    }

    engine.load_instruction(Instruction::new("UNZIP"))?;
    fetch_stack(engine, 1)?;
    let s = engine.cmd.var(0).as_cell()?.clone().into();
    let data = unpack_data_from_cell(s, engine)?;

    let decompressed = unzip(engine, &data)?;

    let cell = pack_data_to_cell(&decompressed, engine)?;
    engine.cc.stack.push(StackItem::cell(cell));
    Ok(())
}

/// DIFF (s s – c), gen diff of two messages.
pub(super) fn execute_diff(engine: &mut Engine) -> Status {
    engine.check_capability(GlobalCapabilities::CapDiff)?;
    let (s0, s1) = get_two_slices_from_stack(engine, "DIFF")?;
    execute_diff_with_options(engine, s0, s1, 0)
}

/// DIFF_ZIP (s s – c), unpack messages, gen diff and pack result.
pub(super) fn execute_diff_zip(engine: &mut Engine) -> Status {
    engine.check_capability(GlobalCapabilities::CapDiff)?;
    let (s0, s1) = get_two_slices_from_stack(engine, "DIFF_ZIP")?;
    execute_diff_with_options(engine, s0, s1, ZIP)
}

/// DIFF_PATCHQ (s s – c), patch message
pub(super) fn execute_diff_patch_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCHQ", engine, IGNORE_ERROR)
}

/// DIFF_PATCH (s s – c), patch message
pub(super) fn execute_diff_patch_not_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH", engine, 0)
}

/// DIFF_PATCH_ZIPQ (s s – c), unpack message and diff, patch message and pack result
pub(super) fn execute_diff_patch_zip_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH_ZIPQ", engine, IGNORE_ERROR + ZIP)
}

/// DIFF_PATCH_ZIP (s s – c), unpack message and diff, patch message and pack result
pub(super) fn execute_diff_patch_zip_not_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH_ZIP", engine, ZIP)
}

/// DIFF_PATCH_BINARYQ (s s – c), patch message
pub(super) fn execute_diff_patch_binary_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH_BINARYQ", engine, IGNORE_ERROR + BINARY)
}

/// DIFF_PATCH_BINARY (s s – c), patch message
pub(super) fn execute_diff_patch_binary_not_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH_BINARY", engine, BINARY)
}

/// DIFF_PATCH_BINARY_ZIPQ (s s – c), unpack message and diff, patch message and pack result
pub(super) fn execute_diff_patch_binary_zip_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH_BINARY_ZIPQ", engine, IGNORE_ERROR + BINARY + ZIP)
}

/// DIFF_PATCH_BINARY_ZIP (s s – c), unpack message and diff, patch message and pack result
pub(super) fn execute_diff_patch_binary_zip_not_quiet(engine: &mut Engine) -> Status {
    execute_patch_with_options("DIFF_PATCH_BINARY_ZIP", engine, ZIP + BINARY)
}
