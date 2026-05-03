# Langlog Integer Safety Notes

Status: research note. This is not a normative language specification. It is a
survey and design rationale for Langlog's integer safety strategy. Normative
requirements should eventually move into a broader language semantics
specification. The current normative home for checked-result arithmetic is
[SEMANTICS.md](./SEMANTICS.md).

## Summary

Langlog should move toward checked-result arithmetic as the default for ordinary
numeric operators. In that model, basic arithmetic remains safe and ergonomic
for ordinary code, because overflow and underflow are represented as values
rather than silent wraparound or compile-time proof failures.

The preferred direction is:

```text
u32 + u32 -> Result<u32, ArithmeticError>
u32 - u32 -> Result<u32, ArithmeticError>
u32 * u32 -> Result<u32, ArithmeticError>
u32 / u32 -> Result<u32, ArithmeticError>
u32 % u32 -> Result<u32, ArithmeticError>
```

Arithmetic operators should also lift over compatible `Result` operands, so
compound expressions propagate arithmetic errors outward:

```text
Result<u32, ArithmeticError> + u32 -> Result<u32, ArithmeticError>
u32 + Result<u32, ArithmeticError> -> Result<u32, ArithmeticError>
Result<u32, ArithmeticError> + Result<u32, ArithmeticError>
    -> Result<u32, ArithmeticError>
```

This changes the role of proof. Instead of requiring every normal arithmetic
operation to be statically proven before it can compile, proof-oriented "raw"
or "unchecked-after-proof" arithmetic can become an explicit fast path. Most
programs get safe defaults. Performance-sensitive code can opt into operations
that create proof obligations and lower to plain backend arithmetic once those
obligations are discharged.

The design goal is:

- default arithmetic is checked and returns `Result`;
- arithmetic errors compose through larger expressions;
- programmers collapse or propagate those results at explicit boundaries;
- no implicit numeric promotion occurs;
- fast/raw arithmetic is explicit and proof-backed;
- backend runtime checks can use the best strategy for the target: overflow
  flags, widened arithmetic, or precondition checks.

## Why Integer Overflow Needs A Policy

Integer overflow is not one problem. It is a family of problems where different
systems make different tradeoffs:

- C and C++ distinguish between defined unsigned wraparound and undefined
  signed overflow, which creates both optimization opportunities and serious
  bug risk. Dietz, Li, Regehr, and Adve found that real programs contain both
  intentional and accidental overflow, and that undefined overflow has broken
  programs under compiler optimization improvements. Their conclusion is useful
  for Langlog: overflow behavior must be explicit enough that tools can
  distinguish intent from accident.
- Rust treats arithmetic overflow as programmer error while keeping intentional
  wrapping available through explicit operations such as `wrapping_add` and
  `Wrapping<T>`. It also exposes checked, overflowing, saturating, and wrapping
  APIs for programmers who need a specific policy.
- WebAssembly integer arithmetic wraps for ordinary add/sub/mul instructions.
  That is an execution-platform fact, not a good surface-language default for a
  safe language.

Langlog should not inherit Wasm's wrapping semantics accidentally just because
Wasm is the first backend. Wasm is a target. Langlog arithmetic semantics
should be chosen for developer clarity, safety, and later optimization.

## Candidate Strategies

### 1. Always Wrapping Arithmetic

In this model, `u32 + u32` always computes modulo `2^32`, matching Wasm `i32`
addition and Rust's explicit `wrapping_add`.

Advantages:

- cheapest lowering to Wasm;
- predictable at the machine level;
- useful for hashes, counters, cyclic buffers, cryptography, and bit-level
  algorithms;
- no proof obligation for overflow.

Disadvantages:

- many arithmetic mistakes silently become valid programs;
- diagnostics cannot distinguish accidental overflow from intended wraparound;
- ordinary business logic and indexing logic become harder to reason about;
- proof facts over mathematical ranges become less intuitive because arithmetic
  is no longer ordinary arithmetic.

This should not be the default. It should become an explicit operation later:

```langlog
value.wrapping_add(amount)
```

