# Langlog Tutorial

This tutorial introduces the current Langlog prototype as it exists today: a
single-file language experiment with syntax diagnostics, semantic checks, proof
checks, a Wasm V1 backend, and a browser playground.

Use these docs together:

- [REFERENCE.md](./REFERENCE.md) for exact syntax
- [examples/tutorial.llg](./examples/tutorial.llg) for a complete sample file

## 1. Check A File

The first useful workflow is syntax checking:

```text
cargo run -p langlog-driver --bin langlog -- check examples/tutorial.llg
```

If the file parses, the driver reports how many top-level items were checked. If
the file is malformed, it prints a labeled source error with line and column
information.

If you want warnings to fail the check, use:

```text
cargo run -p langlog-driver --bin langlog -- check --warnings-as-errors examples/tutorial.llg
```

## 2. Use The Playground

The browser playground lets you edit one Langlog source file, check it, build
it to Wasm, and run the exported `main` function.

- Use **Check** to see syntax, semantic, and proof diagnostics.
- Use **Build** to generate Wasm and inspect the WAT output.
- Use **Run** to instantiate the generated Wasm and call `main`.
- The terminal stdin field is whitespace-separated `u32` input for
  `read_u32()`.

The smallest runnable program is:

```langlog
fn main() -> u32 {
    42
}
```

Playground terminal I/O uses host builtins:

```langlog
fn main() -> u32 {
    let value: u32 = read_u32();
    print_u32(value);
    print_newline();
    value
}
```

## 3. Build To Wasm

From the command line, build the current Wasm V1 subset with:

```text
cargo run -p langlog-driver --bin langlog -- build --target wasm path/to/main.llg
```

Wasm V1 currently supports `fn main() -> u32`, flattened non-collection values
such as tuples, arrays, `Option<T>`, `Result<T, E>`, and `range<u32>`, plus
locals, assignment, checked arithmetic, comparisons, structural equality,
indexing, `if`, `match`, `for` over arrays and ranges, direct calls, recovery
expressions, `observe`, `return`, and the playground host builtins. It still
rejects Set/Map runtime values, first-class function values, indirect calls,
non-local assignment targets, and non-`u32` `main`.

## 4. Write Your First Function

Every current Langlog file is a list of functions:

```langlog
fn add_one(value: u32) -> u32 {
    value + 1 or(err) value
}
```

Important details:

- Parameters use `name: Type`.
- Return types use `-> Type`.
- The last expression in a block can be returned implicitly by leaving off the
  semicolon.

Even though Langlog is inspired by Rust syntax, it is a separate language
experiment. The goal is not Rust compatibility. The goal is a smaller language
that can eventually prove stronger reliability properties.

## 5. Bind Values With `let`

Use `let` for local bindings:

```langlog
fn start() -> u32 {
    let value: u32 = 1;
    let mut total = value;
    total
}
```

Current parser rules:

- Type annotations are optional.
- Initializers are optional.
- `mut` is optional.

Successful semantic checking still requires enough information to determine the
binding type, so phase 1 accepts either an annotation or an initializer but not
neither.

Semantic checking now enforces mutability:

- assignment is rejected unless the target binding was declared `mut`
- in phase 1, `observe` proof expressions may not directly reference `mut`
  bindings

The current semantic checker also enforces these initial type rules:

- `let` annotations, assignment values, returns, and call arguments must match
  declared types and function signatures
- tuple, `Option`, `Result`, `Set`, and `Map` types participate in those same
  compatibility checks
- `if` conditions and logical operators must use `bool`
- arithmetic operators must use `u32` or `Result<u32, ArithmeticError>` and
  produce checked `Result` values; ordering comparisons and range bounds must
  use `u32`
- phase 1 rejects bindings and literals whose types would remain unknown after
  checking, including `let` bindings with neither annotation nor initializer
  and empty array literals
- array literals must be homogeneous, and indexing requires an array target
  plus a `u32` index

## 6. Use Arrays And Loops

Arrays are written with square brackets:

```langlog
let values: [u32; 4] = [1, 2, 3, 4];
```

You can iterate with `for`:

```langlog
fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;

    for value in values {
        total = total + value or(err) 0;
    }

    total
}
```

