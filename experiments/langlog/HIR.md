# Langlog HIR Specification

Status: draft 0. This document defines the initial high-level intermediate
representation that sits between the parser AST and later proof, relation, and
execution phases.

Normative terms in this document follow RFC 2119, but they apply to the
compiler-facing semantic IR rather than to the user-facing surface language.

This document complements, but does not replace, the main language spec:

- [SPEC.md](./SPEC.md) remains the authoritative surface-language and
  user-visible behavior spec.
- [PLAN.md](./PLAN.md) tracks implementation sequencing and milestone status.
- A future `SEMANTICS.md` should formalize static semantics over HIR and
  dynamic semantics over MIR.

## LLG-HIR-01 Pipeline And Lowering

- The front end MUST lower successfully checked programs from AST into typed
  HIR before generating proof IR or MIR.
- Every HIR node MUST preserve a source span sufficient for diagnostics and
  traceability.

## LLG-HIR-02 Identities And Resolution

- Every HIR function item, parameter, and local binding MUST carry a stable
  semantic identity, and every HIR name use MUST resolve to either an item
  identity or a binding identity.

## LLG-HIR-03 Types And Mutability

- Every HIR binding MUST record its mutability and type directly, and every
  HIR expression MUST record its type directly.

## LLG-HIR-04 Normalization Boundary

- Omitted surface function return types MUST lower to explicit `()` return
  types in HIR, grouped expressions MUST NOT survive as distinct HIR nodes, and
  HIR blocks MUST represent trailing result positions explicitly.
- In HIR v0, `observe` MUST remain an explicit HIR statement that preserves
  both proof expressions and the guarded `else` block.

## LLG-HIR-05 Successful HIR Well-Formedness

- Successfully checked HIR MUST NOT contain unresolved names or `Unknown`
  types.

## Non-Normative Notes

The remaining sections are explanatory design notes. They describe the current
intended shape of HIR and the migration strategy, but they do not yet lock in
every internal data-structure choice as a normative requirement.

## Purpose

The HIR exists to give the compiler a stable semantic representation after
parsing and before proof checking or execution lowering.

It should let later phases work over:

- resolved bindings instead of bare identifier text;
- attached types instead of repeated type reconstruction;
- normalized semantic structure instead of parser-only AST shapes;
- stable node and binding identities that survive future language growth.

## Pipeline Position

The intended compilation pipeline is:

```text
source -> AST -> typed HIR -> proof IR -> MIR -> execution/backend
```

In this structure:

- the AST preserves the source-oriented syntax tree;
- HIR preserves source meaning with semantic normalization;
- proof IR captures proof-relevant control flow and obligations;
- MIR captures executable control flow and state updates.

Future formalization should target this pipeline rather than the raw parser AST
alone:

- elaboration rules from AST to HIR;
- static semantics, proof facts, and obligation generation over HIR;
- dynamic semantics over MIR.

## Design Goals

- Make name resolution explicit on HIR nodes.
- Attach types directly to HIR expressions and bindings.
- Keep enough source spans for diagnostics.
- Normalize away syntax-only distinctions that later phases should not care
  about.
- Stay close enough to the current implementation that the first HIR migration
  is incremental rather than a rewrite.

## Non-Goals

- HIR is not the user-facing language contract.
- HIR is not the final proof IR.
- HIR is not the final executable IR.
- HIR does not need to expose parser recovery artifacts from erroneous source.

## Illustrative Shape

The first HIR draft should stay intentionally close to the current phase 1
surface. That keeps migration risk low while still making semantic information
explicit.

Illustrative shape:

