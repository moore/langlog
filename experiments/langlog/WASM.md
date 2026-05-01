# Langlog Wasm Backend Specification

Status: draft 0. This document defines the traceable requirements for the
current WebAssembly backend and browser-host ABI.

Normative terms in this document follow RFC 2119, but they apply to Wasm
lowering and execution rather than to the full surface language.

This document complements, but does not replace, the main language spec:

- [SPEC.md](./SPEC.md) defines the user-facing language and CLI front end.
- [HIR.md](./HIR.md) defines the semantic IR consumed by later compiler phases.
- [PROOF_IR.md](./PROOF_IR.md) defines the planned proof-specific IR boundary.

## LLG-WASM-01 Build Gate And Entry Point

- The Wasm compiler MUST reject programs that do not have checked HIR.
- Wasm builds MUST stop before backend lowering when front-end or proof checks
  fail.
- A successful Wasm build MUST produce WAT and non-empty Wasm bytes.
- Wasm builds MUST export `fn main() -> u32` as `main`.
- Wasm V1 MUST reject `main` forms other than `fn main() -> u32`.
- Wasm V1 MUST reject aggregate return values.
- When backend lowering fails during `langlog build --target wasm`, the CLI
  MUST print diagnostics to stderr.
- Wasm build diagnostics MUST be reported without panicking.

## LLG-WASM-02 Scalar Execution

- Wasm V1 MUST lower `u32` and `bool` values as Wasm `i32` values.
- Wasm V1 MUST execute arithmetic expressions over `u32` values.
- Wasm V1 MUST execute direct function calls.
- Wasm V1 MUST execute `if` statements using scalar conditions.
- Wasm V1 MUST execute mutable local assignment.

## LLG-WASM-03 Arrays And Loops

- Wasm V1 MUST execute fixed-size scalar array literals and constant indexing.
- Wasm V1 MUST execute dynamic indexing into fixed-size scalar arrays.
- Wasm V1 MUST execute `for` loops over fixed-size scalar arrays.
- Wasm V1 MUST pass fixed-size scalar arrays to direct function calls.
- Wasm V1 MUST reject non-scalar array elements.

## LLG-WASM-04 Match And Observe

- Wasm V1 MUST execute `match` statements over scalar boolean patterns.
- Wasm V1 MUST execute `match` statements with scalar binding patterns.
- Wasm V1 MUST execute an `observe` else block when the observed relation is
  false at runtime.

## LLG-WASM-05 Host Builtins

- The semantic phase MUST resolve host builtin calls without user declarations.
- HIR MUST lower host builtin calls to explicit host builtin callees.
- User functions MUST NOT use reserved host builtin names.
- Host builtin calls MUST NOT create recursion edges.
- Wasm V1 MUST emit imports for used host builtins.
- Wasm V1 MUST execute host builtin imports through the `langlog_host` module.
