# Langlog Proof IR Specification

Status: draft 0. This document defines the proof-specific intermediate
representation that sits between typed HIR and later marker-obligation
discharge logic.

Normative terms in this document follow RFC 2119, but they apply to the
compiler-facing proof IR rather than to the user-facing surface language.

This document complements, but does not replace, the main language spec:

- [SPEC.md](./SPEC.md) remains the authoritative surface-language and
  user-visible behavior spec.
- [TYPE_SYSTEM.md](./TYPE_SYSTEM.md) defines the place, value, marker fact, and
  structural mode hierarchy consumed by proof checking.
- [HIR.md](./HIR.md) defines the semantic IR and the AST-to-HIR elaboration
  boundary.
- [PLAN.md](./PLAN.md) tracks implementation sequencing and milestone status.
- A future `SEMANTICS.md` should formalize marker-obligation judgments over
  Proof IR and dynamic semantics over MIR.

## LLG-PIR-01 Pipeline And Lowering

- Successfully checked HIR MUST lower into Proof IR before marker-obligation
  discharge runs.
- Every Proof IR node MUST preserve a source span sufficient for diagnostics
  and traceability.

## LLG-PIR-02 Places And Marker Facts

- Every marker fact target in Proof IR MUST reference a `PlaceId` rather than
  identifier text.
- A `PlaceId` MUST identify a compiler-visible SSA place that can carry marker
  facts.
- Proof IR place state MUST have access to boundary-declared structural modes
  from HIR parameters, return slots, task fields, and state parameters.
- Proof IR MUST keep structural place mode separate from concrete value type
  and marker fact requirements.
- User-defined marker family facts MUST retain the source marker family name and
  instantiated place arguments.
- Proof IR MUST distinguish ordinary marker facts, immutable marker facts, and
  diagnostic-only hints.
- Diagnostic-only hints MUST NOT discharge marker obligations.

## LLG-PIR-03 Marker Obligations And Fact Sources

- Marker-required operations, including indexing and map-presence checks, MUST
  lower to explicit marker obligations that preserve the originating operation
  span.
- Every place-specific marker obligation MUST carry the required marker
  pattern, the target `PlaceId`, and the source operation span.
- Non-place marker obligations, such as future event-cycle productivity
  obligations, MUST carry an explicit obligation target that identifies the
  task or control-flow structure being checked.
- Marker fact sources MUST include control-flow truth markers, successful
  `observe` statements, unsafe marker construction, companion-rule
  implications, assignment identity, and immutable marker carry-forward.
- Comparison-based control-flow tests MUST lower to truth-marker facts on the
  condition result place.
- Direct checked `u32` arithmetic lowers to a successful payload place that can
  receive marker facts from the active arithmetic companion rule.
- Result recovery lowers to separate success and fallback marker paths, and the
  recovered place receives only marker facts proven on both paths.
- Marker facts that survive result recovery merging MUST use a recovery-merge
  fact source.

## LLG-PIR-04 Normalization Boundary

- Proof IR MUST retain only marker-relevant control flow, marker obligations,
  marker fact sources, and marker expressions; non-marker statements MAY be
  omitted unless needed to preserve marker scope.
- Grouped expressions and other parser- or HIR-only wrapper nodes MUST NOT
  survive as distinct Proof IR nodes.

## LLG-PIR-05 Successful Proof IR Well-Formedness

- Successfully lowered Proof IR MUST NOT contain unresolved names,
  identifier-text marker targets, unresolved marker patterns, or `Unknown` or
  otherwise untyped marker expressions.
- Every marker obligation and marker fact in successfully lowered Proof IR MUST
  be attributable to a source span in the originating HIR.

## Non-Normative Notes

The remaining sections are explanatory design notes. They describe the intended
shape of Proof IR and the migration strategy, but they do not yet lock in every
internal data-structure choice as a normative requirement.

## Purpose

Proof IR exists to separate marker-specific normalization from both general HIR
structure and final marker-obligation discharge.

It should let the proof engine work over:

- explicit marker obligations instead of discovering them while walking general
  HIR;
- normalized marker-producing operations instead of arbitrary expression trees;
- branch-scoped marker structure rather than full front-end statement syntax;
- `PlaceId`-based marker targets rather than names or resolution tables.

## Pipeline Position

The intended compilation pipeline is:

```text
source -> AST -> typed HIR -> proof IR -> MIR -> execution/backend
```

In this structure:

- HIR preserves checked source meaning with semantic normalization;
- Proof IR preserves only marker-relevant structure and obligations;
- MIR preserves executable control flow and state updates.

Future formalization should target this pipeline rather than full HIR alone:

- elaboration rules from HIR to Proof IR;
- marker fact generation and obligation discharge over Proof IR;
- dynamic semantics over MIR.

## Current Implementation Status

The current implementation lowers checked HIR into a structured Proof IR before
marker-obligation discharge. The checker now consumes that Proof IR, and
requirement tests assert the lowering boundary for obligations, facts, source
spans, recovery merges, and well-formed proof expressions.

