# Langlog Language Specification

Status: draft 1. This document is the normative language and front-end
specification for the current Langlog experiment. Normative terms in this
document follow RFC 2119.

User-facing docs:

- [REFERENCE.md](./REFERENCE.md)
- [TUTORIAL.md](./TUTORIAL.md)
- [TRACEABILITY.md](./TRACEABILITY.md)

Compiler-facing docs:

- [HIR.md](./HIR.md)
- [SEMANTICS.md](./SEMANTICS.md)
- [PROOF_IR.md](./PROOF_IR.md)
- [WASM.md](./WASM.md)
- [TOOLS.md](./TOOLS.md)

## Goals

Langlog is a standalone systems language experiment focused on reliability
properties that should be enforced structurally rather than by convention:

- execution should be bounded;
- potentially failing operations should be explicit or proven safe;
- resource use should be reasoned about in the language model;
- the eventual runtime should support a single event loop with bounded handlers.

## LLG-CLI-01 Single-File Front End

- The phase 1 front end MUST accept `langlog check <path>`.
- The phase 1 front end MUST accept `langlog check --warnings-as-errors <path>`.
- The phase 1 front end MUST accept `langlog build --target wasm <path>`.
- The phase 1 front end MUST check in-memory source text without filesystem
  access.
- The phase 1 front end MUST use `.langlog-config` build settings when
  building source files below that config file.
- The phase 1 front end MUST treat `<path>` as a single source file.

## LLG-CLI-02 CLI Output Behavior

- When `langlog check <path>` succeeds, the CLI MUST print a success summary to
  stdout.
- When `langlog build --target wasm <path>` succeeds, the CLI MUST print the
  output artifact path to stdout.
- The CLI MUST reject unsupported build targets as usage errors.
- The CLI MUST reject malformed build target flags as usage errors.
- When a successful check includes warnings, the CLI MUST print the warnings
  to stderr while keeping the success summary on stdout.
- When syntax analysis fails, the CLI MUST print diagnostics to stderr.
- When semantic analysis fails, the CLI MUST print diagnostics to stderr.
- When proof analysis fails during `langlog check`, the CLI MUST print
  diagnostics to stderr.
- When `.langlog-config` cannot be read, the CLI MUST print an error to stderr.
- Malformed entries in a `.langlog-config` `[build]` section MUST make build
  fail with a config error on stderr.
- `langlog check --warnings-as-errors <path>` MUST succeed when no warnings are
  emitted.
- The compiler interface MUST promote warnings to failing diagnostics when
  requested.
- Success and syntax-error reporting MUST not write to the opposite stream.
- `langlog check --warnings-as-errors <path>` MUST promote warnings to failing
  diagnostics.

## LLG-LEX-01 Comments

- The lexer MUST ignore line comments beginning with `//`.
- The lexer MUST ignore block comments delimited by `/*` and `*/`.
- The lexer MUST support nested block comments.
- The lexer MUST report an error for an unterminated block comment.

## LLG-LEX-02 Identifiers And Literals

- Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII
  letters, digits, or `_`.
- Integer literals MUST be parsed as unsigned base-10 integers.
- Boolean literals MUST include `true` and `false`.

## LLG-LEX-03 Reserved Keywords

- The keyword set MUST reserve `fn`, `task`, `let`, `mut`, `if`, `else`,
  `match`, `for`, `in`, `forever`, `return`, `exit`, `delegate`, `observe`,
  `or`, `true`, and `false`.

## LLG-LEX-04 Lexical Error Diagnostics

- Lexical diagnostics for invalid characters MUST include a primary span
  covering the offending character.

## LLG-SYN-01 Top-Level Items

- A phase 1 source file MUST contain only function items and task items at the
  top level.
- A non-function, non-task top-level item MUST be rejected with a syntax
  diagnostic.
- A function item MUST use Rust-like syntax with `fn`, a name, a parameter list,
  and a block body.
- The current parser allows the return type to be omitted in phase 1.
- A task item MUST use the form `task name(param: Type, ...) -> Type { ... }`.
- A task item MUST include an explicit return type.
- A task item MUST be treated as orchestration code rather than an ordinary
  total function.