Right now, the parser accepts any expression after `in`. The long-term language
design is stricter: loops have to be bounded. The parser still accepts any
expression after `in`, but semantic checking now rejects loop iterables outside
the phase 1 bounded model.

## 7. State Facts With `observe`

One of Langlog’s core ideas is that programs should be able to state facts the
proof engine can use later. The syntax for that is `observe`:

```langlog
fn bounded(total: u32, limit: u32) -> u32 {
    observe total <= limit else {
        return total;
    }

    total + 1 or(err) total
}
```

Today:

- `observe` parses as `observe <expr> <op> <expr> else <block>`
- the `else` block is mandatory
- both sides must be phase 1 proof expressions
- the supported operators are `==`, `!=`, `<`, `<=`, `>`, and `>=`
- tuple, array, block, range, logical, equality, and comparison
  subexpressions are rejected inside phase 1 proof expressions
- non-proof call callees, call arguments, index targets, and index values are
  rejected inside phase 1 proof expressions
- it appears in the AST
- semantic checking rejects proof expressions that directly reference `mut`
  bindings
- semantic checking requires the `else` block to be terminal
- when the observed relation is true, the proof phase records the relational
  fact from the statement
- the proof phase also records simple comparison facts from `if` conditions
- those `if`-derived facts discharge obligations only for stable non-`mut`
  bindings
- comparisons over `mut` bindings are retained only for diagnostics and can
  warn when an obligation would otherwise rely on them
- the proof phase now rejects arithmetic overflow, division or remainder by
  zero, and out-of-bounds indexing when safety is not proven

`observe` already helps discharge overflow, divide-by-zero, and bounds
obligations in phase 1.

## 8. Branch With `if`

`if` uses Rust-like syntax:

```langlog
fn clamp_flag(total: u32) -> u32 {
    if total > 100 {
        observe total < 1001 else {
            return total;
        }
    } else {
        observe total <= 100 else {
            return total;
        }
    }

    total
}
```

In the current front end, `if` is parsed as a statement. It is not yet an
expression form. Comparison-based `if` facts can help discharge proof
obligations, but only when the referenced bindings are stable rather than
`mut`.

## 9. Use `match`

The parser also supports `match` statements:

```langlog
fn choose(flag: bool) -> u32 {
    let mut value: u32 = 0;

    match flag {
        true => { value = 1; },
        false => { value = 2; }
    }

    value
}
```

Current pattern support is intentionally small:

- `_`
- a binding name
- an integer literal
- `true`
- `false`

That is enough to start shaping the AST and later semantic passes without
pretending the pattern language is finished.

## 10. Read Parser Errors

If you write malformed syntax:

```langlog
fn broken( {
```

`langlog check` prints a labeled error:

```text
error: expected a parameter name
  --> broken.llg:1:11
  |
1 | fn broken( {
  |           ^ identifier expected here
```

That error quality matters because the same span system will later be used by
name-resolution, type-checking, and proof diagnostics.

## 11. A Complete Example

This file is included as [examples/tutorial.llg](./examples/tutorial.llg):

```langlog
fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;

    for value in values {
        total = total + value or(err) 0;
    }

    total
}

fn bounded(total: u32, limit: u32) -> u32 {
    observe total <= limit else {
        return total;
    }

    let next: u32 = total + 1 or(err) total;

    if next > limit {
        return total;
    }

    next
}

fn choose(flag: bool) -> u32 {
    let mut value: u32 = 0;

    match flag {
        true => { value = 1; },
        false => { value = 2; }
    }

    value
}

fn main() -> u32 {
    let values: [u32; 4] = [1, 2, 3, 4];
    let left: u32 = sum(values);
    let right: u32 = bounded(10, 20);
    let selected: u32 = choose(true);
    let subtotal: u32 = left + right or(err) 0;

    subtotal + selected or(err) 0
}
```

Use it as a small end-to-end smoke test while the compiler grows.

## 12. What Comes Next

The parser is no longer the only stage. Current next compiler milestones are:

- define richer runtime semantics
- design executable Set/Map runtime representations
- define richer collection relation syntax and update obligations
- improve playground and documentation polish

So the right way to think about Langlog today is:

- the syntax is becoming concrete
- the semantic, proof, and Wasm layers exist, but they are still intentionally
  partial
- the reliability model is the point of the experiment
