//= HIR.md#llg-hir-01-pipeline-and-lowering
//= type=todo
//# The front end MUST lower successfully checked programs from AST into typed HIR before generating proof IR or MIR.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_01_lowers_checked_programs_into_typed_hir_before_later_irs() {}

//= HIR.md#llg-hir-01-pipeline-and-lowering
//= type=todo
//# Every HIR node MUST preserve a source span sufficient for diagnostics and traceability.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_01_preserves_hir_source_spans_for_diagnostics_and_traceability() {}

//= HIR.md#llg-hir-02-identities-and-resolution
//= type=todo
//# Every HIR function item, parameter, and local binding MUST carry a stable semantic identity, and every HIR name use MUST resolve to either an item identity or a binding identity.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_02_attaches_stable_identities_and_resolved_references() {}

//= HIR.md#llg-hir-03-types-and-mutability
//= type=todo
//# Every HIR binding MUST record its mutability and type directly, and every HIR expression MUST record its type directly.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_03_records_mutability_and_types_directly_on_hir_nodes() {}

//= HIR.md#llg-hir-04-normalization-boundary
//= type=todo
//# Omitted surface function return types MUST lower to explicit `()` return types in HIR, grouped expressions MUST NOT survive as distinct HIR nodes, and HIR blocks MUST represent trailing result positions explicitly.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_04_normalizes_returns_grouping_and_block_results() {}

//= HIR.md#llg-hir-04-normalization-boundary
//= type=todo
//# In HIR v0, `observe` MUST remain an explicit HIR statement that preserves both proof expressions and the guarded `else` block.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_04_preserves_observe_as_an_explicit_hir_statement() {}

//= HIR.md#llg-hir-05-successful-hir-well-formedness
//= type=todo
//# Successfully checked HIR MUST NOT contain unresolved names or `Unknown` types.
#[test]
#[ignore = "HIR requirements are not implemented"]
fn todo_llg_hir_05_excludes_unresolved_names_and_unknown_types() {}