- An executable task program MUST use `task main() -> u32` as its root task.
- Future root task configuration MAY allow other root task names or signatures.

## LLG-SYN-02 Statements

- The parser MUST accept `let`, assignment, expression, `if`, `match`, `for`,
  `return`, and `observe` statements.
- The task-orchestration parser MUST additionally accept `forever`, `exit`, and
  `delegate` statements.
- The current parser allows a `let` statement to include `mut`, a type
  annotation, and an initializer.
- A statement form that requires a semicolon MUST reject the form if the
  semicolon is absent.

## LLG-SYN-03 Expressions And Precedence

- The parser MUST accept integer literals, boolean literals, names, tuples,
  arrays, blocks, grouped expressions, unary operators, binary operators, calls,
  and indexing expressions.
- The supported binary operators MUST include `..`, `||`, `&&`, `==`, `!=`,
  `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/`, and `%`.
- The AST for a binary expression MUST group operands according to the
  specified operator precedence and associativity rules.
- Binary operators with the same precedence MUST associate to the left.
- Postfix call and indexing MUST bind tighter than unary operators.
- Unary operators MUST bind tighter than multiplicative operators.
- Multiplicative operators MUST bind tighter than additive operators.
- Additive operators MUST bind tighter than comparison operators.
- Comparison operators MUST bind tighter than equality operators.
- Equality operators MUST bind tighter than logical and.
- Logical and MUST bind tighter than logical or.
- Logical or MUST bind tighter than range construction.
- Recovery expressions MUST parse `expr or fallback` and `expr or(err)
  fallback`, and recovery MUST bind looser than range construction.

## LLG-SYN-04 Grouped And Tuple Expressions

- `()` MUST parse as an empty tuple expression.
- `(expr)` MUST parse as a grouped expression.
- `(expr,)` MUST parse as a single-element tuple expression.
- `(a, b, ...)` MUST parse as a tuple expression.

## LLG-SYN-05 Patterns And Match Arms

- The parser MUST accept wildcard, binding, integer literal, and boolean
  patterns.
- `match` arms MUST use `pattern => body`.
- `match` arms MUST be comma-separated and MAY end with a trailing comma.

## LLG-SYN-06 Observe Statements

- `observe` statements MUST use the form
  `observe <expr> <op> <expr> else <block>`.
- An `observe` statement without an `else` block MUST be rejected with a syntax
  diagnostic.
- The left-hand side of `observe` MUST accept the same phase 1 proof
  expression forms as the right-hand side.
- The phase 1 `observe` operator set MUST include `==`, `!=`, `<`, `<=`, `>`,
  and `>=`.
- In phase 1, `observe` proof expressions MUST reject tuple, array, block,
  range, logical, equality, and comparison subexpressions.
- In phase 1, `observe` proof expressions MUST reject non-proof call callees,
  call arguments, index targets, and index values.

## LLG-SYN-07 Task Orchestration Statements

- A `forever` statement MUST use the form `forever { ... }`.
- An `exit` statement MUST use the form `exit <expr>;`.
- A `delegate` statement MUST use the form `delegate name(args...);`.

## LLG-TYPE-01 Phase 1 Types

- The parser MUST accept unit, named, tuple, fixed-array, and generic
  application type forms.
- A fixed-array type MUST use the form `[T; N]`.

## LLG-TYPE-02 Grouped And Tuple Types

- `()` MUST parse as the unit type.
- `(T)` MUST parse as a grouped type and MUST NOT create a tuple type.
- `(T,)` MUST parse as a single-element tuple type.
- `(A, B, ...)` MUST parse as a tuple type.

## LLG-TYPE-03 Bounded Collection Type Arity

- `Set<T, N>` MUST require exactly one element type and one explicit capacity.
- `Map<K, V, N>` MUST require exactly one key type, one value type, and one
  explicit capacity.

## LLG-DIAG-01 Source Span Preservation

