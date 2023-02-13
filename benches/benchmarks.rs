/*
 * Copyright (C) 2021 TON Labs. All Rights Reserved.
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

use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use ton_types::SliceData;
use ton_vm::{executor::Engine, stack::{savelist::SaveList, Stack, StackItem}};
use std::{sync::Arc, time::Duration};

static DEFAULT_CAPABILITIES: u64 = 0x572e;

fn read_boc(filename: &str) -> std::io::Cursor<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut file = std::fs::File::open(filename).unwrap();
    std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
    std::io::Cursor::new(bytes)
}

fn load_boc(filename: &str) -> ton_types::Cell {
    let mut cur = read_boc(filename);
    ton_types::deserialize_tree_of_cells(&mut cur).unwrap()
}

fn criterion_bench_elector_algo_1000_vtors(c: &mut Criterion) {
    let elector_code = load_boc("benches/elector-code.boc");
    let elector_data = load_boc("benches/elector-data.boc");
    let config_data = load_boc("benches/config-data.boc");

    let elector_data_output = load_boc("benches/elector-data-output.boc");
    let elector_actions = load_boc("benches/elector-actions.boc");

    let mut ctrls = SaveList::default();
    ctrls.put(4, &mut StackItem::Cell(elector_data)).unwrap();
    let params = vec!(
        StackItem::int(0x76ef1ea),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(1633458077),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::tuple(vec!(
            StackItem::int(1000000000),
            StackItem::None
        )),
        StackItem::slice(SliceData::from_string("9fe0000000000000000000000000000000000000000000000000000000000000001_").unwrap()),
        StackItem::cell(config_data.reference(0).unwrap()),
        StackItem::None,
        StackItem::int(0),
    );
    ctrls.put(7, &mut StackItem::tuple(vec!(StackItem::tuple(params)))).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(1000000000));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(-2));

    let mut group = c.benchmark_group("flat-sampling");
    group.measurement_time(Duration::from_secs(10));
    group.noise_threshold(0.03);
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);
    group.bench_function("elector-algo-1000-vtors", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            SliceData::load_cell_ref(&elector_code).unwrap(),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 82386791);
        let output = engine.ctrl(4).unwrap().as_cell().unwrap();
        assert_eq!(output, &elector_data_output);
        let actions = engine.ctrl(5).unwrap().as_cell().unwrap();
        assert_eq!(actions, &elector_actions);
    }));
    group.finish();
}

use ton_vm::executor::IndexProvider;
#[path = "../src/tests/common.rs"]
mod common;

fn criterion_bench_try_elect_new_1000_vtors(c: &mut Criterion) {
    // common::logger_init();
    let elector_code = common::NEW_ELECTOR_CODE.clone();
    let elector_data = load_boc("benches/elector-data.boc");
    let config_data = load_boc("benches/config-data-try-elect.boc");

    let index_provider = Arc::new(common::FakeIndexProvider::new(elector_data.clone(), true).unwrap());

    let elector_data_output = load_boc("benches/try-elect-data-output.boc");
    let elector_actions = load_boc("benches/try-elect-actions.boc");

    let mut ctrls = SaveList::default();
    ctrls.put(4, &mut StackItem::Cell(elector_data)).unwrap();
    let params = vec!(
        StackItem::int(0x76ef1ea),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(1633458077),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::tuple(vec!(
            StackItem::int(1000000000),
            StackItem::None
        )),
        StackItem::slice(SliceData::from_string("9fe6666666666666666666666666666666666666666666666666666666666666667_").unwrap()),
        StackItem::cell(config_data.reference(0).unwrap()),
        StackItem::None,
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(1000),
    );
    ctrls.put(7, &mut StackItem::tuple(vec!(StackItem::tuple(params)))).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(1000000000));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(-2));

    let mut group = c.benchmark_group("flat-sampling");
    group.measurement_time(Duration::from_secs(10));
    group.noise_threshold(0.03);
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);
    group.bench_function("try-elect-algo-1000-vtors", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            SliceData::load_cell_ref(&elector_code).unwrap(),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.set_index_provider(index_provider.clone());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 17015317);

        let output = engine.ctrl(4).unwrap().as_cell().unwrap();
        assert_eq!(output, &elector_data_output);
        let actions = engine.ctrl(5).unwrap().as_cell().unwrap();
        assert_eq!(actions, &elector_actions);
    }));
    group.finish();
}

fn criterion_bench_tiny_loop_200000_iters(c: &mut Criterion) {
    let tiny_code = load_boc("benches/tiny-code.boc");
    let tiny_data = load_boc("benches/tiny-data.boc");

    let mut ctrls = SaveList::default();
    ctrls.put(4, &mut StackItem::Cell(tiny_data)).unwrap();
    let params = vec!(
        StackItem::int(0x76ef1ea),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::tuple(vec!(
            StackItem::int(1000000000),
            StackItem::None
        )),
        StackItem::default(),
        StackItem::None,
        StackItem::None,
        StackItem::int(0),
    );
    ctrls.put(7, &mut StackItem::tuple(vec!(StackItem::tuple(params)))).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(1000000000));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(-2));

    let mut group = c.benchmark_group("flat-sampling");
    group.measurement_time(Duration::from_secs(10));
    group.noise_threshold(0.03);
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);
    group.bench_function("tiny-loop-200000-iters", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            SliceData::load_cell_ref(&tiny_code).unwrap(),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 34000891);
        // result of computation gets verified within the test itself
    }));
}

fn criterion_bench_num_bigint(c: &mut Criterion) {
    c.bench_function("num-bigint", |b| b.iter( || {
        let n = num::BigInt::from(1000000);
        let mut accum = num::BigInt::from(0);
        let mut iter = num::BigInt::from(0);
        loop {
            if !(iter < n) {
                break;
            }
            accum += num::BigInt::from(iter.bits());
            iter += 1;
        }
        assert_eq!(num::BigInt::from(18951425), accum);
    }));
}

// Note: the gmp-mpfr-based rug crate shows almost the same perf as num-bigint

// fn criterion_bench_rug_bigint(c: &mut Criterion) {
//     c.bench_function("rug-bigint", |b| b.iter( || {
//         let n = rug::Integer::from(1000000);
//         let mut accum = rug::Integer::from(0);
//         let mut iter = rug::Integer::from(0);
//         loop {
//             if !(iter < n) {
//                 break;
//             }
//             accum += rug::Integer::from(iter.significant_bits());
//             iter += 1;
//         }
//         assert_eq!(rug::Integer::from(18951425), accum);
//     }));
// }

fn criterion_bench_load_boc(c: &mut Criterion) {
    let cur = read_boc("benches/elector-data.boc");
    c.bench_function("load-boc", |b| b.iter( || {
        ton_types::deserialize_tree_of_cells(&mut cur.clone()).unwrap()
    }));
}

fn criterion_bench_deep_stack_switch(c: &mut Criterion) {
    let code = ton_labs_assembler::compile_code("
        NULL
        PUSHINT 10000
        PUSHCONT {
            BLKPUSH 15, 0
        }
        REPEAT
        ZERO
        ONLYX
    ").unwrap().into_cell();

    let mut ctrls = SaveList::default();
    let params = vec!(
        StackItem::int(0x76ef1ea),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::tuple(vec!(
            StackItem::int(1000000000),
            StackItem::None
        )),
        StackItem::default(),
        StackItem::None,
        StackItem::None,
        StackItem::int(0),
    );
    ctrls.put(7, &mut StackItem::tuple(vec!(StackItem::tuple(params)))).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(1000000000));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(0));
    stack.push(StackItem::int(-2));

    c.bench_function("deep-cell-switch", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            SliceData::load_cell_ref(&code).unwrap(),
            Some(ctrls.clone()),
            None,
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 310129);
    }));
}

criterion_group!(benches,
    criterion_bench_num_bigint,
//    criterion_bench_rug_bigint,
    criterion_bench_load_boc,
    criterion_bench_elector_algo_1000_vtors,
    criterion_bench_try_elect_new_1000_vtors,
    criterion_bench_tiny_loop_200000_iters,
    criterion_bench_deep_stack_switch,
);
criterion_main!(benches);
