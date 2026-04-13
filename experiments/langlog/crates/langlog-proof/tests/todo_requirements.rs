//= SPEC.md#llg-proof-01-proof-required-operations
//= type=todo
//# The proof phase MUST reject arithmetic that may overflow unless safety is proven.
#[test]
#[ignore = "proof requirements are not implemented"]
fn todo_llg_proof_01_rejects_possible_overflow_without_proof() {}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=todo
//# The proof phase MUST reject division or remainder operations that may divide by zero unless safety is proven.
#[test]
#[ignore = "proof requirements are not implemented"]
fn todo_llg_proof_01_rejects_possible_divide_by_zero_without_proof() {}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=todo
//# The proof phase MUST reject indexing that may go out of bounds unless safety is proven.
#[test]
#[ignore = "proof requirements are not implemented"]
fn todo_llg_proof_01_rejects_possible_out_of_bounds_indexing_without_proof() {}

//= SPEC.md#llg-proof-02-observations
//= type=todo
//# The proof phase MUST derive facts from control-flow tests such as comparisons, range checks, length checks, and membership tests.
#[test]
#[ignore = "proof requirements are not implemented"]
fn todo_llg_proof_02_derives_facts_from_control_flow_tests() {}

//= SPEC.md#llg-proof-02-observations
//= type=todo
//# The proof phase MUST incorporate explicit `observe` statements into the fact model.
#[test]
#[ignore = "proof requirements are not implemented"]
fn todo_llg_proof_02_incorporates_observe_statements() {}

//= SPEC.md#llg-proof-02-observations
//= type=todo
//# In phase 1, an `observe` fact MUST relate a named left-hand side symbol to a scalar-valued right-hand side expression.
#[test]
#[ignore = "proof requirements are not implemented"]
fn todo_llg_proof_02_represents_phase_1_observe_facts_as_relations() {}

//= SPEC.md#llg-rel-01-collections-and-relations
//= type=todo
//# The first enforced relation MUST allow membership in a `Set<K, N>` to imply presence in a `Map<K, V, M>`.
#[test]
#[ignore = "relation requirements are not implemented"]
fn todo_llg_rel_01_propagates_set_membership_to_map_presence() {}
