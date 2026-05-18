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
- The lexer MUST NOT emit tokens for ignored comments.

## LLG-LEX-02 Identifiers And Literals

- Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII
  letters, digits, or `_`.
- Integer literals MUST be parsed as unsigned base-10 integers.
- Boolean literals MUST include `true` and `false`.

## LLG-LEX-03 Reserved Keywords

- The keyword set MUST reserve `fn`, `task`, `let`, `mut`, `if`, `else`,
  `match`, `for`, `in`, `forever`, `return`, `exit`, `delegate`, `observe`,
  `or`, `true`, and `false`.
- The marker-aware language phase MUST recognize `with`, `mark`, `place`,
  `implies`, and `unsafe` in marker syntax positions.
- Marker syntax words MAY remain contextual outside marker syntax positions
  when doing so is unambiguous.

## LLG-LEX-04 Lexical Error Diagnostics

- Lexical diagnostics for invalid characters MUST include a primary span
  covering the offending character.

## LLG-SYN-01 Top-Level Items

- A source file MUST contain only function items, task items, and marker
  companion-rule items at the top level.
- A non-function, non-task, non-marker-rule top-level item MUST be rejected
  with a syntax diagnostic.
- A function item MUST use Rust-like syntax with `fn`, a name, a parameter list,
  and a block body.
- The current parser allows the return type to be omitted in phase 1.
- A task item MUST use the form `task name(param: Type, ...) -> Type { ... }`.
- A task item MUST include an explicit return type.
- A task item MUST be treated as orchestration code rather than an ordinary
  total function.
- An executable task program MUST use `task main() -> u32` as its root task.
- A marker companion-rule item MUST use `mark Name(param: place, ...) { ... }`
  and MUST be proof-only metadata rather than executable code.
- Future root task configuration MAY allow other root task names or signatures.

## LLG-SYN-02 Statements

- The parser MUST accept `let`, assignment, expression, `if`, `match`, `for`,
  `return`, and `observe` statements.
- The parser MUST preserve accepted statement forms and their nested expression
  shapes in the AST.
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

## LLG-MARK-01 Marker Model

The marker-aware language phase replaces ad hoc observations and proof facts
with marker facts attached to places.

- A `place` MUST be a compiler-visible SSA identity that can carry marker
  facts.
- Source locals, state arguments, task fields, projections, and
  compiler-created temporaries MAY lower to places.
- A place MUST NOT be an arbitrary runtime expression. If an expression needs
  to carry marker facts, it MUST first be named or lowered to an SSA temporary.
- A marker fact MUST be compile-time information attached to a place.
- Marker facts MUST NOT add runtime fields and MUST be erased before executable
  lowering after all marker obligations have been checked.
- A marker-qualified type or place requirement MUST use `with`, such as
  `String with Event`.
- Multiple markers MUST use a parenthesized marker list, such as
  `String with (Event, Foo)`.
- The parser MUST preserve marker-qualified types, marker names, and marker
  place arguments in the AST.
- A value with extra marker facts MAY be used where those markers are not
  required.
- A value without a required marker fact MUST NOT be used where that marker is
  required.

## LLG-MARK-02 Function Boundaries

- Function arguments MAY elide unmentioned markers, because ignoring marker
  facts is safe.
- A marker-qualified function parameter MUST create a call-site obligation for
  each required marker on the corresponding argument.
- Function return values MUST carry only the marker facts named by the function
  signature.
- A marker-qualified return type MUST require the returned expression to
  provide each named marker and MUST provide those markers to callers after the
  call succeeds.
- A generic type parameter MUST NOT capture unmentioned marker facts from an
  argument.
- If a function preserves or creates a marker fact across the call boundary,
  the marker MUST appear explicitly in the return type.

For example:

```llg
fn len(input: String) -> u32;

let line: String with Event = stdin.read();
let length = len(line);
```

The call to `len` is accepted because `Event` is elided at the argument
boundary. By contrast:

```llg
fn trim(input: String) -> String;

let line: String with Event = stdin.read();
let trimmed = trim(line); // String, not String with Event
```

The result does not keep `Event` unless `trim` explicitly declares that marker
in its return type.

## LLG-MARK-03 Marker Construction

- Safe code MAY consume and require marker facts.
- Code that creates a marker fact MUST do so inside an `unsafe` block.
- Marker constructor syntax outside `unsafe` MUST be rejected with a syntax
  diagnostic.
- Unsafe marker construction MUST assert that the marker contract is true for
  the marked place.
- Compiler-derived marker facts MAY still be created by built-in control-flow
  and companion marker rules specified by this document.

For example:

```llg
unsafe {
    Event::mark(value)
}
```

## LLG-MARK-04 Builtin Marker Families

