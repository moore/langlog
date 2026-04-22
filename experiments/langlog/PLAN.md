# Langlog Experiment Plan

Langlog is an experimental standalone language focused on reliability properties
that mainstream systems languages do not enforce structurally: bounded
execution, proof-required potentially failing operations, explicit resource
reasoning, and an eventual event-loop runtime with per-event temporary
allocation.

## Current Status

- Current phase: `M2 semantic typing plus early proof checks`
- Last completed milestone: `M1 Lexer and parser to AST`
- Next concrete task: Define the typed HIR, draft `HIR.md`, and move semantic
  and proof inputs onto that representation before relation work continues.
- Current blockers: None. LLVM tooling is intentionally deferred until after
  executable MIR semantics exist.
- Implemented semantic baseline: name resolution, recursion rejection, bounded
  loop enforcement, mutability checks, guarded `observe`, and an initial type
  checker for scalar operators, tuples, built-in generic shells, calls, arrays,
  indexing, assignments, and returns are all in place.
- Implemented proof baseline: control-flow and `observe` facts already drive
  overflow, divide-by-zero, and out-of-bounds indexing checks; stable facts are
  now keyed by binding identity, and mutable control-flow comparisons warn but
  do not discharge obligations.
- Project task runner: use `./tasks.sh` in `experiments/langlog/` to run the
  default fast checks in one place. Mutation testing is intentionally excluded
  from `./tasks.sh`; run `cargo mutants` manually when you explicitly want that
  slower check.
- Documentation split:
  - `SPEC.md` remains the surface-language and user-visible behavior spec.
  - `HIR.md` will define AST-to-HIR elaboration plus HIR invariants.
  - A future `SEMANTICS.md` should define formal static semantics over HIR and
    dynamic semantics over MIR.
- Formatting defaults: `rustfmt` and `rumdl` are both pinned to a 100-column
  line length so requirement text stays stable across Rust and Markdown tooling.

## Milestones

### M0 Workspace bootstrap

- [x] Create `experiments/langlog/` as the isolated experiment root.
- [x] Initialize a Cargo workspace under `experiments/langlog/`.
- [x] Add bootstrap crates for `langlog-driver`, `langlog-syntax`,
  `langlog-sema`, and `langlog-proof`.
- [x] Create `examples/` and `notes/` for sample programs and design documents.
- [x] Add `PLAN.md` as the restart point for future sessions.

Exit criteria: the directory layout exists, the workspace builds, and this plan
file records milestones plus design defaults.

### M1 Lexer and parser to AST

- [x] Define the source file structure for items, statements, and expressions.
- [x] Implement spans and source mapping infrastructure.
- [x] Implement tokens, lexer, and lexer diagnostics.
- [x] Implement an AST for the phase 1 language surface.
- [x] Build a parser with useful recovery at item and statement boundaries.
- [x] Expose `langlog check <file>` parsing with span-rich syntax errors.

Exit criteria: `langlog check <file>` can lex and parse a single source file
into AST and report parse errors with precise spans.

### M2 HIR plus semantic checks

- [ ] Draft `HIR.md` and define the initial AST-to-HIR elaboration boundary.
- [ ] Lower AST into a typed HIR.
- [ ] Re-home binding identity, mutability, and type attachment into HIR
  construction so later phases stop depending on parser AST plus semantic side
  tables.
- [x] Implement name resolution and scope handling.
- [x] Add the initial type checker for scalar operators, tuples, built-in
  generic shells, calls, arrays, indexing, assignments, and returns.
- [x] Reject recursion.
- [x] Reject unbounded loop forms and keep iteration syntax bounded.

Exit criteria: AST lowers to HIR, names resolve, types check, recursion is
rejected, and unbounded loop forms are rejected.

### M3 Proof engine for obligations and observations

- [ ] Define a control-flow-based proof IR from HIR.
- [x] Represent obligations for divide/mod by zero and out-of-bounds indexing.
- [x] Add overflow obligations.
- [x] Infer facts from comparison-based control flow.
- [x] Support explicit `observe` facts when inference is insufficient.
- [x] Emit proof diagnostics when obligations are not discharged.

Exit criteria: the checker can accept or reject arithmetic and indexing based on
inferred or explicit facts.

### M4 Minimal relation enforcement on collections

- [ ] Add syntax and HIR support for declared collection relations.
- [ ] Enforce one initial relation form: `Set<K, N>` membership implies presence
  in a `Map<K, V, M>`.
- [ ] Create proof obligations for constrained collection updates.
- [ ] Report relation violations with source-linked diagnostics.

Exit criteria: one declared relation is enforced and proven during checking.

### M5 MIR plus interpreter or VM

- [ ] Lower checked programs into MIR.
- [ ] Draft `SEMANTICS.md` for formal dynamic semantics over MIR once the MIR
  surface stabilizes.
- [ ] Define executable semantics for the phase 1 language surface.
- [ ] Implement an interpreter or VM for MIR.
- [ ] Preserve proof-approved semantics without inserting hidden fallback
  behavior.

Exit criteria: checked programs lower to MIR and execute in an interpreter or
VM.

### M6 Event-loop runtime and async lowering

- [ ] Define the single-event-loop runtime model.
- [ ] Add bounded event handlers and scheduling semantics.
- [ ] Implement per-event temporary allocation and long-lived bounded
  collections.
- [ ] Lower async constructs into explicit runtime state machines.

Exit criteria: the event-loop runtime exists with bounded handlers and per-event
temporary allocation.

### M7 LLVM backend

- [ ] Lower MIR into LLVM-oriented code generation IR.
- [ ] Generate LLVM IR for checked programs.
- [ ] Produce native binaries through an LLVM-based backend.
- [ ] Verify backend behavior against interpreter or VM semantics.

Exit criteria: MIR lowers to LLVM IR and native binaries can be produced.

## Design Defaults

- Standalone compiler, not proc macros.
- Rust-like syntax for the first language surface.
- `SPEC.md` is the authoritative draft language spec for phase 1 decisions.
- `HIR.md` is the semantic IR spec for elaboration and compiler-facing
  invariants; it complements `SPEC.md` rather than replacing it.
- Future formal semantics should target HIR and MIR rather than raw parser AST.
- Diagnostics-only front end before execution backends.
- Potentially failing arithmetic and indexing are proof-required, not silently
  runtime-checked.
- Proof facts come from both control flow and explicit `observe`.
- Capacity-bounded `Set` and `Map` are part of the early type system.
- Parsing uses a handwritten lexer plus recursive-descent parser with
  Pratt-style expression parsing.
- The long-term runtime model is a single event loop with bounded handlers.
- LLVM is deferred until MIR semantics and the runtime model are stable.
- `journel.md` stays at the repository root as the idea journal.
