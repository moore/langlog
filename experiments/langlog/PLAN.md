# Langlog Experiment Plan

Langlog is an experimental standalone language focused on reliability properties
that mainstream systems languages do not enforce structurally: bounded
execution, proof-required potentially failing operations, explicit resource
reasoning, and an eventual event-loop runtime with per-event temporary
allocation.

## Current Status

- Current phase: `M4 Minimal relation enforcement on collections`
- Last completed milestone: `M3 Proof engine for obligations and observations`
- Next concrete task: Decide whether declared collection-relation syntax belongs
  in M4 before broader relation work continues, then either add that syntax or
  explicitly keep relation work implicit for one more iteration.
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
- Implemented relation baseline: one implicit relation is checked today;
  iterating `Set<K, N>` can prove key presence in a related `Map<K, V, M>` for
  the loop binding. Declared relation syntax and constrained update checking are
  still open.
- Implemented executable baseline: a Wasm V1 backend can build and run the
  current non-collection executable subset, including checked arithmetic,
  recovery, scalar/aggregate values, arrays, bounded loops, `if`, `match`,
  `observe` runtime else blocks, direct calls, local assignment, and host
  builtins. `Set` and `Map` remain check/proof-only in Wasm V1.
- Project task runner: use `./tasks.sh` in `experiments/langlog/` to run the
  default fast checks in one place. Mutation testing is intentionally excluded
  from `./tasks.sh`; run `cargo mutants` manually when you explicitly want that
  slower check.
- Current verification baseline: `cargo test`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo fmt --all -- --check`, `rumdl check .
  --respect-gitignore`, `cargo run -p langlog-xtask -- check-requirements`, and
  `duvet report --require-tests true` are expected to pass. The requirement
  validator currently reports implemented requirement tests only; no ignored
  `todo_*` placeholders are present yet.
- Documentation split:
  - `SPEC.md` remains the surface-language and user-visible behavior spec.
  - `HIR.md` defines AST-to-HIR elaboration plus HIR invariants.
  - `PROOF_IR.md` defines HIR-to-Proof-IR lowering plus proof-facing
    invariants.
  - `SEMANTICS.md` defines the current checked-result semantics; future MIR
    work should extend it with dynamic semantics.
  - `WASM.md` defines the current Wasm V1 backend and browser playground ABI.
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

- [x] Draft `HIR.md` and define the initial AST-to-HIR elaboration boundary.
- [x] Lower AST into a typed HIR.
- [x] Re-home binding identity, mutability, and type attachment into HIR
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

- [x] Draft `PROOF_IR.md` and define the initial HIR-to-Proof-IR boundary.
- [x] Lower HIR into a control-flow-based Proof IR.
- [x] Represent obligations for divide/mod by zero and out-of-bounds indexing.
- [x] Add overflow obligations.
- [x] Infer facts from comparison-based control flow.
- [x] Support explicit `observe` facts when inference is insufficient.
- [x] Emit proof diagnostics when obligations are not discharged.

Exit criteria: the checker can accept or reject arithmetic and indexing based on
inferred or explicit facts.

### M4 Minimal relation enforcement on collections

- [ ] Add syntax and HIR support for declared collection relations.
- [x] Enforce one initial implicit relation form: iterating `Set<K, N>` implies
  key presence in a `Map<K, V, M>` for the loop binding.
- [ ] Create proof obligations for constrained collection updates.
- [ ] Report relation violations with source-linked diagnostics.

Exit criteria: one declared relation is enforced and proven during checking.

### Executable Wasm V1 side track

This work was implemented before the full MIR/interpreter milestone. It is
useful for demos, examples, and playground validation, but it does not replace
M5 because there is still no backend-independent executable semantics layer.

- [x] Add `langlog-wasm` as a backend for the current executable subset.
- [x] Add `langlog build --target wasm <path>`.
- [x] Build Wasm for `fn main() -> u32` programs after syntax, semantic, and
  proof checks pass.
- [x] Execute checked arithmetic, recovery expressions, direct calls, locals,
  assignments, arrays, bounded loops, `if`, `match`, and `observe` runtime else
  blocks in Wasm V1.
- [x] Add host builtin imports for browser/playground interaction.
- [x] Add `langlog-playground-wasm` and a static browser playground adapter.
- [ ] Keep Wasm V1 behavior aligned with future MIR/interpreter semantics once
  M5 exists.

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
- `PROOF_IR.md` is the proof-facing IR spec for lowering and obligation
  structure between HIR and proof discharge.
- Future formal semantics should target HIR, Proof IR, and MIR rather than raw
  parser AST.
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
