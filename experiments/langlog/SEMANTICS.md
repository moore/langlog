# Langlog Semantics Specification

Status: draft 0. This document defines normative static and dynamic semantics
for Langlog features whose behavior is broader than surface parsing or a single
backend.

Normative terms in this document follow RFC 2119.

This document complements, but does not replace, the other traceable specs:

- [SPEC.md](./SPEC.md) defines surface syntax, diagnostics, and front-end
  behavior.
- [HIR.md](./HIR.md) defines compiler-facing semantic IR requirements.
- [PROOF_IR.md](./PROOF_IR.md) defines the planned proof-specific IR boundary.
- [WASM.md](./WASM.md) defines backend and host-ABI requirements.
- [INTEGER_SAFETY.md](./INTEGER_SAFETY.md) gives non-normative rationale for
  the integer safety model.

## LLG-SEM-01 Builtin Result Types

- `Option<T>`, `Result<T, E>`, and `ArithmeticError` MUST be builtin semantic
  types in the first checked-arithmetic phase.
- `ArithmeticError` MUST represent arithmetic overflow, arithmetic underflow,
  divide-by-zero, and remainder-by-zero failures.
- Builtin `Option` and `Result` types MUST use explicit type arguments without
  requiring user-defined enum or generic declarations.
- Builtin constructors `some`, `none`, `ok`, and `err` MUST construct builtin
  `Option` and `Result` values without requiring user-defined enum variants.
- Builtin constructors `arithmetic_overflow`, `arithmetic_underflow`,
  `divide_by_zero`, and `remainder_by_zero` MUST construct the corresponding
  `ArithmeticError` values.

## LLG-SEM-02 Recovery Expressions

- Recovery expressions MUST support `option_expr or fallback_expr`, producing
  `T` from `Option<T>` when `fallback_expr` has type `T`.
- Recovery expressions MUST support `result_expr or(err) fallback_expr`,
  producing `T` from `Result<T, E>` when `fallback_expr` has type `T`.
- In a result recovery expression, the error binding MUST be scoped only inside
  the fallback expression and MUST have the result error type.
- Recovery expressions MUST evaluate the fallback expression only for `None` or
  `Err` values.

## LLG-SEM-03 Checked Arithmetic

- Ordinary `+`, `-`, `*`, `/`, and `%` operations on `u32` operands MUST return
  `Result<u32, ArithmeticError>`.
- Successful checked arithmetic MUST produce an `Ok` result containing the
  computed `u32` value.
- Checked addition and multiplication overflow MUST produce an
  `ArithmeticError` instead of wrapping.
- Checked subtraction underflow MUST produce an `ArithmeticError` instead of
  wrapping.
- Checked division and remainder by zero MUST produce an `ArithmeticError`.

## LLG-SEM-04 Result Lifting

- Arithmetic operators MUST lift over operands of the same numeric type when
  either operand is `Result<T, ArithmeticError>`.
- Result-lifted arithmetic MUST produce `Result<T, ArithmeticError>` for the
  shared numeric type `T`.
- Result-lifted arithmetic MUST propagate the first arithmetic error in
  left-to-right evaluation order.

## LLG-SEM-05 Numeric Type Discipline

- Numeric operators MUST NOT perform implicit numeric promotion.
- Numeric operators MUST require the same underlying numeric type after
  stripping any compatible `Result<T, ArithmeticError>` layer.

## LLG-SEM-06 Raw Arithmetic Reservation

- Future raw or proof-backed arithmetic MUST be explicit at the operation site
  and MUST NOT be inferred from ordinary arithmetic operators.
- This checked-arithmetic phase MUST NOT reserve or recognize exact surface
  names for raw or proof-backed arithmetic operations.
