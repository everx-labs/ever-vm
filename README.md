# TVM
TON Virtual Machine implementation

## Prerequisites

https://www.rust-lang.org/en-US/install.html

## To Build & Run:

```
cargo build
cargo run
```

## Compile smart contract:

After build project you can use **compile** util from `target/release/compile` or `target/debug/compile` for compile your contract.

Commands (by unix example):
- Compile contract
  `./compile your_bytecode_file your_cells_file`
- Get help
  `./compile --help`

## Execute smart contract:

After build project you can use **execute** util from `target/release/execute` or `target/debug/execute` for execute your contract.

Commands (by unix example):
- Execute contract
  `./execute your_contract_file`
  - Execute contract with stack items (strings)
    `./execute your_contract_file --params stack-items`
- Get help
  `./execute --help`