This document still defines a draft internal boundary: the first Proof IR is
structured and close to current marker needs, while future MIR work should give
execution its own backend-independent semantics.

## Design Goals

- Keep the proof engine focused on marker concepts rather than general language
  traversal.
- Normalize marker-relevant control flow once during lowering.
- Preserve enough source information for user-facing diagnostics.
- Keep the first Proof IR structured and close to current marker needs rather
  than introducing a full CFG immediately.
- Support future relational markers without reworking the obligation model
  again.

## Non-Goals

- Proof IR is not the user-facing language contract.
- Proof IR is not the executable IR.
- Proof IR does not need to preserve HIR nodes that have no marker relevance.
- Proof IR does not need to be the final internal form for all future marker
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
    condition_place: PlaceId,
    then_facts: Vec<MarkerFact>,
    else_facts: Vec<MarkerFact>,
    diagnostic_hints: Vec<MarkerFact>,
    then_block: ProofBlock,
    else_block: Option<ProofBlock>,
    span: Span,
}

ProofEntry::Observe {
    condition_place: PlaceId,
    fact: MarkerFact,
    else_block: ProofBlock,
    span: Span,
}

ProofEntry::Obligation {
    target: ObligationTarget,
    required: MarkerPattern,
    source: ObligationSource,
    span: Span,
}

ProofEntry::Eval {
    expr: ProofExpr,
    span: Span,
}

MarkerFact {
    target: PlaceId,
    marker: MarkerPattern,
    source: MarkerFactSource,
    span: Span,
}

MarkerFactSource::ControlFlowTruth
MarkerFactSource::Observe
MarkerFactSource::UnsafeConstruction
MarkerFactSource::CompanionRule
MarkerFactSource::AssignmentIdentity
MarkerFactSource::ImmutableCarryForward

MarkerPattern::Equal { left: PlaceId, right: PlaceId }
MarkerPattern::LessThan { left: PlaceId, right: PlaceId }
MarkerPattern::MemberOf { key: PlaceId, map: PlaceId }
MarkerPattern::Event
MarkerPattern::True
MarkerPattern::False
MarkerPattern::User { family: String, args: Vec<PlaceId> }

ObligationTarget::Place(PlaceId)
ObligationTarget::StateCycle { task: TaskId, cycle: Vec<StateId> }

ObligationSource::Index { array: PlaceId, index: PlaceId }
ObligationSource::MapLookup { map: PlaceId, key: PlaceId }
ObligationSource::EventCycle
```

This is intentionally not final. The first objective is to make marker
obligations, branch-scoped marker facts, source spans, and stability
distinctions explicit.

## Expected First Lowering Rules

The first HIR-to-Proof-IR lowering is expected to follow these rules:

- A HIR `observe` lowers to an explicit marker-producing entry plus the guarded
  `else` block that defines the failing path.
- A marker-relevant `if` lowers to a branch entry that records `True()` for the
  condition result place in the then branch and `False()` for the condition
  result place in the else branch.
- Companion marker rules lower to marker-producing entries that preserve the
  source span of the operator application and the source span of the rule that
  emitted the marker.
- Proof checking MUST evaluate active companion marker rules against the current
  marker environment after branch or observe truth facts have been introduced.
- Source companion marker rules MAY override trusted builtin companion rules in
  the active Proof IR rule set.
- Direct checked `u32` arithmetic lowers to a successful payload place that can
  receive marker facts from the active arithmetic companion rule.
- Result recovery lowers to separate success and fallback marker paths, and the
  recovered place receives only marker facts proven on both paths.
- Marker facts that survive result recovery merging MUST use a recovery-merge
  fact source.
- Assignment lowers as marker identity propagation.
- Mutation lowers as a new `PlaceId` for the new SSA version of the mutated
  value.
- Function, task, state, field, and return boundaries lower with their
  declared structural place modes so copy, move, and discard checks can use the
  same receiving-mode compatibility rules as source checking.
- Immutable marker carry-forward lowers as an explicit marker fact source
  rather than as implicit reuse of the old place.
- Indexing, map-presence checks, and future raw arithmetic operations lower to
  explicit marker obligation entries whenever the language phase requires marker
  checking.
- Event-productivity checking for future explicit `go` cycles lowers cyclic
  state paths to marker obligations requiring an `Event` introduction in some
  state body on each cycle.
- Non-marker statements may lower only through their marker-relevant expressions
  rather than preserving the full statement shell.
- Marker fact targets lower to `PlaceId`s, while diagnostic displays still
  preserve the original source spans for the places and operators involved.

## Relationship To Formal Semantics

If Langlog later adds a formal proof system document, Proof IR is the right
place to write those judgments. It is smaller than HIR, closer to the solver
model, and stable enough to express:

- marker-fact introduction judgments;
- branch-scoped marker environment rules;
- marker-obligation generation judgments;
- direct marker-discharge rules for indexing, map presence, event
  productivity, and future raw arithmetic obligations.
