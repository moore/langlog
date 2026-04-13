# Langlog Reference Manual

Status: parser reference for the current experimental front end. This document
describes the surface language accepted by `langlog check` today. It does not
promise that every parsed program already has full semantic checking behind it.

See also:

- [TUTORIAL.md](./TUTORIAL.md) for a guided introduction
- [SPEC.md](./SPEC.md) for the broader language design goals

## Files And Compilation

- A phase 1 Langlog program is a single source file.
- The only supported command is `langlog check <path>`.
- The parser accepts one or more top-level function items.
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
`true`, `false`

## Items

The only top-level item is a function:

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

## Statements

The parser currently accepts these statements:

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
- The statement must end with `;`.

### Assignment

```langlog
total = total + value;
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
    observe total < 1000;
} else {
    observe total >= 0;
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
    total = total + value;
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

### `observe`

```langlog
observe total <= 1000;
```

`observe` records an explicit fact in the source program. The parser accepts it
now. Its proof behavior will be implemented later.

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
- The parser accepts user-written names such as `MyType`, but name resolution
  does not exist yet.
- Capacity-bounded `Set` and `Map` are parsed now because they are central to
  the language design, even though collection semantics are still ahead of the
  current implementation.

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
- Assignment targets are not semantically validated yet.
- Type checking, name resolution, recursion rejection, and proof checking are
  still in progress.
