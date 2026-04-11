# Langlog Language Specification

Status: draft 0. This document is the working specification for the phase 1 front end. If implementation and this file disagree, update one of them before proceeding.

## Goals

Langlog is a standalone systems language experiment focused on reliability properties that should be enforced structurally rather than by convention:

- execution should be bounded;
- potentially failing operations should be explicit or proven safe;
- resource use should be reasoned about in the language model;
- the eventual runtime should support a single event loop with bounded handlers.

## Non-Goals

Phase 1 does not attempt to provide:

- unrestricted Rust compatibility;
- general Turing completeness;
- recursion;
- implicit panics from arithmetic, indexing, or similar operations;
- async or event-loop syntax;
- modules, traits, generics, or multi-file compilation.

## Compilation Model

The compiler pipeline is:

1. Source text
2. AST
3. HIR
4. Proof-oriented IR
5. MIR
6. LLVM IR

Phase 1 ends after parsing, semantic analysis, and proof checking. Execution is a later milestone.

## Source Model

- A phase 1 program is a single source file.
- The top-level contains function items only.
- The first CLI surface is `langlog check <path>`.
- The language uses Rust-like blocks and statements, but Langlog semantics take precedence over Rust precedent.

## Lexical Model

- Identifiers follow Rust-style ASCII identifier rules for now: leading alphabetic or `_`, followed by alphanumeric or `_`.
- Integer literals are base-10 unsigned integers in phase 1.
- Line comments use `//`.
- Block comments use `/* ... */` and may be nested only if the lexer supports it explicitly.
- The lexer must attach byte spans to every token and preserve enough information for source-linked diagnostics.
- AST nodes should either store their own source span or be able to derive one from spanned children without reparsing.

Reserved keywords for phase 1:

`fn`, `let`, `if`, `else`, `match`, `for`, `in`, `return`, `observe`, `true`, `false`

Reserved type names for phase 1:

`bool`, `u8`, `u16`, `u32`, `u64`, `usize`, `i8`, `i16`, `i32`, `i64`, `isize`, `Option`, `Result`, `Set`, `Map`

## Phase 1 Surface Syntax

### Items

The only top-level item form is a function:

```text
fn name(param1: Type, param2: Type) -> Type {
    ...
}
```

Functions without a meaningful return value should use the unit type `()`.

### Statements

Phase 1 statements are:

- `let` bindings with optional `mut`;
- assignment to a previously declared place;
- expression statements;
- `if` and `if/else`;
- `match`;
- `for`;
- `return`;
- `observe <predicate>;`

`while`, `loop`, and recursion are not part of the language.

### Expressions

Phase 1 expressions include:

- literals;
- variable references;
- tuple expressions;
- array expressions;
- block expressions;
- unary operators;
- binary operators;
- function calls;
- indexing;
- parenthesized expressions.

Field access and method syntax are deferred until there is a concrete need.

### Types

Phase 1 types are:

- unit `()`;
- booleans;
- fixed-width signed and unsigned integers;
- tuples;
- fixed arrays `[T; N]`;
- `Option<T>`;
- `Result<T, E>`;
- `Set<T, N>`;
- `Map<K, V, N>`.

Collection capacities are mandatory syntax. They are part of the type because bounded memory is a language property, not a library convention.

## Control Flow And Boundedness

- Functions must be non-recursive.
- Iteration must be syntactically bounded.
- Phase 1 `for` loops allow only:
  - iteration over fixed arrays; or
  - iteration over bounded integer ranges written as `start..end`.
- `while` and open-ended iterator protocols are rejected.
- The semantic phase is responsible for rejecting direct and indirect recursion.

## Safety And Proof Model

Potentially failing operations are proof-required in phase 1. The compiler must reject the program if it cannot prove the operation is safe.

The initial proof-required operations are:

- integer arithmetic that may overflow;
- division or remainder where the divisor may be zero;
- indexing where the index may be out of bounds.

The checker obtains proof facts from two sources:

- control flow, such as comparisons, range checks, length checks, and membership tests;
- explicit `observe` statements supplied by the programmer.

Example:

```text
observe value_count <= u32::MAX;
```

The exact observation surface may evolve, but explicit observations are part of the language and not just an internal compiler hint.

## Collections And Relations

- `Set<T, N>` and `Map<K, V, N>` are built-in bounded collection types.
- Collection operations may later create proof obligations for capacity and relational safety.
- Phase 1 parsing should reserve enough syntax headroom for declared relations, but relation checking is implemented in milestone M4.
- The first enforced relation remains: membership in a `Set<K, N>` may imply presence in a `Map<K, V, M>`.

## Error Model

- Parser errors must report spans and recover at item and statement boundaries when practical.
- Diagnostics should support primary and secondary span labels so later rendering can approach Rust-style error reporting.
- Semantic and proof errors must point to the source operation that created the obligation.
- The compiler must prefer rejection over silently inserting runtime checks that weaken the language guarantees.

## Open Design Boundaries

These are intentionally not fixed yet:

- the exact syntax for relation declarations;
- whether `Result` error types are closed or user-defined in early phases;
- whether collection insertion is proof-required or explicitly fallible in the first executable runtime;
- the final async surface syntax for the event-loop runtime.
