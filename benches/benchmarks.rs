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
    let bytes = read_boc("benches/elector-data.boc");
    c.bench_function("load-boc", |b| b.iter( || {
        ton_types::read_single_root_boc(bytes.clone()).unwrap()
    }));
}

criterion_group!(
    benches,
    criterion_bench_num_bigint,
//    criterion_bench_rug_bigint,
    criterion_bench_load_boc,
    criterion_bench_elector_algo_1000_vtors,
    criterion_bench_tiny_loop_200000_iters
);
criterion_main!(benches);
