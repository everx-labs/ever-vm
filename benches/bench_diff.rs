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

use criterion::{criterion_group, criterion_main, Criterion};
use std::{fs, time::Duration};
use std::fs::{File, read_to_string};
use std::io::{Cursor, Read};
use std::time::Instant;

fn _old_diff(f1: &String, f2: &String) -> (String, usize) {
    let mut options = diffy::DiffOptions::default();
    options.set_context_len(0);
    let patch = options.create_patch(&*f1, &*f2);
    let size = patch.hunks().len();
    let text = format!("{}", patch).to_string();

    (text, size)
}

fn new_diff(f1: &String, f2: &String) -> (String, usize) {
    let mut config = similar::TextDiffConfig::default();
    let current_time = Instant::now();
    let deadline = Duration::from_millis(300);
    config.algorithm(similar::Algorithm::Myers).deadline(current_time + deadline);
    let diff = config.diff_lines(f1, f2);
    let mut output = diff.unified_diff();
    let result = format!("{}", output.context_radius(0));

    (result, diff.grouped_ops(0).len())
}

fn bench_text_diff(name: &str, c: &mut Criterion, file1: &str, file2: &str) {
    let f1 = read_to_string("./benches/diff/".to_string() + file1).unwrap();
    let f2 = read_to_string("./benches/diff/".to_string() + file2).unwrap();

    let mut size = 0;
    let mut text = "".to_string();
    c.bench_function(&*(name.to_string() + "_diff"), |b| b.iter(|| {
        (text, size) = new_diff(&f1, &f2);
    }));
    println!("File lines: {}, {}", f1.lines().count(), f2.lines().count());
    println!("Diff operations: {}; diff text length: {}", size, text.len());

    c.bench_function(&*(name.to_string() + "_init_patch"), |b| b.iter(|| {
        let _patch = diffy::Patch::from_str(&*text).unwrap();
        assert!(_patch.hunks().len() != 0);
    }));
    let patch = diffy::Patch::from_str(&*text).unwrap();

    let mut f_res = "".to_string();
    c.bench_function(&*(name.to_string() + "_patch"), |b| b.iter(|| {
        f_res = diffy::apply(&*f1, &patch).unwrap();
    }));
    assert_eq!(f2, f_res);
}

fn criterion_diff(c: &mut Criterion) {
    bench_text_diff("test_1", c, "f11.txt", "f12.txt");
    bench_text_diff("test_2", c, "f21.txt", "f22.txt");
    bench_text_diff("test_3", c, "f31.txt", "f32.txt");
    bench_text_diff("test_4", c, "f41.txt", "f42.txt");
}

fn bench_zip(name: &str, c: &mut Criterion, file: &str) {
    let f1 = read_to_string("./benches/diff/".to_string() + file).unwrap();

    let mut compressed = Vec::new();
    c.bench_function(&*(name.to_string() + "zip"), |b| b.iter(|| {
        compressed = Vec::new();
        zstd::stream::copy_encode(
            &mut Cursor::new(f1.clone()),
            &mut compressed,
            3
        ).unwrap();
    }));
    println!(
        "Bytes: old file: {}, compressed: {}, sum: {}",
         f1.as_bytes().len(), compressed.len(), f1.as_bytes().len() + compressed.len()
    );

    let mut decompressed = Vec::new();
    c.bench_function(&*(name.to_string() + "unzip"), |b| b.iter(|| {
        decompressed = Vec::new();
        zstd::stream::copy_decode(&mut Cursor::new(compressed.clone()), &mut decompressed).unwrap();
    }));
    let result = String::from_utf8(decompressed).unwrap();
    assert_eq!(f1, result);
}

fn criterion_zip(c: &mut Criterion) {
    bench_zip("test_1", c, "f11.txt");
    bench_zip("test_1", c, "f21.txt");
    bench_zip("test_1", c, "f31.txt");
    bench_zip("test_1", c, "f41.txt");
}

fn get_file_as_byte_vec(filename: String) -> Vec<u8> {
    let mut f = File::open(&filename).expect("no file found");
    let metadata = fs::metadata(&filename).expect("unable to read metadata");
    let mut buffer = vec![0; metadata.len() as usize];
    f.read(&mut buffer).expect("buffer overflow");

    buffer
}

fn bench_diff_binary(name: &str, c: &mut Criterion, file1: &str, file2: &str) {
    let fst = get_file_as_byte_vec("./benches/diff/".to_string() + file1);
    let snd = get_file_as_byte_vec("./benches/diff/".to_string() + file2);

    let mut options = diffy::DiffOptions::default();
    options.set_context_len(0);
    let patch = options.create_patch_bytes(&fst, snd.as_slice());
    let result = patch.to_bytes();

    let mut patch = diffy::Patch::from_bytes(result.as_slice()).unwrap();
    c.bench_function(&*(name.to_string() + "_init_patch"), |b| b.iter(|| {
        patch = diffy::Patch::from_bytes(result.as_slice()).unwrap();
    }));

    println!(
        "Count patches: {}, file_lengths: {}, diff_size_in_bytes: {}",
        patch.hunks().len(), fst.len() + snd.len(), result.len()
    );

    let mut res= Vec::new();
    c.bench_function(&*(name.to_string() + "_patch"), |b| b.iter(|| {
        res = diffy::apply_bytes(&fst, &patch).unwrap();
    }));

    assert_eq!(res, snd);
}

fn criterion_diff_binary(c: &mut Criterion) {
    bench_diff_binary("binary_1", c, "1.jpg", "2.jpg");
    bench_diff_binary("binary_2", c, "10.png", "11.png");
    bench_diff_binary("binary_3", c, "20.png", "21.png");
    bench_diff_binary("binary_4", c, "40.png", "41.png");
    bench_diff_binary("binary_40", c, "40.png", "40.png");
    bench_diff_binary("binary_5", c, "50.png", "51.png");
}

criterion_group!(benches_diff,
    criterion_diff,
    criterion_zip,
    criterion_diff_binary
);
criterion_main!(benches_diff);