or, depending on the eventual method/associated-function design:

```langlog
u32::wrapping_add(value, amount)
```

### 2. Trap-On-Overflow Arithmetic

In this model, ordinary arithmetic traps or aborts when overflow occurs.

Advantages:

- ordinary arithmetic still returns plain `u32`;
- overflow cannot silently continue;
- maps to some languages' debug or strict modes;
- conceptually simple at the expression level.

Disadvantages:

- introduces hidden control flow;
- makes error handling less explicit;
- complicates embedding and browser playground behavior;
- makes it harder for callers to recover locally;
- still requires runtime checks unless proven away.

Trap-on-overflow may be useful as a future mode or explicit operation, but it
does not fit Langlog's desire to make failure paths visible and composable.

### 3. Proof-First Ordinary Arithmetic

In this model, ordinary arithmetic is accepted only when the compiler can prove
the result fits in the destination type. Proven operations lower to ordinary
backend arithmetic. Unproven operations fail proof checking.

Advantages:

- no runtime overhead for proven arithmetic;
- makes integer safety part of the language's proof story;
- keeps default arithmetic mathematical rather than wrapping;
- works well when bounds are already part of the program's logic.

Disadvantages:

- rejects many safe programs until the proof engine is strong enough;
- makes simple programs feel proof-heavy;
- requires excellent diagnostics to be ergonomic;
- asks most programmers to think about proof obligations even outside hot
  paths.

This was the previous preferred direction. It remains valuable, but it should
be moved out of ordinary operators and into explicit raw/proof-backed
operations for code that needs the performance profile.

### 4. Checked-Result Arithmetic

In this model, ordinary arithmetic performs checked arithmetic and returns
`Result<T, ArithmeticError>`.

Advantages:

- safe by default;
- ordinary code can recover from arithmetic failure explicitly;
- complex expressions can propagate arithmetic errors without boilerplate;
- proof is no longer required just to write simple arithmetic;
- backend implementations can choose efficient runtime checks per target;
- fast paths remain possible through explicit raw/proof-backed operations.

Disadvantages:

- arithmetic becomes effectful;
- expression types become `Result` more often;
- the language needs good recovery/collapse operators early;
- backend V1 must represent arithmetic errors at runtime;
- type checking must define lifting rules clearly.

This is now the preferred direction for Langlog's default arithmetic.

### 5. Explicit Arithmetic Modes

Checked-result default arithmetic should coexist with explicit operations for
other policies:

- `value.raw_add(amount)` creates a proof obligation and lowers to plain
  arithmetic after proof succeeds;
- `value.wrapping_add(amount)` computes modulo `2^N`;
- `value.saturating_add(amount)` clamps at numeric bounds;
- `value.overflowing_add(amount)` returns `(value, overflowed)`;
- `value.trapping_add(amount)` traps or aborts on overflow if we want that API.

The exact spelling is open. Method-like syntax is attractive because it keeps
the operation attached to the receiver type and leaves room for associated
constants such as `u32::MAX`.

## Checked-Result Typing Model

The language should not perform implicit numeric promotion. Numeric binary
operators should require both operands to have the same underlying numeric
type.

However, arithmetic should lift over `Result<T, ArithmeticError>` so complex
expressions compose:

```text
T op T -> Result<T, ArithmeticError>
Result<T, ArithmeticError> op T -> Result<T, ArithmeticError>
T op Result<T, ArithmeticError> -> Result<T, ArithmeticError>
Result<T, ArithmeticError> op Result<T, ArithmeticError>
    -> Result<T, ArithmeticError>
```

The important distinction is that operands must agree on the same value type
after unwrapping the arithmetic-result layer. These should be valid:

```langlog
let total = a + b * c - d;
```

where `total` has type:

```text
Result<u32, ArithmeticError>
```

These should not be accepted without explicit conversion:

```text
u32 + u64
i32 + u32
Result<u32, ArithmeticError> + Result<u64, ArithmeticError>
```

Error propagation should be left-to-right and deterministic. If multiple
subexpressions fail, the first failure in evaluation order should be the value
that propagates. That rule should eventually be made precise in the language
semantics spec.

