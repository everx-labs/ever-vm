## Problem statement

Ever/Solidity exception handling mechanism, inspired by Java and C++, keeps all the effects already applied to the outer scope when an exception occurs:
```
function example(int x) private pure returns (int) {
  try {
    x += 1;
    x /= 0;
  } catch (variant, uint16) {
    x += 1;
  }
  return x;
}
function example_test() public pure {
  require(sample(100) == 102, 500);
}
```

Ever/Solidity compiler needs to produce an assembler code which, when an exception occurs, is required to
1. keep all (perhaps mutated) outer-scope variables and
2. drop any extra values pushed on top of the kept ones in the try-block before entering the catch-block.

However, from the VM perspective, the existing EH mechanism discards the entire stack of current continuation and passes exactly two stack values to an exception handler. It's especially true for exceptions thrown by ordinary instructions, like `DIV` or `LDI` — no `SETCONTVARARGS` trickery can be done in this case (as it could be for explicit `THROW`-like instructions with some assistance from the compiler). Therefore, it's impossible to support the first requirement above.

## Proposed solution

We propose adding one new instruction, `TRYKEEP`, which will allow running a slightly modified exception handling sequence. Effectively, an exception handler will also get the outer-scope stack slots besides just an exception pair.

The proposed description of the new instruction is given in the `doc/tvm.tex` file and is copied here:

* `F2FE` — `TRYKEEP` ($c$ $c'$ — ), similar to `TRY`, but when an exception occurs, $c'$ receives all the stack slots populated upon entering $c$ with two exception arguments on top. All changes made by $c$ in those slots are kept and become visible to $c'$; all extra slots created by $c$ on top of the kept ones get discarded upon entering $c'$.

Internal implementation of the `TRYKEEP` instruction will involve the following changes to VM:

- when `TRYKEEP` ($c$ $c'$ — ) executes, VM envelopes $c'$ into a new dedicated CatchRevert(depth) continuation with the depth parameter set to the initial stack size (i.e. without both $c$ and $c'$ on the stack);

- when an exception occurs, and if $c2$ is a CatchRevert(depth), then VM shrinks the current stack to the depth size, pushes two exception arguments on top, and executes $c'$ by opening the envelope.
