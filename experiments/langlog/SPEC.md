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
- When arithmetic proof analysis fails during `langlog check`, the CLI MUST
  print diagnostics to stderr.
- When `.langlog-config` cannot be read, the CLI MUST print an error to stderr.
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

- The phase 1 keyword set MUST reserve `fn`, `let`, `mut`, `if`, `else`,
  `match`, `for`, `in`, `return`, `observe`, `true`, and `false`.

## LLG-LEX-04 Lexical Error Diagnostics

- Lexical diagnostics for invalid characters MUST include a primary span
  covering the offending character.

## LLG-SYN-01 Top-Level Functions

- A phase 1 source file MUST contain only function items at the top level.
- A non-function top-level item MUST be rejected with a syntax diagnostic.
- A function item MUST use Rust-like syntax with `fn`, a name, a parameter list,
  and a block body.
- The current parser allows the return type to be omitted in phase 1.

## LLG-SYN-02 Statements

- The parser MUST accept `let`, assignment, expression, `if`, `match`, `for`,
  `return`, and `observe` statements.
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

## LLG-DIAG-02 Rendered Syntax Diagnostics

- The CLI MUST render syntax errors with file path, line, column, source line
  text, and an underline spanning the full primary source span.

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
- The semantic phase MUST reject references to undefined bindings.

## LLG-SEMA-02 Totality Constraints

- The semantic phase MUST reject direct recursion.
- The semantic phase MUST reject indirect recursion.
- The semantic phase MUST reject `for` iterables outside the bounded phase 1
  loop model; phase 1 bounded iterables are range expressions, array literals,
  and bindings whose declared types or initializers make them fixed arrays or
  explicit-capacity `Set`/`Map` values.
- The semantic phase MUST require the `else` block of `observe` to be terminal
  so control cannot continue after a failed observation.

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
- The semantic phase MUST require arithmetic operators, ordering comparisons,
  and range bounds to use `u32`.
- The semantic phase MUST reject phase 1 programs whose types would remain
  unknown after checking; this includes `let` bindings without either a type
  annotation or an initializer, and empty array literals without an explicit
  element type.
- The semantic phase MUST require array literals to have a homogeneous element
  type, and MUST require indexing to use an array target plus a `u32` index.
- The semantic phase MUST recognize tuple, `Option`, `Result`, `Set`, and
  `Map` types in bindings, returns, call compatibility, and equality checks.

## LLG-PROOF-01 Proof-Required Operations

- The proof phase MUST reject arithmetic that may overflow unless safety is
  proven.
- The proof phase MUST reject division or remainder operations that may divide
  by zero unless safety is proven.
- The proof phase MUST reject indexing that may go out of bounds unless safety
  is proven.

## LLG-PROOF-02 Observations

- In the current phase, the proof phase MUST derive facts from comparison-based
  control-flow tests.
- The proof phase MUST incorporate explicit `observe` statements into the fact
  model on the continuing path after a guarded `observe` succeeds.
- In phase 1, an `observe` fact MUST relate a left-hand proof expression to a
  right-hand proof expression.
- Control-flow comparisons over mutable bindings MUST be tracked for diagnostics
  but MUST NOT discharge proof obligations.
- Warnings about mutable control-flow facts MUST appear only when such a fact
  would otherwise discharge a real obligation.
- Mutable control-flow facts MUST NOT survive reassignment as if they were
  stable proofs.
- Binding-based proof facts MUST attach to binding identity rather than
  identifier text so shadowing does not inherit outer facts.

## LLG-REL-01 Collections And Relations

- The first enforced relation MUST allow membership in a `Set<K, N>` to imply
  presence in a `Map<K, V, M>`.

## Non-Goals

Phase 1 does not attempt to provide:

- unrestricted Rust compatibility;
- general Turing completeness;
- async or event-loop syntax;
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
- the final async surface syntax for the event-loop runtime.