- `True()` MUST mark a boolean result place that is known to be true.
- `False()` MUST mark a boolean result place that is known to be false.
- `Equal(left, right)` MUST mark that `left` is equal to `right`.
- `LessThan(left, right)` MUST mark that `left` is less than `right`.
- `GreaterThan(left, right)` MUST mark that `left` is greater than `right`.
- `LessOrEqual(left, right)` MUST mark that `left` is less than or equal to
  `right`.
- `GreaterOrEqual(left, right)` MUST mark that `left` is greater than or equal
  to `right`.
- `MemberOf(key, map)` MUST mark that `key` is known to be present in `map`.
- `Event` MUST mark a value that represents fresh external input or a fresh
  externally scheduled occurrence.
- The trusted `read_u32()` host builtin MUST return a value marked with
  `Event`.

Full user-defined marker-family declaration syntax is deferred. The first
marker-aware phase defines builtin marker families and builtin companion marker
rules.

## LLG-MARK-05 Marker Transfer

- Assignment MUST preserve marker facts because it preserves place identity.
- Mutating a value MUST create a new place for the new SSA version.
- Ordinary marker facts attached to the old place MUST NOT automatically apply
  to the new place.
- Immutable marker facts MAY be copied from the old place to the new place when
  they depend only on a stable public facet that the mutation cannot change.
- A fixed-array length is a stable public facet. An index proven less than a
  fixed-array length MAY remain proven after an element update.
- A collection length that can change through mutation MUST NOT be treated as a
  stable facet for this purpose.

## LLG-MARK-06 Companion Marker Rules

Each syntax operator MAY have a companion marker rule that describes marker
facts produced by that operator. Companion marker rules MUST use `mark`, `place`,
`implies`, and marker-pattern bindings such as `?bound`.

This marker slice MUST accept only builtin comparison companion rule names.
Companion marker rules MUST reject unknown marker families in refinements and
implications.
Companion marker rules MUST lower refinement-pattern bindings and implications
into Proof IR marker-rule templates.
Control-flow comparison marker facts MUST be emitted as companion-rule
implications.

Marker-rule conditions of the form `a with Marker(...)` MUST be marker
refinement patterns. The condition succeeds only if the current marker
environment already contains a matching marker attached to `a`; it MUST NOT
create the marker.

Marker refinement patterns MUST be accepted only inside marker companion-rule
bodies. The same `place with Marker(...)` spelling in ordinary function or task
code MUST be rejected because it has no runtime value.

Marker-pattern bindings MUST use `?name` at the binding site. In
`a with LessThan(a, ?bound)`, the compiler searches for an existing marker
attached to `a` with shape `LessThan(a, X)` and binds `bound` to the matched
place `X` inside the block.

For boolean operators, the result is also a place. Control flow MUST mark the
condition result with `True()` in the then branch and `False()` in the else
branch. The operator's companion marker rule translates those truth markers
into relational marker facts:

```llg
mark LessThan(a: place, b: place, result: place) {
    if result with True() {
        implies LessThan(a, b) for a;
        implies GreaterThan(b, a) for b;
    }

    if result with False() {
        implies GreaterOrEqual(a, b) for a;
        implies LessOrEqual(b, a) for b;
    }
}
```

For value-producing operators, implications apply to the successful result of
the operation:

```llg
mark Sub(a: place, amount: place, result: place) {
    if a with LessThan(a, ?bound) {
        implies LessThan(result, bound) for result;
    }
}
```

If no companion marker rule applies, the operation MUST NOT preserve the input
marker facts onto the result.

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

## LLG-DIAG-04 Marker Obligation Diagnostics

- A failed marker obligation diagnostic MUST identify the operation that
  created the obligation.
- A failed marker obligation diagnostic MUST display the required marker
  pattern.
- A failed place-specific marker obligation diagnostic MUST identify the target
  place that needed the marker.
- When useful for a place-specific obligation, a failed marker obligation
  diagnostic SHOULD display marker facts currently known for the target place.
- When useful, a failed marker obligation diagnostic SHOULD display near-miss
  marker facts that have the right marker family but refer to different places.
- When a direct guard or checked operation can produce the missing marker, the
  diagnostic SHOULD suggest that source pattern.
- Diagnostics MUST NOT imply that Langlog performs arbitrary constraint solving.

Example for a missing array-bound marker:

```text
cannot index `array` with `index`

required marker:
    index with LessThan(index, array.length)

known markers on `index`:
    none

help: add a guard that proves the bound:
    if index < array.length { ... }
```

Example for a near miss:

```text
cannot index `array` with `index`

required marker:
    index with LessThan(index, array.length)

found:
    index with LessThan(index, limit)

Langlog does not infer that `limit <= array.length`.
Add a direct guard or checked operation that produces the required marker.
```

Example for a stale SSA marker:

```text
marker applies to an older version of `users`

required:
    key with MemberOf(key, users1)

found:
    key with MemberOf(key, users0)

`users.remove(id)` created a new version of `users`.
Re-check membership after the mutation.
```

