//= PROOF_IR.md#llg-pir-01-pipeline-and-lowering
//= type=todo
//# Successfully checked HIR MUST lower into Proof IR before proof obligation discharge runs.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_01_lowers_checked_hir_into_proof_ir_before_discharge() {}

//= PROOF_IR.md#llg-pir-01-pipeline-and-lowering
//= type=todo
//# Every Proof IR node MUST preserve a source span sufficient for diagnostics and traceability.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_01_preserves_proof_ir_source_spans() {}

//= PROOF_IR.md#llg-pir-02-fact-subjects-and-stability
//= type=todo
//# Every proof fact subject in Proof IR MUST reference binding identity rather than identifier text.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_02_references_binding_identity_for_fact_subjects() {}

//= PROOF_IR.md#llg-pir-02-fact-subjects-and-stability
//= type=todo
//# Proof IR MUST distinguish stable facts from mutable diagnostic-only hints so mutable comparisons cannot discharge obligations.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_02_distinguishes_stable_facts_from_mutable_hints() {}

//= PROOF_IR.md#llg-pir-03-obligations-and-fact-sources
//= type=todo
//# Potentially failing arithmetic, division or remainder, and indexing operations MUST lower to explicit proof obligations that preserve the originating operation span.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_03_lowers_operations_to_explicit_proof_obligations() {}

//= PROOF_IR.md#llg-pir-03-obligations-and-fact-sources
//= type=todo
//# Successful `observe` statements and comparison-based control-flow tests MUST lower to explicit fact-producing nodes that preserve the originating relation spans.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_03_lowers_observe_and_control_flow_tests_to_fact_nodes() {}

//= PROOF_IR.md#llg-pir-04-normalization-boundary
//= type=todo
//# Proof IR MUST retain only proof-relevant control flow, obligations, fact sources, and proof expressions; non-proof statements MAY be omitted unless needed to preserve proof scope.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_04_retains_only_proof_relevant_structure() {}

//= PROOF_IR.md#llg-pir-04-normalization-boundary
//= type=todo
//# Grouped expressions and other parser- or HIR-only wrapper nodes MUST NOT survive as distinct Proof IR nodes.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_04_removes_grouping_and_wrapper_nodes() {}

//= PROOF_IR.md#llg-pir-05-successful-proof-ir-well-formedness
//= type=todo
//# Successfully lowered Proof IR MUST NOT contain unresolved names, identifier-text fact subjects, or `Unknown` or otherwise untyped proof expressions.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_05_excludes_unresolved_names_identifier_text_and_untyped_exprs() {}

//= PROOF_IR.md#llg-pir-05-successful-proof-ir-well-formedness
//= type=todo
//# Every proof obligation and fact in successfully lowered Proof IR MUST be attributable to a source span in the originating HIR.
#[test]
#[ignore = "proof IR requirements are not implemented"]
fn todo_llg_pir_05_preserves_source_attribution_for_obligations_and_facts() {}

//= SPEC.md#llg-rel-01-collections-and-relations
//= type=todo
//# The first enforced relation MUST allow membership in a `Set<K, N>` to imply presence in a `Map<K, V, M>`.
#[test]
#[ignore = "relation requirements are not implemented"]
fn todo_llg_rel_01_propagates_set_membership_to_map_presence() {}
