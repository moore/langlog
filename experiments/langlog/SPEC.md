# Langlog Language Specification

Status: draft 1. This document is the normative language and front-end
specification for the current Langlog experiment. Normative terms in this
document follow RFC 2119.

User-facing docs:

- [REFERENCE.md](./REFERENCE.md)
- [TUTORIAL.md](./TUTORIAL.md)
- [TRACEABILITY.md](./TRACEABILITY.md)

## Goals

Langlog is a standalone systems language experiment focused on reliability
properties that should be enforced structurally rather than by convention:

- execution should be bounded;
- potentially failing operations should be explicit or proven safe;
- resource use should be reasoned about in the language model;
- the eventual runtime should support a single event loop with bounded handlers.

## LLG-CLI-01 Single-File Front End

- The phase 1 front end MUST accept `langlog check <path>`.
- The phase 1 front end MUST treat `<path>` as a single source file.

## LLG-LEX-01 Comments And Token Spans

- The lexer MUST ignore line comments beginning with `//`.
- The lexer MUST ignore block comments delimited by `/*` and `*/`.
- The lexer MUST support nested block comments.
- The lexer MUST report an error for an unterminated block comment.
- The lexer MUST attach a byte span to every emitted token.

## LLG-LEX-02 Identifiers And Literals

- Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII
  letters, digits, or `_`.
- Integer literals MUST be parsed as unsigned base-10 integers.
- Boolean literals MUST include `true` and `false`.

## LLG-LEX-03 Reserved Keywords

- The phase 1 keyword set MUST reserve `fn`, `let`, `mut`, `if`, `else`,
  `match`, `for`, `in`, `return`, `observe`, `true`, and `false`.

## LLG-SYN-01 Top-Level Functions

- A phase 1 source file MUST contain only function items at the top level.
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
- Postfix call and indexing MUST bind tighter than unary operators.
- Unary operators MUST bind tighter than multiplicative operators.
- Multiplicative operators MUST bind tighter than additive operators.
- Additive operators MUST bind tighter than comparison operators.
- Comparison operators MUST bind tighter than equality operators.
- Equality operators MUST bind tighter than logical and.
- Logical and MUST bind tighter than logical or.
- Logical or MUST bind tighter than range construction.

## LLG-TYPE-01 Phase 1 Types

- The parser MUST accept unit, named, tuple, fixed-array, and generic
  application type forms.
- A fixed-array type MUST use the form `[T; N]`.
- `Set<T, N>` and `Map<K, V, N>` MUST carry explicit capacity arguments in the
  source type.

## LLG-DIAG-01 Source Spans And Syntax Diagnostics

- The front end MUST preserve byte spans for tokens and syntax nodes or derive
  them from spanned children without reparsing source text.
- Syntax diagnostics MUST include a primary source span.
- The CLI MUST render syntax errors with file path, line, column, source line
  text, and an underline for the primary span.

## LLG-SEMA-01 Name Resolution And Scopes

- The semantic phase MUST resolve item, parameter, and local bindings according
  to lexical scope.
- The semantic phase MUST reject references to undefined bindings.

## LLG-SEMA-02 Totality Constraints

- The semantic phase MUST reject direct recursion.
- The semantic phase MUST reject indirect recursion.
- The semantic phase MUST reject unbounded iteration forms that are outside the
  bounded phase 1 loop model.

## LLG-PROOF-01 Proof-Required Operations

- The proof phase MUST reject arithmetic that may overflow unless safety is
  proven.
- The proof phase MUST reject division or remainder operations that may divide
  by zero unless safety is proven.
- The proof phase MUST reject indexing that may go out of bounds unless safety
  is proven.

## LLG-PROOF-02 Observations

- The proof phase MUST derive facts from control-flow tests such as comparisons,
  range checks, length checks, and membership tests.
- The proof phase MUST incorporate explicit `observe` statements into the fact
  model.

## LLG-REL-01 Collections And Relations

- The language MUST parse capacity-bounded `Set<T, N>` and `Map<K, V, N>` types.
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
