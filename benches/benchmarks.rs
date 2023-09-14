/*
 * Copyright (C) 2021-2023 TON Labs. All Rights Reserved.
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
use pprof::criterion::{PProfProfiler, Output};
use ton_block::{StateInit, Deserializable};
use ton_types::SliceData;
use ton_vm::{
    executor::Engine,
    stack::{savelist::SaveList, Stack, StackItem, continuation::ContinuationData}
};
use std::time::Duration;

static DEFAULT_CAPABILITIES: u64 = 0x572e;

fn read_boc(filename: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut file = std::fs::File::open(filename).unwrap();
    std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
    bytes
}

fn load_boc(filename: &str) -> ton_types::Cell {
    let bytes = read_boc(filename);
    ton_types::read_single_root_boc(bytes).unwrap()
}

fn load_stateinit(filename: &str) -> StateInit {
    StateInit::construct_from_file(filename).unwrap()
}

fn bench_elector_algo_1000_vtors(c: &mut Criterion) {
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

fn bench_tiny_loop_200000_iters(c: &mut Criterion) {
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

fn bench_num_bigint(c: &mut Criterion) {
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

// fn bench_rug_bigint(c: &mut Criterion) {
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

fn bench_load_boc(c: &mut Criterion) {
    let bytes = read_boc("benches/elector-data.boc");
    c.bench_function("load-boc", |b| b.iter( || {
        ton_types::read_single_root_boc(bytes.clone()).unwrap()
    }));
}

const MAX_TUPLE_SIZE: usize = 255;

// array = [row1, row2, ...], array.len() <= MAX_TUPLE_SIZE
// row_i = [v1, v2, ...], row_i.len() <= row_size

fn make_array(input: &[i64], row_size: usize) -> StackItem {
    assert!(0 < row_size && row_size <= MAX_TUPLE_SIZE);
    assert!(input.len() <= row_size * MAX_TUPLE_SIZE);
    let mut row = Vec::new();
    let mut rows = Vec::new();
    for i in 0..input.len() {
        row.push(StackItem::int(input[i]));
        if (i + 1) % row_size == 0 {
            assert_eq!(row.len(), row_size);
            rows.push(StackItem::tuple(row));
            row = Vec::new();
        }
    }
    if row.len() > 0 {
        rows.push(StackItem::tuple(row));
    }
    StackItem::tuple(rows)
}

fn bench_mergesort_tuple(c: &mut Criterion) {
    let code = load_boc("benches/mergesort/mergesort.boc");
    let code_slice = SliceData::load_cell_ref(&code).unwrap();

    const ROW_SIZE: usize = 32; // size() function in the code
    const COUNT: usize = 1000; // total elements count

    let mut input = Vec::with_capacity(COUNT);
    for i in 0..COUNT {
        input.push((COUNT - i - 1) as i64);
    }
    let array = make_array(&input, ROW_SIZE);
    input.sort();
    let expected = make_array(&input, ROW_SIZE);

    // runvmx mode: +1 = same_c3
    let mut ctrls = SaveList::default();
    ctrls.put(3, &mut StackItem::continuation(
        ContinuationData::with_code(code_slice.clone())
    )).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(-1));
    stack.push(array);
    // runvmx mode: +2 = push_0
    stack.push(StackItem::int(0));

    c.bench_function("mergesort-tuple", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            code_slice.clone(),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 51_216_096);
        assert_eq!(engine.stack().depth(), 1);
        assert_eq!(engine.stack().get(0), &expected);
    }));
}

fn bench_massive_cell_upload(c: &mut Criterion) {
    let stateinit = load_stateinit("benches/massive/cell-upload.tvc");
    let mut ctrls = SaveList::default();
    ctrls.put(4, &mut StackItem::cell(stateinit.data().unwrap().clone())).unwrap();
    let params = vec!(
        StackItem::int(0x76ef1ea),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(1678299227),
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

    let msg = load_boc("benches/massive/cell-upload-msg.boc");
    let mut body = SliceData::load_cell_ref(&msg).unwrap();
    body.move_by(366).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(1000000000));
    stack.push(StackItem::int(0));
    stack.push(StackItem::cell(msg));
    stack.push(StackItem::slice(body));
    stack.push(StackItem::int(-1));

    c.bench_function("massive-cell-upload", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            SliceData::load_cell_ref(&stateinit.code().unwrap()).unwrap(),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 5479);
    }));
}

fn bench_massive_cell_finalize(c: &mut Criterion) {
    let stateinit = load_stateinit("benches/massive/cell-finalize.tvc");
    let mut ctrls = SaveList::default();
    ctrls.put(4, &mut StackItem::cell(stateinit.data().unwrap().clone())).unwrap();
    let params = vec!(
        StackItem::int(0x76ef1ea),
        StackItem::int(0),
        StackItem::int(0),
        StackItem::int(1678296619),
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

    let msg = load_boc("benches/massive/cell-finalize-msg.boc");
    let mut body = SliceData::load_cell_ref(&msg).unwrap();
    body.move_by(366).unwrap();

    let mut stack = Stack::new();
    stack.push(StackItem::int(1000000000));
    stack.push(StackItem::int(0));
    stack.push(StackItem::cell(msg));
    stack.push(StackItem::slice(body));
    stack.push(StackItem::int(-1));

    c.bench_function("massive-cell-finalize", |b| b.iter(|| {
        let mut engine = Engine::with_capabilities(DEFAULT_CAPABILITIES).setup_with_libraries(
            SliceData::load_cell_ref(&stateinit.code().unwrap()).unwrap(),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 203585);
    }));
}

criterion_group!(
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets =
        bench_num_bigint,
        // bench_rug_bigint,
        bench_load_boc,
        bench_elector_algo_1000_vtors,
        bench_tiny_loop_200000_iters,
        bench_mergesort_tuple,
        bench_massive_cell_upload,
        bench_massive_cell_finalize,
);
criterion_main!(benches);
