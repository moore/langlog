use langlog_sema::CheckedProgram;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSummary {
    pub obligations: usize,
    pub observations: usize,
}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=todo
//# The proof phase MUST reject arithmetic that may overflow unless safety is proven.
//= SPEC.md#llg-proof-01-proof-required-operations
//= type=todo
//# The proof phase MUST reject division or remainder operations that may divide by zero unless safety is proven.
//= SPEC.md#llg-proof-01-proof-required-operations
//= type=todo
//# The proof phase MUST reject indexing that may go out of bounds unless safety is proven.
//= SPEC.md#llg-proof-02-observations
//= type=todo
//# The proof phase MUST derive facts from control-flow tests such as comparisons, range checks, length checks, and membership tests.
//= SPEC.md#llg-proof-02-observations
//= type=todo
//# The proof phase MUST incorporate explicit `observe` statements into the fact model.
//= SPEC.md#llg-rel-01-collections-and-relations
//= type=todo
//# The first enforced relation MUST allow membership in a `Set<K, N>` to imply presence in a `Map<K, V, M>`.
pub fn check(_program: &CheckedProgram) -> ProofSummary {
    ProofSummary {
        obligations: 0,
        observations: 0,
    }
}