Example for a missing event in a future explicit state cycle:

```text
cycle in task `server` can repeat without receiving an Event

cycle:
    poll -> poll

required:
    some state body in the cycle must introduce a value with Event

help: add a receive, timer, or unsafe Event::mark-backed operation in the cycle
```

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
- The semantic phase MUST allow `observe` proof expressions to reference
  mutable bindings; marker validity is enforced by SSA place versioning.

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
- A task instance MUST be representable as a finite tagged union of task
  states.
- Each task-state variant MUST represent one task item in the reachable
  delegation set for that task instance.
- At runtime, exactly one task-state variant MUST be active per task instance.
- A `delegate` statement MUST evaluate its arguments before replacing the
  current task state with the target task-state variant.
- A `delegate` statement MUST discard the caller task-local state and MUST NOT
  create or retain a task stack frame.
- Cyclic task delegation MUST be accepted when every delegate in the cycle
  otherwise type-checks, because repeated delegation is a bounded state
  transition rather than stack growth.
- The static memory bound for task-local state MUST be the maximum storage
  required by any reachable task-state variant plus tag overhead.
- An `exit` statement MUST type check its expression against the enclosing task
  return type.
- An `exit` statement MUST exit the program with the checked value.
- A task body MUST NOT fall through accidentally. Every reachable task control
  path MUST end in an `exit` statement, a same-return-type `delegate`
  statement, or a non-nested `forever` statement.
- A bare `forever { ... }` task body MUST be accepted as a valid crash-only or
  externally terminated task shape.

The task memory model specifies required behavior, not an exact ABI layout.
Large external resources such as buffers should be represented in task state by
handles or leases rather than forced inline by this memory model.

## LLG-PROOF-01 Marker-Required Operations

- The marker-aware proof phase MUST reject an operation whose marker obligation
  is not discharged.
- Marker checking MUST traverse task bodies, including `forever` bodies, `exit`
  values, and `delegate` arguments.
- Array indexing MUST require a marker obligation equivalent to
  `index with LessThan(index, array.length)`.
- Map indexing or map-presence-sensitive lookup MUST require a marker
  obligation equivalent to `key with MemberOf(key, map)`.
- A place-specific marker obligation MUST name the required marker pattern, the
  target place, and the source operation span.

## LLG-PROOF-02 Marker Introduction And Discharge

- Marker obligations MUST be discharged only by a direct marker match, possibly
  after applying declared companion marker transfer rules.
- The marker checker MUST NOT perform arbitrary algebra, transitive relation
  solving, backtracking proof search, or global constraint solving.
- If no direct marker match exists, the compiler MUST reject the operation and
  explain which marker is missing.
- Control-flow conditions MUST introduce `True()` for the condition result in
  the then branch and `False()` for the condition result in the else branch.
- Successful `observe` statements MUST introduce `True()` for the observed
  condition result on the continuing path.
- Companion marker rules MAY translate `True()` and `False()` facts into
  relation markers such as `LessThan(index, array.length)`.
- Marker facts MUST remain scoped to the control-flow region in which they are
  known.
- Marker facts MUST attach to places rather than identifier text, so shadowing
  does not inherit outer marker facts.
- Marker checking MUST inspect obligations inside `else` branches.
- Marker facts MUST be available for bindings introduced inside `else`
  branches, loop patterns, match patterns, and expression blocks.

## LLG-PROOF-03 Event Productivity

- `Event` MUST be the marker used to represent fresh external input or a fresh
  externally scheduled occurrence.
- In a future explicit-state task model, every cyclic path through `go`
  transitions MUST introduce an `Event` marker during execution of some state
  body in the cycle.
- An `Event` marker carried into a task argument or state argument MUST NOT by
  itself satisfy the cycle obligation, because the event did not happen inside
  the cycle.
- This rule does not rewrite the current `forever`/`delegate` task syntax.

## LLG-REL-01 Collections And Relations

- The first enforced collection relation MUST be expressed as a marker transfer
  rule.
- A key introduced by iterating a `Set<K, N>` MAY imply a `MemberOf(key, map)`
  marker for a related `Map<K, V, M>` only when the relation has been declared
  by the language or a trusted builtin rule.

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

These items are intentionally left open while the front end and marker-aware
proof model evolve:

- the exact syntax for declared collection relations;
- full user-defined marker-family declarations beyond the first builtin marker
  families and companion rules;
- whether `Result` error types are closed or user-defined in early phases;
- whether collection insertion is proof-required or explicitly fallible in the
  first executable runtime;
- root task configuration beyond the initial `task main() -> u32` executable
  entrypoint;
- possible future use of `delegate` for explicit tail calls to ordinary
  functions;
- the final async, I/O program, and handler surface syntax for the event-loop
  runtime.