## Recovering From `Option` And `Result`

Checked-result arithmetic only becomes ergonomic if the language has a compact
way to collapse or recover from `Result` values.

The proposed shape is:

```langlog
option_expr or fallback_expr
result_expr or(err) fallback_expr
```

For `Option<T>`:

```text
Option<T> or T -> T
```

For `Result<T, E>`:

```text
Result<T, E> or(err) T -> T
```

Inside the fallback expression, `err` is in scope and has type `E`.

Examples:

```langlog
fn add_or_zero(a: u32, b: u32) -> u32 {
    a + b or(err) {
        0
    }
}
```

```langlog
fn add_or_report(a: u32, b: u32) -> u32 {
    a + b or(err) {
        print_u32(0);
        0
    }
}
```

The fallback expression must produce the same value type as the success case.
The result-collapsing operator should not silently discard errors without a
visible recovery expression.

This operator is deliberately not logical `||`. It is a recovery operator. The
surface spelling can still be debated, but the language needs this concept
early if arithmetic returns `Result`.

## Runtime Check Placement

If ordinary arithmetic returns `Result`, the backend must implement checked
arithmetic. The important performance question is how to implement the check.

### Precondition Checks

A precondition check proves safety before executing the operation:

```text
a + b: a <= u32::MAX - b
a - b: a >= b
a * b: b == 0 || a <= u32::MAX / b
```

Advantages:

- conditions are easy to explain to programmers;
- same conditions are useful as proof obligations for raw arithmetic;
- no need to execute a potentially overflowing operation first;
- portable to targets without overflow flags.

Disadvantages:

- may need extra arithmetic just to compute the condition;
- multiplication checks can require division;
- some checks duplicate work if the target has cheap overflow flags.

### Overflow-Flag Or Postcondition Checks

Many compilers and CPUs can produce a result plus an overflow bit. LLVM exposes
this shape directly through intrinsics such as `llvm.uadd.with.overflow`,
`llvm.usub.with.overflow`, and `llvm.umul.with.overflow`. Rust's unsigned
`overflowing_add` is also implemented around overflow intrinsics, and current
`checked_add` uses `add_with_overflow` to decide whether to return `None`.

Advantages:

- often maps naturally to hardware flags;
- computes result and overflow information together;
- can avoid expensive precondition expressions;
- may be a better runtime implementation for checked operations.

Disadvantages:

- target-dependent;
- less directly helpful for source-level proof diagnostics;
- Wasm currently exposes wrapping integer operations directly, not a general
  scalar overflow-flag instruction for all integer arithmetic;
- may require compiler-specific lowering or helper code.

### Widened Arithmetic

Another implementation is to widen operands, compute in a larger type, and test
whether the result fits.

Advantages:

- simple for `u32` when `u64` is available;
- easy to reason about;
- can be convenient in an interpreter or non-Wasm backend.

Disadvantages:

- not always available for the largest integer type;
- can be slower or larger than flag-based code;
- changes register pressure and backend code shape.

### What The Research Suggests

The main lesson from existing work is not "precondition is always faster" or
"flag checks are always faster." The IOC work by Dietz, Li, Regehr, and Adve
is especially relevant because it had to detect integer overflow dynamically in
real C/C++ programs. Their empirical result is that integer overflow is common,
subtle, and often misunderstood, and their tool work shows that different
checking strategies have different optimization behavior.

For Langlog, the safe conclusion is:

- do not guess a universal runtime-check strategy;
- make the backend choose a target-appropriate checked arithmetic lowering;
- benchmark precondition checks, overflow-like lowering, and widened
  arithmetic on the actual backend;
- keep proof obligations available for explicit raw arithmetic, even though
  ordinary arithmetic uses checked results.

## Current Langlog Arithmetic Model

The current implementation is still proof-first:

- `+`, `-`, and `*` create arithmetic safety obligations;
- `/` and `%` create non-zero divisor obligations;
- array indexing creates an in-bounds obligation;
- `observe` and control-flow comparisons create facts that can discharge those
  obligations;