```rust
Program {
    functions: Vec<Function>,
}

Function {
    id: ItemId,
    name: String,
    params: Vec<Binding>,
    return_type: Type,
    body: Block,
    span: Span,
}

Binding {
    id: BindingId,
    name: String,
    kind: BindingKind,
    mutable: bool,
    ty: Type,
    span: Span,
}

Block {
    statements: Vec<Stmt>,
    result: Option<Expr>,
    span: Span,
}

Stmt::Let {
    binding: Binding,
    annotation: Option<Type>,
    value: Option<Expr>,
    span: Span,
}

Stmt::Assign {
    target: Expr,
    value: Expr,
    span: Span,
}

Stmt::Expr {
    expr: Expr,
    span: Span,
}

Stmt::If {
    condition: Expr,
    then_block: Block,
    else_branch: Option<ElseBranch>,
    span: Span,
}

Stmt::Match {
    scrutinee: Expr,
    arms: Vec<MatchArm>,
    span: Span,
}

Stmt::For {
    pattern: Pattern,
    iterable: Expr,
    body: Block,
    span: Span,
}

Stmt::Return {
    value: Option<Expr>,
    span: Span,
}

Stmt::Observe {
    left: Expr,
    op: ObserveOp,
    right: Expr,
    else_block: Block,
    span: Span,
}

Expr {
    kind: ExprKind,
    ty: Type,
    span: Span,
}

ExprKind::Binding(BindingId)
ExprKind::Item(ItemId)
ExprKind::Int(u64)
ExprKind::Bool(bool)
ExprKind::Tuple(Vec<Expr>)
ExprKind::Array(Vec<Expr>)
ExprKind::Block(Block)
ExprKind::Unary { op: UnaryOp, expr: Box<Expr> }
ExprKind::Binary { op: BinaryOp, left: Box<Expr>, right: Box<Expr> }
ExprKind::Call { callee: Box<Expr>, args: Vec<Expr> }
ExprKind::Index { target: Box<Expr>, index: Box<Expr> }
```

This is intentionally not final. It is the smallest semantic IR that makes
binding identity, mutability, and types explicit.

## Types In HIR

The initial HIR type set should match the currently supported semantic type
set:

- `()`
- `bool`
- `u32`
- tuples
- fixed arrays
- `Option<T>`
- `Result<T, E>`
- `Set<T, N>`
- `Map<K, V, N>`
- `range<T>`
- named item types as needed for early front-end checking
- function types for callable items

`Unknown` may still exist in recovery-oriented lowering paths while the
migration is in progress, but it should not appear in a successfully checked
HIR artifact.

In the current front end, programs that would otherwise leave HIR types
unknown are rejected during semantic checking instead. The first two concrete
cases are `let` bindings with neither an annotation nor an initializer, and
empty array literals without a contextual element type.

## AST To HIR Mapping Notes

The first HIR elaboration is expected to follow these rules:

- A parsed function item lowers to one HIR function with an explicit return
  type. If the surface omitted the return type, HIR uses `()`.
- A surface parameter or local binding lowers to one HIR binding with a stable
  `BindingId`, mutability flag, type, and declaration span.
- A surface name expression lowers to either `ExprKind::Binding` or
  `ExprKind::Item` depending on semantic resolution.
- A block lowers to a list of HIR statements plus an optional trailing result
  expression.
- `if` may remain a statement in HIR for now because that matches the current
  language surface.
- `match` may remain a structured statement in HIR for now; deeper lowering can
  wait for MIR if needed.
- The first HIR draft may keep assignment targets as general expressions rather
  than introducing a distinct lvalue grammar immediately.
- Pattern forms may remain close to the current syntax because phase 1 supports
  only wildcard, binding, integer literal, and boolean literal patterns.

## Relationship To Proof

HIR should not directly store derived proof facts. Instead:

- HIR stores the semantic program;
- the proof phase derives control-flow facts and proof obligations from HIR;
- mutable and stable binding information comes from HIR bindings rather than
  ad hoc span lookups against the parser AST.

This keeps HIR as a semantic source representation rather than mixing it with
phase-specific proof state.

## Relationship To Future Formal Semantics

The long-term formalization should be layered:

1. `AST ==> HIR` elaboration.
2. HIR well-formedness and typing.
3. HIR-to-proof obligation generation.
4. MIR operational semantics.

That split keeps the formal story manageable:

- surface syntax remains a parsing concern;
- semantic meaning is stabilized in HIR;
- execution rules are written over MIR rather than over rich source syntax.

## Open Questions

The first HIR draft leaves several choices intentionally open:

- whether HIR should live in a dedicated crate such as `langlog-hir` or begin
  inside `langlog-sema`;
- whether assignment should gain a distinct lvalue representation during the
  first HIR migration;
- whether `match` should remain structured in HIR or lower earlier;
- what exact HIR node shape should represent future collection relations;
- whether proof IR should become a separate owned representation or a derived
  view over HIR.

## Near-Term Migration Plan

The expected implementation order is:

1. Introduce HIR data structures and this spec.
2. Lower only successfully checked programs into HIR first.
3. Make proof consume HIR-backed checked programs.
4. Move additional semantic bookkeeping from side tables into HIR nodes.
5. Add relation declarations and relation checking on top of HIR.
