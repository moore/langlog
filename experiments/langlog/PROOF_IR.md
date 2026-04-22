# Langlog Proof IR Specification

Status: draft 0. This document defines the proof-specific intermediate
representation that sits between typed HIR and later proof discharge logic.

Normative terms in this document follow RFC 2119, but they apply to the
compiler-facing proof IR rather than to the user-facing surface language.

This document complements, but does not replace, the main language spec:

- [SPEC.md](./SPEC.md) remains the authoritative surface-language and
  user-visible behavior spec.
- [HIR.md](./HIR.md) defines the semantic IR and the AST-to-HIR elaboration
  boundary.
- [PLAN.md](./PLAN.md) tracks implementation sequencing and milestone status.
- A future `SEMANTICS.md` should formalize proof judgments over Proof IR and
  dynamic semantics over MIR.

## LLG-PIR-01 Pipeline And Lowering

- Successfully checked HIR MUST lower into Proof IR before proof obligation
  discharge runs.
- Every Proof IR node MUST preserve a source span sufficient for diagnostics
  and traceability.

## LLG-PIR-02 Fact Subjects And Stability

- Every proof fact subject in Proof IR MUST reference binding identity rather
  than identifier text.
- Proof IR MUST distinguish stable facts from mutable diagnostic-only hints so
  mutable comparisons cannot discharge obligations.

## LLG-PIR-03 Obligations And Fact Sources

- Potentially failing arithmetic, division or remainder, and indexing
  operations MUST lower to explicit proof obligations that preserve the
  originating operation span.
- Successful `observe` statements and comparison-based control-flow tests MUST
  lower to explicit fact-producing nodes that preserve the originating relation
  spans.

## LLG-PIR-04 Normalization Boundary

- Proof IR MUST retain only proof-relevant control flow, obligations, fact
  sources, and proof expressions; non-proof statements MAY be omitted unless
  needed to preserve proof scope.
- Grouped expressions and other parser- or HIR-only wrapper nodes MUST NOT
  survive as distinct Proof IR nodes.

## LLG-PIR-05 Successful Proof IR Well-Formedness

- Successfully lowered Proof IR MUST NOT contain unresolved names,
  identifier-text fact subjects, or `Unknown` or otherwise untyped proof
  expressions.
- Every proof obligation and fact in successfully lowered Proof IR MUST be
  attributable to a source span in the originating HIR.

## Non-Normative Notes

The remaining sections are explanatory design notes. They describe the intended
shape of Proof IR and the migration strategy, but they do not yet lock in every
internal data-structure choice as a normative requirement.

## Purpose

Proof IR exists to separate proof-specific normalization from both general HIR
structure and final proof discharge.

It should let the proof engine work over:

- explicit proof obligations instead of discovering them while walking general
  HIR;
- normalized fact-producing comparisons instead of arbitrary expression trees;
- branch-scoped proof structure rather than full front-end statement syntax;
- binding-identity-based subjects rather than names or resolution tables.

## Pipeline Position

The intended compilation pipeline is:

```text
source -> AST -> typed HIR -> proof IR -> MIR -> execution/backend
```

In this structure:

- HIR preserves checked source meaning with semantic normalization;
- Proof IR preserves only proof-relevant structure and obligations;
- MIR preserves executable control flow and state updates.

Future formalization should target this pipeline rather than full HIR alone:

- elaboration rules from HIR to Proof IR;
- proof fact generation and obligation discharge over Proof IR;
- dynamic semantics over MIR.

## Current Implementation Status

The current implementation still discharges obligations directly from HIR.
This document defines the intended next internal boundary so proof reasoning,
testing, and eventual formal semantics stop depending on the full HIR shape.

## Design Goals

- Keep the proof engine focused on proof concepts rather than general language
  traversal.
- Normalize proof-relevant control flow once during lowering.
- Preserve enough source information for user-facing diagnostics.
- Keep the first Proof IR structured and close to current proof needs rather
  than introducing a full CFG immediately.
- Support future relation proofs without reworking the obligation model again.

## Non-Goals

- Proof IR is not the user-facing language contract.
- Proof IR is not the executable IR.
- Proof IR does not need to preserve HIR nodes that have no proof relevance.
- Proof IR does not need to be the final internal form for all future proof
  optimizations.

## Illustrative Shape

The first Proof IR draft can stay structured rather than graph-shaped. One
possible shape is:

```rust
ProofProgram {
    functions: Vec<ProofFunction>,
}

ProofFunction {
    body: ProofBlock,
    span: Span,
}

ProofBlock {
    entries: Vec<ProofEntry>,
    span: Span,
}

ProofEntry::Branch {
    condition_facts: Vec<ProofFact>,
    mutable_hints: Vec<ProofFact>,
    then_block: ProofBlock,
    else_block: Option<ProofBlock>,
    span: Span,
}

ProofEntry::Observe {
    fact: ProofRelation,
    else_block: ProofBlock,
    span: Span,
}

ProofEntry::Obligation {
    kind: ObligationKind,
    span: Span,
}

ProofEntry::Eval {
    expr: ProofExpr,
    span: Span,
}

ProofRelation {
    subject: BindingId,
    op: ObserveOp,
    right: ProofExpr,
    source: FactSource,
    stable: bool,
    span: Span,
}

ObligationKind::Overflow { op: BinaryOp, left: ProofExpr, right: ProofExpr }
ObligationKind::NonZero { expr: ProofExpr }
ObligationKind::InBounds { target: ProofExpr, index: ProofExpr, length: u64 }
```

This is intentionally not final. The first objective is to make proof
obligations, branch-scoped facts, and stability distinctions explicit.

## Expected First Lowering Rules

The first HIR-to-Proof-IR lowering is expected to follow these rules:

- A HIR `observe` lowers to an explicit fact-producing entry plus the guarded
  `else` block that defines the failing path.
- A proof-relevant `if` lowers to a branch entry that records comparison facts
  for the guarded success path.
- Arithmetic, division or remainder, and indexing operations lower to explicit
  obligation entries whenever phase 1 requires proof.
- Non-proof statements may lower only through their proof-relevant expressions
  rather than preserving the full statement shell.
- Fact subjects lower to binding identities, while fact displays still preserve
  the original left and right source spans for diagnostics.

## Relationship To Formal Semantics

If Langlog later adds a formal proof system document, Proof IR is the right
place to write those judgments. It is smaller than HIR, closer to the solver
model, and stable enough to express:

- fact-introduction judgments;
- branch-scoped assumption rules;
- obligation-generation judgments;
- discharge rules for arithmetic, non-zero, indexing, and future relation
  obligations.
