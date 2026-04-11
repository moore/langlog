# Parsing Strategy

## Decision

Langlog will use a handwritten lexer plus a recursive-descent parser with Pratt-style expression parsing. We are not using a parser generator in phase 1.

## Why Not Pest

Pest is workable for small grammars, but it is a poor fit for where Langlog is headed:

- Rust-like syntax is expression-heavy and benefits from direct precedence handling.
- Langlog will need good recovery and high-quality diagnostics, especially around proof-oriented syntax and incomplete programs.
- Future syntax will likely contain context-sensitive edges where a PEG grammar becomes awkward or overly indirect.
- We want the AST and parser structure to evolve together instead of being constrained by grammar-tool ergonomics.

This is not an indictment of Pest in general. It is simply the wrong tradeoff for a language front end that will accumulate custom syntax and analysis pressure.

## Chosen Architecture

- A dedicated lexer converts source text into tokens with byte spans.
- The parser consumes token slices and produces an AST.
- Item and statement parsing use ordinary recursive-descent functions.
- Expression parsing uses Pratt parsing for precedence and associativity.
- Recovery happens at natural synchronization points such as `fn` starts, semicolons, and block terminators.
- Tokens, syntax nodes, and diagnostics all depend on the same `FileId` plus byte-span model so later error rendering can preserve Rust-like source precision.

## Crate Layout Target

`langlog-syntax` should grow toward this structure:

```text
src/
  ast.rs
  lexer.rs
  parser/
    expr.rs
    item.rs
    stmt.rs
  span.rs
  token.rs
  lib.rs
```

The exact filenames may change, but the separation of responsibilities should remain.

## Immediate Next Steps

1. Define the phase 1 token set and keyword table on top of the span model.
2. Implement the lexer with byte-span output and diagnostic stubs.
3. Define AST nodes for items, statements, expressions, and types.
4. Implement top-level and block parsing before filling in the full expression grammar.
5. Keep parser nodes spanned from the start instead of backfilling locations later.
