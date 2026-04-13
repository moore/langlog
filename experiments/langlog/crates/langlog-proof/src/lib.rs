use langlog_sema::CheckedProgram;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSummary {
    pub obligations: usize,
    pub observations: usize,
}

pub fn check(_program: &CheckedProgram) -> ProofSummary {
    ProofSummary {
        obligations: 0,
        observations: 0,
    }
}