- mutable facts are tracked as hints for diagnostics but do not discharge
  obligations.

This model is useful infrastructure, but it should be reinterpreted as the
future foundation for explicit raw/proof-backed arithmetic rather than the
long-term default for ordinary `+`, `-`, `*`, `/`, and `%`.

The implementation migration should not throw away the proof machinery. It
should redirect it:

- ordinary arithmetic returns `Result`;
- explicit raw arithmetic creates proof obligations;
- proven raw arithmetic lowers to plain backend arithmetic;
- wrapping/saturating/trapping arithmetic use separate explicit operations.

## Proof Obligations For Raw `u32` Arithmetic

For future raw/proof-backed `u32` arithmetic, the useful source-level
obligations remain:

```text
a.raw_add(b) is safe when a <= u32::MAX - b
a.raw_sub(b) is safe when a >= b
a.raw_mul(b) is safe when b == 0 || a <= u32::MAX / b
a.raw_div(b) is safe when b != 0
a.raw_rem(b) is safe when b != 0
```

For range facts:

```text
a in [a_min, a_max]
b in [b_min, b_max]

a.raw_add(b) is safe when a_max + b_max <= u32::MAX
a.raw_sub(b) is safe when a_min >= b_max
a.raw_mul(b) is safe when a_max * b_max <= u32::MAX
```

These remain important diagnostic targets. They are simple, explainable, and
map onto the proof engine's current range model.

For constants and exact ranges, the compiler can discharge raw arithmetic
obligations without an explicit observation:

```langlog
fn main() -> u32 {
    40.raw_add(2)
}
```

For variable values, users should be able to write observations such as:

```langlog
fn add_one(value: u32) -> u32 {
    observe value <= 4294967294 else {
        return 0;
    }
    value.raw_add(1)
}
```

With numeric constants, this becomes clearer:

```langlog
fn add_one(value: u32) -> u32 {
    observe value < u32::MAX else {
        return 0;
    }
    value.raw_add(1)
}
```

The method names here are illustrative. The eventual language design may pick
different names, but the key point is that proof obligations attach to explicit
raw arithmetic, not ordinary arithmetic.

## Numeric Bounds Constants

Langlog should provide min/max constants for numeric types. Rust's associated
constant style is a good model:

```langlog
u32::MIN
u32::MAX
```

Advantages:

- scales to future numeric types;
- keeps bounds attached to the type;
- avoids global names like `U32_MAX`;
- matches programmer expectations from Rust;
- makes diagnostics and examples much clearer.

The shorter-term alternative is to add builtin globals:

```langlog
U32_MIN
U32_MAX
```

That is easier to parse and resolve, but it does not scale as cleanly. It also
creates global namespace pressure. The better design is `u32::MAX`, even if it
requires adding path or associated-constant syntax first.

Numeric bounds should be usable in:

- ordinary expressions;
- proof expressions;
- `observe` statements;
- generated diagnostic suggestions;
- raw arithmetic proof obligations.

## Diagnostic Strategy

Default checked-result arithmetic should produce normal type and recovery
diagnostics. The most important messages will be about unhandled
`Result<T, ArithmeticError>` values:

```text
expected `u32`, found `Result<u32, ArithmeticError>`
help: recover with `or(err) ...` or return/propagate the result
```

For explicit raw arithmetic, diagnostics should become operator-specific.

Raw addition:

```text
possible u32 raw addition overflow is not proven safe
help: prove the left operand is at most `u32::MAX - <right>`
```

Raw subtraction:

```text
possible u32 raw subtraction underflow is not proven safe
help: prove the left operand is greater than or equal to the right operand
```

Raw multiplication:

```text
possible u32 raw multiplication overflow is not proven safe
help: prove one operand is zero, or prove the other operand is at most
      `u32::MAX / <operand>`
```

Raw division and remainder:

```text
possible raw divide-by-zero is not proven safe
help: prove the divisor is not zero
```

