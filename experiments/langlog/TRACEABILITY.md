# Duvet Traceability

Langlog uses [Duvet](https://awslabs.github.io/duvet/) to trace normative
requirements in [SPEC.md](./SPEC.md) to implementation and planned work.

## Layout

- `.duvet/config.toml` configures Duvet for this experiment.
- `SPEC.md` contains the normative requirements using RFC 2119 terms.
- Rust source files use Duvet annotations such as `//=` and `//#` to cite
  implemented requirements.
- Planned but not yet implemented requirements are marked with `type=todo`
  annotations in the relevant crates.

## Run The Report

From `experiments/langlog/`:

```text
duvet report --require-tests false
```

This uses `.duvet/config.toml` by default and writes reports under
`.duvet/reports/`.

## Current Strategy

- Parser and diagnostic requirements are traced directly to `langlog-syntax` and
  `langlog-driver`.
- Semantic and proof requirements that are planned but not implemented are
  traced with `todo` annotations in `langlog-sema` and `langlog-proof`.
- The spec is intentionally small and requirement-oriented so the traceability
  graph remains stable while the language evolves.
