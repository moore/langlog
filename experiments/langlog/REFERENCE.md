# Langlog Reference Manual

Status: phase 1 language and tooling reference for the current experiment.
This document describes the surface language accepted by `langlog check`, the
initial semantic and proof checks, and the current Wasm V1 backend.

See also:

- [TUTORIAL.md](./TUTORIAL.md) for a guided introduction
- [SPEC.md](./SPEC.md) for the broader language design goals

## Files And Compilation

- A phase 1 Langlog program is a single source file.
- The supported check commands are `langlog check <path>` and
  `langlog check --warnings-as-errors <path>`.
- The supported build command is `langlog build --target wasm <path>`.
- Successful checks print their summary to stdout. Warnings print to stderr.
- `--warnings-as-errors` promotes warnings into failing diagnostics.
- Successful Wasm builds print the output artifact path to stdout.
- Build configuration may be provided by a `.langlog-config` file discovered
  from the source directory upward:

  ```toml
  [build]
  target = "wasm"
  out_dir = "target/langlog"
  ```

- The current parser accepts one or more top-level function items.
- The task-orchestration surface described below is the next M6 design target;
  compiler support is not implemented yet.
- Multi-file compilation, imports, modules, and packages do not exist yet.

## Lexical Structure

### Comments

- Line comments start with `//` and continue to the end of the line.
- Block comments use `/* ... */`.
- Nested block comments are accepted by the current lexer.

### Identifiers

- Identifiers use ASCII letters, digits, and `_`.
- The first character must be a letter or `_`.

### Literals

- Integer literals are unsigned base-10 integers.
- Boolean literals are `true` and `false`.

### Keywords

Reserved keywords in the current language:

`fn`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`, `return`, `observe`,
`or`, `true`, `false`

Task orchestration additionally reserves:

`task`, `forever`, `exit`, `delegate`

## Items

The current executable subset uses top-level functions:

```langlog
fn name(param1: Type, param2: Type) -> Type {
    ...
}
```

Notes:

- The return type is optional in the parser.
- Functions with no meaningful value should use `()`.
- Recursion is a language-level non-goal, but recursion rejection is part of
  later semantic checking rather than parsing.

The planned task-orchestration surface also adds top-level tasks:

```langlog
task name(param1: Type, param2: Type) -> Type {
    ...
}
```

Tasks are orchestration code, not ordinary total functions.

Rules:

- A task return type is mandatory.
- Executable task programs use `task main() -> u32` as the root task for now.
- Ordinary functions cannot call tasks.
- Tasks can call ordinary functions.
- Tasks can transfer to other tasks with `delegate`.
- `delegate` is a terminal orchestration statement and does not return to the
  caller.
- A `delegate` statement requires the callee return type to exactly match the
  caller task return type.
- Plain call syntax cannot call a task.
- Cyclic task delegation is rejected.

Examples:

```langlog
task main() -> u32 {
    exit 0;
}

task service() -> u32 {
    forever {
        tick();
    }
}

task setup() -> u32 {
    delegate worker();
}

task worker() -> u32 {
    exit 0;
}
```

Invalid examples:

```langlog
// Invalid: task return type is mandatory.
task missing_return_type() {
    exit 0;
}

task main() -> u32 {
    exit 0;
}

// Invalid: ordinary functions cannot call tasks.
fn not_allowed() -> u32 {
    main();
}

// Invalid: task calls require `delegate`, even inside another task.
task not_delegated() -> u32 {
    worker();
}

// Invalid: delegation requires matching return types.
task wrong_type() -> u32 {
    delegate shutdown();
}

