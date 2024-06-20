# Release Notes

All notable changes to this project will be documented in this file.

## Version 2.2.0

- Use modern crates anyhow and thiserror instead of failure

## Version 2.1.9

- Fixed BLS_* instructions behavior in case of bad data

## Version 2.1.8

- Fix build warnings

## Version 2.1.4

- Removed using ed25519 crates directly

## Version 2.1.0

- the crate was renamed from `ton_vm` to `ever_vm`
- supported renaming of other crates

## Version 2.0.0

- Added BLS commands set

## Version 1.9.7

- Changed exception type when storing NaN integer on stack

## Version 1.9.5

- Bugfixes

## Version 1.9.3

- Check the type of exotic cell for ENDXC insn

## Version 1.9.2

- Revise the price of signature checking instructions

## Version 1.9.1

- Fix a gas computation bug in stack moving

## Version 1.9.0

- Fix ENDS insn: account for refs too

## Version 1.8.217

- BLS pubkey dummy

## Version 1.8.212

- Public slice_serialize and slice_deserialize functions

## Version 1.8.209

- Benchmark massive cell making

## Version 1.8.208

- Optimize IntegerData::as_slice()

## Version 1.8.207

- Add a tuples benchmark

## Version 1.8.206

- Recover load_hashed_cell() perf

## Version 1.8.194

- Drop trees of StackItem::Tuple iteratively

## Version 1.8.192

- Add cc() and ctrls() methods

## Version 1.8.190

- Fix depth bug of exotic cell

## Version 1.8.187

- Add new GASREMAINING insn

## Version 1.8.186

- Add new TRYKEEP instruction

## Version 1.8.177

- Supported changed interface of BocWriter::with_roots

## Version 1.8.170

- Disable debug symbols by default

## Version 1.8.161

- Fix TRY nargs bogus modification

## Version 1.8.160

- Use SHA algos from ever-types

## Version: 1.8.159

- Minor fix for support changes in types

## Version: 1.8.131
### Fixed
 - check_signature: on bad pubkey/signature return false

## Version 1.8.128
- Supported ever-types version 2.0

## Version: 1.8.97
### Fixed
 - Fix DATASIZE primitives with GlobalCapability

## Version: 1.8.94
### Fixed
 - Put new code under Capability

## Version: 1.8.93
### Fixed
 - Removed recursion in STCONT/LDCONT primitives

## Version: 1.8.82
### Fixed
 - Fixed STCONT/LDCONT primitives for tuples and continuations

## Version: 1.8.79

### Fixed
 - Adapt some functions (CDATASIZE, XCTOS, XLOAD, CTOS) to library cells

## Version: 1.8.78
### Fixed
 - Fixed debug output

## Version: 1.8.77

### Fixed
 - Stack overflow during load library cell

## Version: 1.8.71

### New
- Add common as submodule

### Fixed
- Some minor fixes

## Version: 1.8.60

### Fixed
 - Fix for cells loading
 - Some refactoring for remove direct usage of Arcs
 - Bumped versions of some creates

## Version: 1.8.40

### Fixed
 - Fixed several potential panics

## Version: 1.8.39

### New
 - Implemented MYCODE primitive
 - Implemented COPYLEFT primitive
 - Implemented COPYLEFT primitive
 - Implemented STORAGE_FEE primitive
 - Implemented TRYELECT primitive
 - Implemented SEQNO primitive
 - Refactored code for cargo clippy
 - Optimized prefomance
 - Simplified SPEC_LIMIT is nothing more than i64::MAX
 - Handle BUYGAS out-of-gas condition the same way as for SETGASLIMIT
 - Supported new cells (possibly without tag)
 - Some micro optimizations for hot spots
 - Make SaveList a vector instead of hashmap
 - Simplify StackItem::as_continuation_mut
 - Eliminate cloning of cmd_code's cell
 - Put log-related computations under an if
 - Improve perf of ContinuationData ctors
 - Do arith operations in-place
 - Get rid of swaps in step_while_loop()
 - Optimize transplanting of the topmost range of a stack
 - Optimize switching of loop iterations
 - Simplify SaveList::apply()
 - Improve move_stack_from_cc(): add a special case, remove unsafe code
 - Add a script tuning a linux machine for finer benchmarking
 - Add bigint benchmarks
 - Turn off pointless benchmarking of tests; improve profiling
 - Put tracing under a check to save a bunch of cycles
 - Specialize switch() for the case of switching to c0
 - Disable rug-bigint benchmark since CI can't build gmp-mpfr-sys
 - Make StackItem variants hold Rc instead of Arc
 - Streamline integer manipulations
 - Add load-boc benchmark
 - Make SaveList's storage an array
 - Remove unnecessary engine.cmd reset
 - Split Instruction struct
 - Remove Context struct
 - Move cc parts into loop cont instead of cloning
 - Move last_cmd field out of ContinuationData into Engine
 - Add SaveList::put_opt() an unchecked version of put()
 - Improve ContinuationData printing
 - Remove unused c6 from SaveList
 - Do addition in-place
 - Simplify raise_exception()
 - Add assertions
 - Add handlers printer
 - Add a script for estimating the tests coverage
 - Address feedback
 - Fix after rebase
 - Add deep stack switch test
 - Add a benchmark for deep stack switch
 - Minor improvements
 - Minor optimization

### Fixed
 - Fixed ZEROSWAP* and ZEROROT* promitives are fixed - check for bool instead of zero
 - Fixed empty AGAIN, REPEAT loops
 - Fixed GRAMTOGAS
 - Fixed BUYGAS

## Version: 1.8.38

### New

- Implemented behavior modifier mechanism
- Implemented behavior modifier for skipping check of signature for offline execution purposes

### Fixed
- Fixed tvm.tex and tvm.pdf
