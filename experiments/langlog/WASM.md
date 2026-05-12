# Langlog Wasm Backend Specification

Status: draft 0. This document defines the traceable requirements for the
current WebAssembly backend and browser-host ABI.

Normative terms in this document follow RFC 2119, but they apply to Wasm
lowering and execution rather than to the full surface language.

This document complements, but does not replace, the main language spec:

- [SPEC.md](./SPEC.md) defines the user-facing language and CLI front end.
- [HIR.md](./HIR.md) defines the semantic IR consumed by later compiler phases.
- [PROOF_IR.md](./PROOF_IR.md) defines the proof-specific IR boundary.

## LLG-WASM-01 Build Gate And Entry Point

- The Wasm compiler MUST reject programs that do not have checked HIR.
- Wasm builds MUST stop before backend lowering when front-end or proof checks
  fail.
- A successful Wasm build MUST produce WAT and non-empty Wasm bytes.
- Wasm builds MUST export `fn main() -> u32` as `main`.
- Wasm V1 MUST reject `main` forms other than `fn main() -> u32`.
- Wasm builds MUST export `task main() -> u32` as `main`.
- Wasm V1 MUST reject task-mode roots other than `task main() -> u32`.
- Wasm V1 MUST compile helper functions returning flattened aggregate values.
- Wasm V1 MUST compile generic flattened `Result<T, E>` values.
- Wasm V1 MUST compile helper functions returning `()` without Wasm result
  values.
- Wasm V1 task-state layout MUST include exactly the tasks reachable from
  `task main` through `delegate` statements and MUST size the shared state
  slots to the largest reachable task-state variant.
- Wasm V1 task-state layout MUST collect cyclic delegation without recursive
  stack growth.
- Wasm V1 task-state layout MUST expose delegate target parameter offsets for
  state transitions.
- Wasm V1 MUST lower `delegate` statements as task-state transitions without
  direct Wasm calls to task items.
- Wasm V1 MUST execute cyclic task delegation as bounded task-state
  transitions.
- Wasm V1 MUST evaluate delegate arguments before replacing caller task state.
- Wasm V1 MUST discard caller task-local state before entering delegated target
  task state.
- Wasm V1 MUST compile `forever` task statements as Wasm loops.
- Wasm V1 MUST emit imports for host builtins used inside reachable task
  bodies.
- Wasm V1 MUST reject task-state values that are not representable as flattened
  Wasm values.
- Wasm V1 MUST reject Set and Map values as check/proof-only runtime values.
- Wasm V1 MUST reject first-class function values and indirect calls.
- Wasm V1 MUST reject assignment targets other than local bindings.
- When backend lowering fails during `langlog build --target wasm`, the CLI
  MUST print diagnostics to stderr.
- Wasm task-root diagnostics MUST be reported through the CLI stderr path
  during build.
- Wasm build diagnostics MUST be reported without panicking.

## LLG-WASM-02 Scalar Execution

- Wasm V1 MUST lower `u32` and `bool` values as Wasm `i32` values.
- Wasm V1 MUST execute checked arithmetic expressions over `u32` values when
  their `Result` is recovered.
- Wasm V1 MUST execute direct function calls.
- Wasm V1 MUST pass fixed-size scalar tuple parameters to direct function
  calls.
- Wasm V1 MUST execute `if` statements using scalar conditions.
- Wasm V1 MUST execute `else` branches when scalar `if` conditions are false.
- Wasm V1 MUST compile unit-valued block expressions without leaving stack
  values.
- Wasm V1 MUST execute mutable local assignment.
- Wasm V1 MUST execute structural equality and inequality for flattened values.

## LLG-WASM-03 Arrays And Loops

- Wasm V1 MUST execute fixed-size scalar array literals and constant indexing.
- Wasm V1 MUST execute constant indexing directly on fixed-size scalar array
  literals.
- Wasm V1 MUST lower constant array indices directly rather than through
  dynamic index dispatch.
- Wasm V1 MUST execute dynamic indexing into fixed-size scalar arrays.
- Wasm V1 MUST execute `for` loops over fixed-size scalar arrays.
- Wasm V1 MUST pass fixed-size scalar arrays to direct function calls.
- Wasm V1 MUST execute `for` loops over u32 ranges and range bindings.
- Wasm V1 MUST execute fixed-size arrays with flattened aggregate elements.
- Wasm V1 MUST execute `for` loops over fixed-size arrays with aggregate
  elements.

## LLG-WASM-04 Match And Observe

- Wasm V1 MUST execute `match` statements over scalar boolean patterns.
- Wasm V1 MUST execute `match` statements with scalar binding patterns.
- Wasm V1 MUST discard non-unit expression match-arm bodies used in statement
  position.
- Wasm V1 MUST execute an `observe` else block when the observed relation is
  false at runtime.

## LLG-WASM-05 Host Builtins

- The semantic phase MUST resolve host builtin calls without user declarations.
- HIR MUST lower host builtin calls to explicit host builtin callees.
- User functions MUST NOT use reserved host builtin names.
- Host builtin calls MUST NOT create recursion edges.
- Wasm V1 MUST emit imports for used host builtins.
- Wasm V1 MUST emit imports for host builtins used inside nested `else`
  branches.
- Wasm V1 MUST execute host builtin imports through the `langlog_host` module.

## LLG-WASM-06 Playground Adapter

- The playground `check` API MUST report success and frontend counts without
  producing Wasm bytes.
- The playground `build` API MUST report Wasm text and bytes but not mark the
  module runnable.
- The playground `buildAndRunReady` API MUST mark successful Wasm builds
  runnable.
- The playground APIs MUST report rendered diagnostics and separated error
  messages for invalid source.
- Native playground adapter tests MUST expose the wasm-bindgen APIs as
  inspectable string summaries.
- The playground example programs MUST build successfully and be marked
  runnable by the playground adapter.
