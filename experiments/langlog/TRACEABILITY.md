# Duvet Traceability

Langlog uses [Duvet](https://awslabs.github.io/duvet/) to trace normative
requirements in [SPEC.md](./SPEC.md), [HIR.md](./HIR.md), and
[PROOF_IR.md](./PROOF_IR.md) to implementation and planned work.

## Layout

- `.duvet/config.toml` configures Duvet for this experiment.
- `SPEC.md` contains the normative surface-language requirements using RFC 2119
  terms.
- `HIR.md` contains normative compiler-facing semantic-IR requirements using
  RFC 2119 terms.
- `PROOF_IR.md` contains normative compiler-facing proof-IR requirements using
  RFC 2119 terms.
- Rust test files use Duvet annotations such as `//=` and `//#` to trace both
  implemented requirements and planned work.
- Planned but not yet implemented requirements are tracked with `type=todo`
  annotations on ignored placeholder tests.

## Run The Report

From `experiments/langlog/`:

```text
duvet report --require-tests false
```

This uses `.duvet/config.toml` by default and writes reports under
`.duvet/reports/`.

## Current Strategy

- Parser and diagnostic requirements are verified by tests in `langlog-syntax`
  and `langlog-driver`.
- HIR and Proof IR requirements should be traced by tests or placeholder todo
  tests in the crates that own lowering and validation.
- Each normative requirement bullet should map to exactly one
  `requirement_*` test function.
- Each `requirement_*` test function should trace exactly one normative
  requirement bullet.
- Each planned-but-unimplemented requirement should map to exactly one ignored
  `todo_*` placeholder test.
- `cargo run -p langlog-xtask -- check-requirements` enforces that Duvet annotations live on
  test functions and that the one-to-one shape holds before the Duvet report
  runs.
- Unit tests outside the requirement suites should cover non-normative helper
  behavior and local invariants rather than duplicate spec-backed contracts.
- Semantic and proof requirements that are planned but not implemented are
  traced by placeholder tests in `langlog-sema/tests/` and
  `langlog-proof/tests/`, including future Proof IR work.
- The spec is intentionally small and requirement-oriented so the traceability
  graph remains stable while the language evolves.