- The front end MUST preserve byte spans for tokens and syntax nodes.
- Syntax diagnostics MUST include a primary source span.
- Source files MUST preserve configured file identifiers, line counts, line
  text, and byte-offset to line/column locations.
- Source files MUST extract source text and line spans from valid same-file
  spans.
- Span and source length helpers MUST report exact byte lengths and emptiness.
- Source files MUST reject foreign spans, out-of-bounds locations, and
  locations that do not land on UTF-8 character boundaries.
- Source line helpers MUST trim CRLF line endings without trimming source
  content before the line ending.
- Empty source files MUST still expose one empty first line.

## LLG-DIAG-02 Rendered Syntax Diagnostics

- The CLI MUST render syntax errors with file path, line, column, source line
  text, and an underline spanning the full primary source span.
- Token descriptions used in diagnostics MUST name identifiers, integer
  literals, and keywords.
- Token descriptions used in diagnostics MUST name punctuation, operators, and
  end of file.

## LLG-DIAG-03 Parser Recovery

- Parser recovery MUST preserve following valid top-level items after malformed
  top-level input.
- Parser recovery MUST preserve following valid statements after a malformed
  statement.
- Parser recovery MUST preserve following valid statements after a malformed
  nested expression.
- Parser recovery MUST preserve following match arms after a malformed match
  arm.
- A missing semicolon before `}` MUST not cascade into additional syntax errors
  for the same statement.

## LLG-SEMA-01 Name Resolution And Scopes

- The semantic phase MUST resolve item, parameter, and local bindings according
  to lexical scope.
- Bindings introduced by block, loop, and match scopes MUST NOT be visible
  after those scopes end.
- Type information for block-scoped bindings MUST NOT be visible after the
  block scope ends.
- The semantic phase MUST reject references to undefined bindings.

## LLG-SEMA-02 Totality Constraints

- The semantic phase MUST reject direct recursion.
- The semantic phase MUST reject indirect recursion.
- The semantic phase MUST reject `for` iterables outside the bounded phase 1
  loop model; phase 1 bounded iterables are range expressions, array literals,
  and bindings whose declared types or initializers make them fixed arrays or
  explicit-capacity `Set`/`Map` values.
- Binary expressions are bounded iterables only when they are range
  expressions.
- Scalar declared types MUST NOT be accepted as bounded iterables.
- The semantic phase MUST require the `else` block of `observe` to be terminal
  so control cannot continue after a failed observation.
- Terminal `observe` else-blocks MAY terminate through nested `if` and `match`
  statements only when every branch terminates.

## LLG-SEMA-03 Mutability And Stable Facts

- The semantic phase MUST reject assignment to immutable bindings.
- In phase 1, the semantic phase MUST reject `observe` proof expressions that
  directly reference mutable bindings.

## LLG-SEMA-04 Initial Type Checking

- The semantic phase MUST reject `let` annotations, assignments, returns, and
  call arguments whose types do not match declared annotations or function
  signatures.
- The semantic phase MUST require `if` conditions and logical operators to use
  `bool`.
- The semantic phase MUST require arithmetic operands to use `u32` or
  `Result<u32, ArithmeticError>`, and ordering comparisons and range bounds to
  use `u32`.
- The semantic phase MUST reject phase 1 programs whose types would remain
  unknown after checking; this includes `let` bindings without either a type
  annotation or an initializer, and empty array literals without an explicit
  element type.
- Unknown types inside compound expressions and compound types MUST prevent
  successful HIR lowering.
- Type compatibility checks MUST NOT emit mismatch diagnostics when either side
  is already unknown.
- The semantic phase MUST require array literals to have a homogeneous element
  type, and MUST require indexing to use either an array target plus a `u32`
  index or a `Map<K, V, N>` target plus a `K` key.
- The semantic phase MUST recognize tuple, `Option`, `Result`, `Set`, and
  `Map` types in bindings, returns, call compatibility, and equality checks.
- The semantic phase MUST reject calls to non-function values and calls with
  the wrong number of arguments.
- The semantic phase MUST require `observe` equality operands to have matching
  types and ordering operands to use `u32`.

## LLG-SEMA-05 Task Orchestration Semantics

