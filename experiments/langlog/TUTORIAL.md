# Langlog Tutorial

This tutorial introduces the current Langlog prototype as it exists today: a
parser-first compiler front end with span-rich syntax errors. You can already
write `.llg` files and run them through `langlog check`, even though execution
and most semantic analysis are still ahead of the project.

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

## 2. Write Your First Function

Every current Langlog file is a list of functions:

```langlog
fn add_one(value: u32) -> u32 {
    value + 1
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

## 3. Bind Values With `let`

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
- arithmetic operators, ordering comparisons, and range bounds must use `u32`
- array literals must be homogeneous, and indexing requires an array target
  plus a `u32` index

## 4. Use Arrays And Loops

Arrays are written with square brackets:

```langlog
let values: [u32; 4] = [1, 2, 3, 4];
```

You can iterate with `for`:

```langlog
fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;

    for value in values {
        total = total + value;
    }

    total
}
```

Right now, the parser accepts any expression after `in`. The long-term language
design is stricter: loops have to be bounded. The parser still accepts any
expression after `in`, but semantic checking now rejects loop iterables outside
the phase 1 bounded model.

## 5. State Facts With `observe`

One of Langlog’s core ideas is that programs should be able to state facts the
proof engine can use later. The syntax for that is `observe`:

```langlog
fn bounded(total: u32, limit: u32, one: u32) -> u32 {
    observe total + one <= limit + one else {
        return total;
    }
    total
}
```

Today:

- `observe` parses as `observe <expr> <op> <expr> else <block>`
- the `else` block is mandatory
- both sides must be phase 1 proof expressions
- the supported operators are `==`, `!=`, `<`, `<=`, `>`, and `>=`
- tuple, array, block, range, logical, equality, and comparison
  subexpressions are rejected inside phase 1 proof expressions
- it appears in the AST
- semantic checking rejects proof expressions that directly reference `mut`
  bindings
- semantic checking requires the `else` block to be terminal
- when the observed relation is true, the proof phase records the relational
  fact from the statement
- the proof phase also records simple comparison facts from `if` conditions
- the proof phase now rejects division or remainder by zero and out-of-bounds
  indexing when safety is not proven
- arithmetic overflow checking is still ahead of the current implementation

Later, `observe` will help discharge obligations such as overflow safety and
bounds safety.

## 6. Branch With `if`

`if` uses Rust-like syntax:

```langlog
fn clamp_flag(total: u32) -> u32 {
    if total > 100 {
        observe total + 1 < 1001 else {
            return total;
        }
    } else {
        observe total >= 0 else {
            return total;
        }
    }

    total
}
```

In the current front end, `if` is parsed as a statement. It is not yet an
expression form.

## 7. Use `match`

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

## 8. Read Parser Errors

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

## 9. A Complete Example

This file is included as [examples/tutorial.llg](./examples/tutorial.llg):

```langlog
fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;

    for value in values {
        total = total + value;
    }

    total
}

fn bounded(total: u32, limit: u32, one: u32) -> u32 {
    observe total + one <= limit + one else {
        return total;
    }

    if total > 100 {
        observe total + 1 < 1001 else {
            return total;
        }
    }

    total
}

fn choose(flag: bool) -> u32 {
    let mut value: u32 = 0;

    match flag {
        true => { value = 1; },
        false => { value = 2; }
    }

    value
}
```

Use it as the main parser smoke test while the compiler grows.

## 10. What Comes Next

The parser is no longer the only stage. The next compiler milestones are:

- lower the AST into HIR
- add overflow obligations to the proof phase
- enforce the first collection relation

So the right way to think about Langlog today is:

- the syntax is becoming concrete
- the semantic and proof layers exist, but they are still intentionally partial
- the reliability model is the point of the experiment