task shutdown() -> bool {
    exit true;
}
```

## Statements

The current parser accepts these statements in functions. The planned
task-orchestration surface additionally defines `forever`, `delegate`, and
`exit` for task bodies.

### `let`

```langlog
let value: u32 = 1;
let mut total = 0;
let pending: Option<u32>;
```

Rules:

- `mut` is optional.
- The type annotation is optional.
- The initializer is optional.
- Successful semantic checking still requires at least one of the type
  annotation or initializer so the binding type is known.
- The statement must end with `;`.

### Assignment

```langlog
total = total + value or(err) 0;
```

Rules:

- Assignment is a statement, not an expression.
- The parser currently accepts any expression on the left-hand side.
- Semantic checking will narrow that later to valid assignable places.

### Expression Statement

```langlog
log_value(total);
```

Rules:

- The expression must end with `;`.
- A block may end with a trailing expression without `;`.

### `if`

```langlog
if total > 10 {
    observe total < 1001 else {
        return total;
    }
} else {
    observe total <= 10 else {
        return total;
    }
}
```

Rules:

- `if` is currently parsed as a statement.
- `else if` chains are accepted.
- Both branches use block syntax.

### `match`

```langlog
match flag {
    true => { value = 1; },
    false => { value = 2; }
}
```

Rules:

- `match` is currently parsed as a statement.
- Each arm uses `pattern => body`.
- The body may be a block or a single expression.
- Arms are comma-separated.

### `for`

```langlog
for value in values {
    total = total + value or(err) 0;
}
```

Rules:

- `for` is currently parsed as a statement.
- The loop binding uses the pattern grammar described below.
- The iterable is currently any expression syntactically.
- Semantic checking restricts loops to bounded forms.
- Phase 1 bounded loops allow range expressions, array literals, and bindings
  backed by fixed arrays or explicit-capacity `Set`/`Map` values.

### `return`

```langlog
return total;
return;
```

`return` is invalid inside task bodies. Tasks must terminate with `exit`,
`delegate`, or a non-nested `forever` loop.

### `forever`

```langlog
task main() -> u32 {
    forever {
        tick();
    }
}
```

`forever` is the task orchestration loop. It is valid only inside task bodies.

Rules:

- `forever` uses the form `forever { ... }`.
- A bare `forever` loop is a valid crash-only or externally terminated task.
- Nested `forever` loops are rejected in the initial task design.
- Each iteration is expected to contain bounded work; runtime scheduling and
  handler dispatch semantics are future work.

### `delegate`

```langlog
task setup() -> u32 {
    delegate worker();
}

