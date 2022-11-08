# TVM
TON Virtual Machine implementation

## Prerequisites

https://www.rust-lang.org/en-US/install.html

## To Build & Run:

```
cargo build
```
## Warning:
Any changes to master are automatically applied to [ton-labs-vm](https://github.com/tonlabs/ton-labs-vm/) public repository except tests. If you add new dependencies required for testing, please adjust [remove_tests.sh](https://github.com/tonlabs/ton-labs-vm/blob/master/remove_tests.sh) accordingly.

## To Test:
```
cargo test
```
## Features:
`--features`
`ci_run` - run long tests
`fift_check` - check test results using fift binaries should be near test executable
`log_file` - ouput log to file
`verbose` - show execution process, don't forget to call `logger::init()`

## Verbose output
We can get verbose information about TVM execution, such as primitive name with parameters, stack dump and values of control registers after each executed command.
Logging can work in some ways:
1. We want to get verbose output of one broken own test in TVM. Run this test with key --features verbose
`cargo test --test test_gas buygas_normal --features verbose`

2. We want to get verbose output of TVM execution wich is inluded as library to other application (for example node)
In application use log4rs crate init procedure `log4rs::init_file` or use predefined set from TVM calling `ton_vm::init_full` with relative path to config file.
Available targets in logging are: `compile` - trace compile process and `tvm` - trace execution process
The level of tracing: trace and higher

See https://docs.ton.dev for documentation

---
Copyright (C) 2019-2021 TON Labs. All Rights Reserved.

Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
this file except in compliance with the License.

You may obtain a copy of the
License at: https://www.ton.dev/licenses

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific TON DEV software governing permissions and
limitations under the License.

