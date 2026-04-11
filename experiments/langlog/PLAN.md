# Langlog Experiment Plan

Langlog is an experimental standalone language focused on reliability properties that mainstream systems languages do not enforce structurally: bounded execution, proof-required potentially failing operations, explicit resource reasoning, and an eventual event-loop runtime with per-event temporary allocation.

## Current Status

- Current phase: `M1 Lexer and parser to AST`
- Last completed milestone: `M0 Workspace bootstrap`
- Next concrete task: Define the source-file layout, token kinds, span representation, and parser entrypoints in `langlog-syntax`.
- Current blockers: None. LLVM tooling is intentionally deferred until after executable MIR semantics exist.

## Milestones

### M0 Workspace bootstrap

- [x] Create `experiments/langlog/` as the isolated experiment root.
- [x] Initialize a Cargo workspace under `experiments/langlog/`.
- [x] Add bootstrap crates for `langlog-driver`, `langlog-syntax`, `langlog-sema`, and `langlog-proof`.
- [x] Create `examples/` and `notes/` for sample programs and design documents.
- [x] Add `PLAN.md` as the restart point for future sessions.

Exit criteria: the directory layout exists, the workspace builds, and this plan file records milestones plus design defaults.

### M1 Lexer and parser to AST

- [ ] Define the source file structure for items, statements, and expressions.
- [ ] Implement spans, tokens, and lexer diagnostics.
- [ ] Implement an AST for the phase 1 language surface.
- [ ] Build a parser with useful recovery at item and statement boundaries.
- [ ] Expose `langlog check <file>` parsing with span-rich syntax errors.

Exit criteria: `langlog check <file>` can lex and parse a single source file into AST and report parse errors with precise spans.

### M2 HIR plus semantic checks

- [ ] Lower AST into a typed HIR.
- [ ] Implement name resolution and scope handling.
- [ ] Add the initial type checker for scalars, tuples, arrays, `Option`, `Result`, `Set`, and `Map`.
- [ ] Reject recursion.
- [ ] Reject unbounded loop forms and keep iteration syntax bounded.

Exit criteria: AST lowers to HIR, names resolve, types check, recursion is rejected, and unbounded loop forms are rejected.

### M3 Proof engine for obligations and observations

- [ ] Define a control-flow-based proof IR from HIR.
- [ ] Represent obligations for overflow, divide/mod by zero, and out-of-bounds indexing.
- [ ] Infer facts from control flow, comparisons, length checks, and membership tests.
- [ ] Support explicit `observe` facts when inference is insufficient.
- [ ] Emit proof diagnostics when obligations are not discharged.

Exit criteria: the checker can accept or reject arithmetic and indexing based on inferred or explicit facts.

### M4 Minimal relation enforcement on collections

- [ ] Add syntax and HIR support for declared collection relations.
- [ ] Enforce one initial relation form: `Set<K, N>` membership implies presence in a `Map<K, V, M>`.
- [ ] Create proof obligations for constrained collection updates.
- [ ] Report relation violations with source-linked diagnostics.

Exit criteria: one declared relation is enforced and proven during checking.

### M5 MIR plus interpreter or VM

- [ ] Lower checked programs into MIR.
- [ ] Define executable semantics for the phase 1 language surface.
- [ ] Implement an interpreter or VM for MIR.
- [ ] Preserve proof-approved semantics without inserting hidden fallback behavior.

Exit criteria: checked programs lower to MIR and execute in an interpreter or VM.

### M6 Event-loop runtime and async lowering

- [ ] Define the single-event-loop runtime model.
- [ ] Add bounded event handlers and scheduling semantics.
- [ ] Implement per-event temporary allocation and long-lived bounded collections.
- [ ] Lower async constructs into explicit runtime state machines.

Exit criteria: the event-loop runtime exists with bounded handlers and per-event temporary allocation.

### M7 LLVM backend

- [ ] Lower MIR into LLVM-oriented code generation IR.
- [ ] Generate LLVM IR for checked programs.
- [ ] Produce native binaries through an LLVM-based backend.
- [ ] Verify backend behavior against interpreter or VM semantics.

Exit criteria: MIR lowers to LLVM IR and native binaries can be produced.

## Design Defaults

- Standalone compiler, not proc macros.
- Rust-like syntax for the first language surface.
- Diagnostics-only front end before execution backends.
- Potentially failing arithmetic and indexing are proof-required, not silently runtime-checked.
- Proof facts come from both control flow and explicit `observe`.
- Capacity-bounded `Set` and `Map` are part of the early type system.
- The long-term runtime model is a single event loop with bounded handlers.
- LLVM is deferred until MIR semantics and the runtime model are stable.
- `journel.md` stays at the repository root as the idea journal.
