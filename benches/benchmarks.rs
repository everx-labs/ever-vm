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
use ton_vm::{executor::Engine, stack::{savelist::SaveList, Stack, StackItem, integer::IntegerData}, int};
use std::{sync::Arc, time::Duration};

fn load_boc(filename: &str) -> ton_types::Cell {
    let mut bytes = Vec::new();
    let mut file = std::fs::File::open(filename).unwrap();
    std::io::Read::read_to_end(&mut file, &mut bytes).unwrap();
    let mut cur = std::io::Cursor::new(bytes.clone());
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
        int!(0x76ef1ea),
        int!(0),
        int!(0),
        int!(1633458077),
        int!(0),
        int!(0),
        int!(0),
        StackItem::tuple(vec!(
            int!(1000000000),
            StackItem::None
        )),
        StackItem::slice(SliceData::from_string("9fe0000000000000000000000000000000000000000000000000000000000000001_").unwrap()),
        StackItem::cell(config_data.reference(0).unwrap()),
        StackItem::None,
        int!(0),
    );
    ctrls.put(7, &mut StackItem::tuple(vec!(StackItem::tuple(params)))).unwrap();

    let mut stack = Stack::new();
    stack.push(int!(1000000000));
    stack.push(int!(0));
    stack.push(int!(0));
    stack.push(int!(-2));

    let mut group = c.benchmark_group("flat-sampling");
    group.measurement_time(Duration::from_secs(10));
    group.noise_threshold(0.03);
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);
    group.bench_function("elector-algo-1000-vtors", |b| b.iter(|| {
        let mut engine = Engine::new().setup_with_libraries(
            SliceData::from(elector_code.clone()),
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
        int!(0x76ef1ea),
        int!(0),
        int!(0),
        int!(0),
        int!(0),
        int!(0),
        int!(0),
        StackItem::tuple(vec!(
            int!(1000000000),
            StackItem::None
        )),
        StackItem::default(),
        StackItem::None,
        StackItem::None,
        int!(0),
    );
    ctrls.put(7, &mut StackItem::tuple(vec!(StackItem::tuple(params)))).unwrap();

    let mut stack = Stack::new();
    stack.push(int!(1000000000));
    stack.push(int!(0));
    stack.push(int!(0));
    stack.push(int!(-2));

    let mut group = c.benchmark_group("flat-sampling");
    group.measurement_time(Duration::from_secs(10));
    group.noise_threshold(0.03);
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);
    group.bench_function("tiny-loop-200000-iters", |b| b.iter(|| {
        let mut engine = Engine::new().setup_with_libraries(
            SliceData::from(tiny_code.clone()),
            Some(ctrls.clone()),
            Some(stack.clone()),
            None,
            vec!());
        engine.execute().unwrap();
        assert_eq!(engine.gas_used(), 34000891);
        // result of computation gets verified within the test itself
    }));
}

criterion_group!(benches,
    criterion_bench_elector_algo_1000_vtors,
    criterion_bench_tiny_loop_200000_iters,
);
criterion_main!(benches);