Until `u32::MAX` exists, diagnostics can mention the numeric value
`4294967295`, but that should be considered temporary. The better long-term UX
is to explain obligations in terms of named type bounds.

Generated observation suggestions should be conservative. A suggestion should
only be emitted when the suggested expression is valid Langlog today. If the
ideal expression is not currently valid proof syntax, the diagnostic should
state the rule in prose rather than suggesting code that cannot compile.

## Backend Strategy

For checked-result ordinary arithmetic:

- the backend must produce a success value or an `ArithmeticError`;
- the backend should choose the best check strategy for the target and type;
- Wasm V1 may start with simple precondition or widened checks if that is
  easiest;
- later backends can use overflow flags or intrinsics when available;
- complex arithmetic expressions should avoid repeated wrapping/unwrapping when
  the backend can fuse checks safely.

For explicit raw/proof-backed arithmetic:

- proof failure remains a compile-time error;
- proven-safe operations can lower to plain backend arithmetic;
- no runtime overflow checks are needed for raw arithmetic that reaches code
  generation;
- explicit wrapping operations can lower directly to Wasm wrapping arithmetic;
- explicit saturating/trapping/overflowing operations can be implemented
  independently.

This deliberately separates Langlog semantics from Wasm mechanics. Wasm add is
wrapping, but Langlog ordinary add is checked-result arithmetic, and Langlog raw
add is only emitted when proof has shown that wrapping cannot occur.

## Open Questions

- What should the concrete `ArithmeticError` type contain: just an enum tag, or
  operation/type/source information?
- Should arithmetic errors be one shared type or type-specific errors such as
  `U32ArithmeticError`?
- Should checked arithmetic expressions automatically propagate only
  `ArithmeticError`, or should there be a more general effect/result lifting
  rule?
- What is the exact syntax for method-like raw/wrapping/saturating operations?
- Should checked arithmetic return `Result<T, ArithmeticError>` directly, or a
  special compiler-known result type that aliases to normal `Result`?
- How soon should `u32::MIN`, `u32::MAX`, and `u32::BITS` be added?
- Should result recovery use `or(err)` or another spelling?
- Should Langlog later add a propagation operator similar to Rust's `?`?
- How should this model generalize to signed integers, narrower integer types,
  and future generic numeric code?

## Recommended Direction

The recommended Langlog strategy is:

1. Make ordinary arithmetic checked-result arithmetic.
2. Lift arithmetic operators over compatible `Result<T, ArithmeticError>`
   operands so complex expressions propagate errors.
3. Add recovery operators for `Option` and `Result`, including an error-binding
   form such as `or(err)`.
4. Keep the proof engine, but use it for explicit raw/proof-backed arithmetic
   rather than ordinary arithmetic.
5. Add numeric bounds constants, preferably `u32::MIN` and `u32::MAX`.
6. Add explicit wrapping/saturating/overflowing/trapping APIs later.
7. Benchmark runtime checked arithmetic strategies before committing to a
   lowering strategy for each backend.

This gives Langlog safe defaults without forcing every arithmetic-heavy program
to use proof annotations, while preserving an explicit path to fast proven
arithmetic when developers need it.

## References

- Will Dietz, Peng Li, John Regehr, and Vikram Adve, "Understanding Integer
  Overflow in C/C++", ICSE 2012:
  <https://llvm.org/pubs/2012-06-08-ICSE-UnderstandingIntegerOverflow.html>
- Rust Reference, "Integer overflow":
  <https://doc.rust-lang.org/reference/behavior-not-considered-unsafe.html#integer-overflow>
- Rust Reference, operator overflow behavior:
  <https://doc.rust-lang.org/stable/reference/expressions/operator-expr.html#overflow>
- Rust `core::num` unsigned integer implementation:
  <https://doc.rust-lang.org/src/core/num/uint_macros.rs.html>
- LLVM overflow intrinsics:
  <https://releases.llvm.org/10.0.0/docs/LangRef.html#llvm-uadd-with-overflow-intrinsics>
- WebAssembly numeric semantics:
  <https://webassembly.github.io/spec/core/exec/numerics.html>