- The semantic phase MUST reject ordinary functions that call task items.
- A `forever` statement MUST appear only inside a task body.
- A nested `forever` statement MUST be rejected.
- An `exit` statement MUST appear only inside a task body.
- A `delegate` statement MUST appear only inside a task body.
- A `return` statement MUST be rejected inside a task body.
- A task body MAY call ordinary functions, and ordinary function calls from a
  task body MUST return to the task normally.
- A task body MAY transfer to another task only with a terminal `delegate`
  statement.
- A `delegate` statement MUST target a task item.
- In the initial task-orchestration surface, `delegate` MUST NOT target an
  ordinary function.
- A `delegate` statement MUST NOT return to the current task.
- A `delegate` statement MUST have a callee return type that exactly matches the
  caller task return type.
- A task item MUST NOT be callable through ordinary call expression syntax,
  including as a subexpression, initializer, call argument, expression
  statement, or any other non-`delegate` expression.
- Cyclic task delegation MUST be rejected.
- An `exit` statement MUST type check its expression against the enclosing task
  return type.
- An `exit` statement MUST exit the program with the checked value.
- A task body MUST NOT fall through accidentally. Every reachable task control
  path MUST end in an `exit` statement, a same-return-type `delegate`
  statement, or a non-nested `forever` statement.
- A bare `forever { ... }` task body MUST be accepted as a valid crash-only or
  externally terminated task shape.

## LLG-PROOF-01 Proof-Required Operations

- The proof phase MUST reject indexing that may go out of bounds unless safety
  is proven.
- Proof checking MUST traverse task bodies, including `forever` bodies, `exit`
  values, and `delegate` arguments.
- Indexing MUST require the proven index upper bound to be strictly less than
  the indexed array length.

## LLG-PROOF-02 Observations

- In the current phase, the proof phase MUST derive facts from comparison-based
  control-flow tests.
- The proof phase MUST incorporate explicit `observe` statements into the fact
  model on the continuing path after a guarded `observe` succeeds.
- In phase 1, an `observe` fact MUST relate a left-hand proof expression to a
  right-hand proof expression.
- Control-flow equality and inequality comparisons MUST be available as proof
  facts inside the guarded branch.
- Control-flow comparisons over mutable bindings MUST be tracked for diagnostics
  but MUST NOT discharge proof obligations.
- Warnings about mutable control-flow facts MUST appear only when such a fact
  would otherwise discharge a real obligation.
- A mutable control-flow warning MUST be reported when mutable facts would
  discharge a proof obligation.
- Redundant mutable control-flow hints MUST NOT produce extra warnings for an
  obligation that is already explained by another mutable hint.
- Mutable control-flow facts MUST NOT survive reassignment as if they were
  stable proofs.
- Proof checking MUST inspect obligations inside `else` branches.
- Proof facts MUST be available for bindings introduced inside `else` branches,
  loop patterns, match patterns, and expression blocks.
- Binding-based proof facts MUST attach to binding identity rather than
  identifier text so shadowing does not inherit outer facts.

## LLG-REL-01 Collections And Relations

- The first enforced relation MUST allow a key introduced by iterating a
  `Set<K, N>` to imply presence in a `Map<K, V, M>`.

## Non-Goals

Phase 1 does not attempt to provide:

- unrestricted Rust compatibility;
- general Turing completeness;
- async or I/O handler syntax beyond the `task`/`forever`/`exit`/`delegate`
  orchestration surface;
- modules, traits, generics beyond the current parser surface, or multi-file
  compilation;
- implicit panics from arithmetic, indexing, or similar operations.

## Open Design Boundaries

These items are intentionally left open while the front end and proof model
evolve:

- the exact syntax for declared collection relations;
- whether `Result` error types are closed or user-defined in early phases;
- whether collection insertion is proof-required or explicitly fallible in the
  first executable runtime;
- root task configuration beyond the initial `task main() -> u32` executable
  entrypoint;
- possible future use of `delegate` for explicit tail calls to ordinary
  functions;
- the final async, I/O program, and handler surface syntax for the event-loop
  runtime.