task worker() -> u32 {
    exit 0;
}
```

`delegate` transfers orchestration from the current task to another task.

Rules:

- `delegate` uses the form `delegate name(args...);`.
- `delegate` is valid only inside task bodies.
- `delegate` must target a task, not an ordinary function.
- `delegate` does not return to the current task.
- The target task return type must exactly match the current task return type.
- Cyclic task delegation is rejected.

Future versions may consider using `delegate` for explicit tail calls to
ordinary functions, but the initial task-orchestration surface limits it to
tasks.

### `exit`

```langlog
task main() -> u32 {
    exit 0;
}
```

`exit` terminates the program from a task.

Rules:

- `exit` uses the form `exit <expr>;`.
- `exit` is valid only inside task bodies.
- The expression must match the task return type.
- A task body cannot accidentally fall through. Every reachable path must end
  in `exit`, a same-return-type `delegate`, or a non-nested `forever`.

### `observe`

```langlog
observe total <= 1000 else {
    return total;
}
```

`observe` records an explicit fact in the source program.

Rules:

- Phase 1 `observe` uses the form `observe <expr> <op> <expr> else <block>`.
- The `else` block is mandatory.
- Both sides must be phase 1 proof expressions.
- The supported operators are `==`, `!=`, `<`, `<=`, `>`, and `>=`.
- Phase 1 proof expressions allow scalar literals, names, grouping, unary
  operators, and arithmetic.
- Tuple, array, block, range, logical, equality, and comparison subexpressions
  are rejected inside phase 1 proof expressions.
- Non-proof call callees, call arguments, index targets, and index values are
  rejected inside phase 1 proof expressions.
- In phase 1, semantic checking rejects proof expressions that directly
  reference `mut` bindings.
- Semantic checking requires the `else` block to be terminal.
- When the observed relation is true, the proof phase records the relational
  fact for later checking.
- The `else` block runs when the observed relation is false.
- The proof phase also records simple comparison-based `if` conditions.
- Facts inferred from `if` conditions are proof-usable only when they refer to
  stable non-`mut` bindings.
- Comparisons over `mut` bindings are retained only for diagnostics: they can
  trigger warnings when an obligation would otherwise rely on them, but they do
  not discharge proofs.
- Ordinary arithmetic returns `Result<u32, ArithmeticError>`, so overflow,
  underflow, division-by-zero, and remainder-by-zero are explicit checked
  results rather than hidden panics.
- The proof phase currently rejects indexing expressions that are not proven
  safe and map indexing whose key is not proven present.

## Patterns

Current patterns are intentionally small:

- wildcard: `_`
- binding: `name`
- integer literal: `0`
- boolean literal: `true`, `false`

Patterns currently appear in:

- `for <pattern> in ...`
- `match <expr> { <pattern> => ... }`

## Expressions

### Primary Expressions

- integer literals
- boolean literals
- names
- tuple literals
- array literals
- block expressions
- parenthesized expressions

Examples:

```langlog
0
true
value
(left, right)
[1, 2, 3, 4]
{ total }
```

### Unary Operators

- `-expr`
- `!expr`

### Binary Operators

Supported binary operators:

- range: `..`
- logical or: `||`
- logical and: `&&`
- equality: `==`, `!=`
- comparisons: `<`, `<=`, `>`, `>=`
- arithmetic: `+`, `-`, `*`, `/`, `%`

Current precedence, lowest to highest:

1. `..`
2. `||`
3. `&&`
4. `==`, `!=`
5. `<`, `<=`, `>`, `>=`
6. `+`, `-`
7. `*`, `/`, `%`
8. unary `-`, `!`
9. postfix call and indexing

### Postfix Expressions

Function call:

```langlog
sum(values)
```

Indexing:

```langlog
values[index]
```

## Playground Host Builtins

The browser playground and Wasm backend expose a small terminal-oriented host
API. These names are reserved and do not need user declarations:

```langlog
read_u32() -> u32
print_u32(value: u32) -> ()
print_bool(value: bool) -> ()
print_newline() -> ()
```

`read_u32` consumes one whitespace-separated unsigned integer token from the
playground stdin field. Invalid or exhausted input is a runtime trap in the
host, not a compile-time diagnostic.

## Blocks

Blocks use Rust-like braces:

```langlog
{
    let value = 1;
    value
}
```

Rules:

- A block contains zero or more statements.
- A block may end with a trailing expression without a semicolon.
- A block is also an expression.

## Types

The parser currently accepts:

- unit: `()`
- named types: `u32`, `bool`, `MyType`
- tuple types: `(u32, bool)`
- fixed arrays: `[u32; 4]`
- applied types with generic arguments:
  - `Option<u32>`
  - `Result<u32, Error>`
  - `Set<u32, 16>`
  - `Map<u32, bool, 32>`

Notes:

- Generic arguments may be either types or integer constants.
- The parser accepts user-written names such as `MyType`; semantic checking
  currently understands `u32`, `bool`, arrays, and the built-in collection
  shells, but richer user-defined type meaning is still ahead of the current
  implementation.
- Capacity-bounded `Set` and `Map` are parsed now because they are central to
  the language design, even though collection semantics are still ahead of the
  current implementation.

## Current Semantic Type Checks

The current semantic checker already enforces these rules:

- `let` annotations, assignment values, returns, and call arguments must match
  declared types and function signatures.
- tuple, `Option`, `Result`, `Set`, and `Map` types participate in those same
  compatibility checks.
- `if` conditions and logical operators must use `bool`.
- arithmetic operators must use `u32` or `Result<u32, ArithmeticError>`, while
  ordering comparisons and range bounds must use `u32`.
- phase 1 rejects bindings and literals whose types would remain unknown after
  checking, including `let` bindings with neither annotation nor initializer
  and empty array literals.
- array literals must be homogeneous, and indexing requires either an array
  target plus a `u32` index or a `Map<K, V, N>` target plus a `K` key.

## Wasm V1 Backend

`langlog build --target wasm <path>` compiles checked programs to WebAssembly.
The backend runs only after syntax, semantic, and proof checks succeed.

Wasm V1 supports:

- `fn main() -> u32`
- flattened non-collection values using `i32` slots:
  - `()`
  - `u32`, `bool`, and `ArithmeticError`
  - tuples
  - fixed-size arrays
  - `Option<T>`
  - `Result<T, E>`
  - `range<u32>`
- local `let`, mutable assignment, expression statements, and block results
- arithmetic, structural equality, ordering comparisons over `u32`, array
  indexing, `if`, `match`, `for` over arrays and ranges, direct calls,
  `observe`, recovery expressions, and `return`
- playground host builtins lowered as `langlog_host` imports

Wasm V1 rejects:

- `Set` and `Map` runtime values, loops, and map indexing; these remain
  check/proof-only until a runtime collection representation is designed
- first-class function values and indirect calls
- assignment targets other than local bindings
- `main` forms other than `fn main() -> u32`

## Diagnostics

`langlog check` reports syntax errors with:

- file path
- line and column
- source line snippet
- primary span underline

Example:

```text
error: expected a parameter name
  --> broken.llg:1:10
  |
1 | fn main( {
  |          ^ identifier expected here
```

The current renderer is intentionally minimal, but the span model is designed to
grow toward richer Rust-style diagnostics.

## Current Limits

These are important current limits, not future promises:

- `if` and `match` are statements, not expressions.
- Field access and method syntax do not exist yet.
- String literals do not exist yet.
- Wasm execution currently covers only the Wasm V1 subset described above.
- Type checking, name resolution, recursion rejection, and proof checking
  exist, but they intentionally cover only the current phase 1 language model.
