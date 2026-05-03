# Langlog

Langlog is an experimental programming language about making programs unable to
fail at runtime.

The long-term goal is not to prove that an algorithm computes the right answer.
Langlog is not trying to prove sorting algorithms, scheduling policy, business
logic, or mathematical intent. The goal is narrower and more operational:

- no runtime panics, including arithmetic traps, invalid indexing, missing map
  entries, and allocation failure;
- all functions are total, with bounded execution instead of recursion or
  unbounded loops;
- memory and other resources have explicit bounds, and exhaustion is part of
  the type-level/control-flow story rather than a hidden process failure;
- only valid states can be reached, even when the underlying data structures are
  capable of representing invalid states.

In other words, a Langlog program may still choose a bad policy or compute a
wrong answer, but it should not crash, panic, run forever, silently overflow,
index out of bounds, or transition into a state that violates declared
invariants.

Try the browser playground: <https://0xa9f4.com/playground/>

## Core Idea

Mainstream systems languages have made major progress on memory safety, but
many production failures are not memory-safety bugs. They are divide-by-zero,
overflow, unchecked indexing, missing data, resource exhaustion, unexpected
panics, unbounded loops, and state transitions that rely on careful review
rather than enforceable rules.

Langlog explores a language design where those hazards are structural:

- Fallible operations return `Option` or `Result`, or require proof that failure
  cannot occur.
- Iteration is bounded. Recursion is rejected.
- Proof obligations are created by potentially failing operations.
- Observations from `if` conditions and explicit `observe ... else { ... }`
  statements can discharge those obligations.
- Capacity-bounded collections are part of the type system so relations between
  collections can eventually be checked.

The journal that started the project is [journel.md](./journel.md). It sketches
the reliability motivation, bounded allocation ideas, and the
obligation/observation model.

## Current Prototype

The active implementation lives in [experiments/langlog](./experiments/langlog).
It is a Rust workspace containing:

- `langlog-syntax`: lexer, parser, AST, source spans, and syntax diagnostics.
- `langlog-sema`: name resolution, type checking, recursion rejection,
  mutability checks, and HIR lowering.
- `langlog-proof`: proof obligations for arithmetic safety, divide/remainder by
  zero, bounds checks, and early collection-relation work.
- `langlog-wasm`: a Wasm backend for the current executable subset.
- `langlog-driver`: the command-line interface.
- `langlog-playground-wasm`: the browser playground compiler adapter.

Implemented language features include:

- single-file programs with top-level `fn` items;
- `u32`, `bool`, `()`, tuples, fixed arrays, `Option<T>`, `Result<T, E>`,
  `ArithmeticError`, `range<u32>`, and parsed `Set`/`Map` shells;
- checked arithmetic returning `Result<u32, ArithmeticError>`;
- recovery expressions with `or` and `or(err)`;
- bounded `for` loops over arrays and ranges;
- `if`, `match`, local bindings, mutable local assignment, `return`, calls, and
  block expressions;
- structural equality for flattened values in the Wasm backend;
- explicit `observe` statements for proof facts;
- a Wasm backend that can run the current non-collection executable subset in
  the browser playground.

`Set` and `Map` currently participate in checking/proof work, but they are not
yet executable runtime collections in Wasm.

## What Langlog Is Not

Langlog is intentionally not a general theorem prover for program meaning.

It does not try to prove that:

- an algorithm is optimal;
- a sorted output is actually sorted;
- a business rule is the correct business rule;
- a chosen state transition is semantically desirable.

Instead, Langlog aims to prove that execution stays inside the operational
contract: every operation is defined, every loop is bounded, every failure path
is explicit, every allocation failure is handled, and every declared state
invariant is preserved.

This is closer to proving "the program cannot fail or enter an invalid state"
than proving "the program is the algorithm I intended."

## Running The Prototype

From the active experiment directory:

```sh
cd experiments/langlog
./tasks.sh
```

Check a file:

```sh
cargo run -p langlog-driver --bin langlog -- check examples/tutorial.llg
```

Build a Wasm module:

```sh
cargo run -p langlog-driver --bin langlog -- build --target wasm examples/tutorial.llg
```

Build the local playground site:

```sh
./tasks.sh playground
```

Serve it locally:

```sh
./tasks.sh playground-serve
```

## Documentation

Useful project documents:

- [Reference manual](./experiments/langlog/REFERENCE.md)
- [Tutorial](./experiments/langlog/TUTORIAL.md)
- [Language specification](./experiments/langlog/SPEC.md)
- [Checked arithmetic semantics](./experiments/langlog/SEMANTICS.md)
- [Proof IR](./experiments/langlog/PROOF_IR.md)
- [Wasm backend spec](./experiments/langlog/WASM.md)
- [Implementation plan](./experiments/langlog/PLAN.md)

The project is still an experiment. Syntax, semantics, proof rules, and runtime
design are expected to change as the reliability model becomes sharper.
