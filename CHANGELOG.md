# Release Notes

All notable changes to this project will be documented in this file.

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
