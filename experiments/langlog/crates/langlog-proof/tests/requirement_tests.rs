use langlog_proof::{check, CheckedProof, FactSource};
use langlog_sema::{analyze, CheckedProgram};
use langlog_syntax::{parse, LabelStyle, ObserveOp, Severity};

fn check_proof(source: &str) -> (CheckedProgram, CheckedProof) {
    let parsed = parse("requirement.llg", source);
    assert!(!parsed.has_errors(), "{:#?}", parsed.diagnostics);

    let checked = analyze(parsed);
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let proof = check(&checked);
    (checked, proof)
}

fn check_ok(source: &str) -> (CheckedProgram, CheckedProof) {
    let (checked, proof) = check_proof(source);
    assert!(!proof.has_errors(), "{:#?}", proof.diagnostics);
    (checked, proof)
}

fn check_err(source: &str) -> (CheckedProgram, CheckedProof) {
    let (checked, proof) = check_proof(source);
    assert!(proof.has_errors(), "{:#?}", proof.diagnostics);
    (checked, proof)
}

fn assert_fact(
    checked: &CheckedProgram,
    proof: &CheckedProof,
    expected_source: FactSource,
    expected_left: &str,
    expected_op: ObserveOp,
    expected_right: &str,
) {
    assert!(
        proof.facts.iter().any(|fact| {
            fact.source == expected_source
                && fact.op == expected_op
                && checked.parsed.source.span_text(fact.left_span) == Some(expected_left)
                && checked.parsed.source.span_text(fact.right_span) == Some(expected_right)
        }),
        "missing fact {expected_source:?} {expected_left} {expected_op:?} {expected_right:?}: {:#?}",
        proof.facts
    );
}

fn assert_primary_diagnostic(
    checked: &CheckedProgram,
    proof: &CheckedProof,
    message: &str,
    expected_span_text: &str,
) {
    assert_primary_diagnostic_with_severity(
        checked,
        proof,
        Severity::Error,
        message,
        expected_span_text,
    );
}

fn assert_primary_diagnostic_with_severity(
    checked: &CheckedProgram,
    proof: &CheckedProof,
    severity: Severity,
    message: &str,
    expected_span_text: &str,
) {
    assert!(proof.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == severity
            && diagnostic.message == message
            && diagnostic.labels.iter().any(|label| {
                label.style == LabelStyle::Primary
                    && checked.parsed.source.span_text(label.span) == Some(expected_span_text)
            })
    }));
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# In the current phase, the proof phase MUST derive facts from comparison-based control-flow tests.
#[test]
fn requirement_llg_proof_02_derives_facts_from_control_flow_tests() {
    let (checked, proof) = check_ok(
        r#"
fn main(total: u32, limit: u32, baseline: u32) {
    if total < limit && total >= baseline {
        total;
    }
}
"#,
    );

    let control_flow_facts = proof
        .facts
        .iter()
        .filter(|fact| fact.source == FactSource::ControlFlow)
        .count();

    // A comparison on the left side of `&&` should become a control-flow fact.
    assert_fact(
        &checked,
        &proof,
        FactSource::ControlFlow,
        "total",
        ObserveOp::Lt,
        "limit",
    );

    // A comparison on the right side of `&&` should also become a control-flow fact.
    assert_fact(
        &checked,
        &proof,
        FactSource::ControlFlow,
        "total",
        ObserveOp::GtEq,
        "baseline",
    );

    // Only the two comparison tests should be recorded for this example.
    assert_eq!(control_flow_facts, 2);
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Control-flow equality and inequality comparisons MUST be available as proof facts inside the guarded branch.
#[test]
fn requirement_llg_proof_02_derives_equality_facts_from_control_flow_tests() {
    let (checked, proof) = check_ok(
        r#"
fn main(index: u32, other: u32) {
    if index == 1 {
        index;
    }
    if other != 0 {
        other;
    }
}
"#,
    );

    assert_fact(
        &checked,
        &proof,
        FactSource::ControlFlow,
        "index",
        ObserveOp::Eq,
        "1",
    );
    assert_fact(
        &checked,
        &proof,
        FactSource::ControlFlow,
        "other",
        ObserveOp::NotEq,
        "0",
    );
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# The proof phase MUST incorporate explicit `observe` statements into the fact model on the continuing path after a guarded `observe` succeeds.
#[test]
fn requirement_llg_proof_02_incorporates_observe_statements() {
    let (checked, proof) = check_ok(
        r#"
fn main(total: u32, limit: u32) {
    observe total <= limit else {
        return;
    }
    observe total != 0 else {
        return;
    }
}
"#,
    );

    let observe_facts = proof
        .facts
        .iter()
        .filter(|fact| fact.source == FactSource::Observe)
        .count();

    // Each explicit `observe` statement should add one fact to the model.
    assert_eq!(observe_facts, 2);

    // The recorded facts should preserve the observed relations.
    assert_fact(
        &checked,
        &proof,
        FactSource::Observe,
        "total",
        ObserveOp::LtEq,
        "limit",
    );
    assert_fact(
        &checked,
        &proof,
        FactSource::Observe,
        "total",
        ObserveOp::NotEq,
        "0",
    );
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# In phase 1, an `observe` fact MUST relate a left-hand proof expression to a right-hand proof expression.
#[test]
fn requirement_llg_proof_02_represents_phase_1_observe_facts_as_relations() {
    let (checked, proof) = check_ok(
        r#"
fn main(total: u32, limit: u32) {
    observe total <= limit else {
        return;
    }
    observe total != 0 else {
        return;
    }
}
"#,
    );

    // The fact model should preserve the observed relation.
    assert_fact(
        &checked,
        &proof,
        FactSource::Observe,
        "total",
        ObserveOp::NotEq,
        "0",
    );

    // This example includes both explicit observe relations.
    assert_eq!(proof.observations, 2);
}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=test
//# The proof phase MUST reject indexing that may go out of bounds unless safety is proven.
#[test]
fn requirement_llg_proof_01_rejects_possible_out_of_bounds_indexing_without_proof() {
    let (checked, failing_proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    values[index];
}
"#,
    );

    // Reject an index whose upper bound is not proven.
    assert_primary_diagnostic(
        &checked,
        &failing_proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );

    let (_, literal_safe) = check_ok(
        r#"
fn main() {
    [10, 20, 30][2];
}
"#,
    );

    // A constant in-bounds index against an array literal should be accepted.
    assert_eq!(literal_safe.obligations, 1);

    let (_, observed_safe) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    observe index < 4 else {
        return;
    }
    values[index];
}
"#,
    );

    // An explicit upper-bound observe should discharge the indexing obligation.
    assert_eq!(observed_safe.obligations, 1);

    let (_, control_flow_safe) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    }
}
"#,
    );

    // A control-flow bound check should discharge the indexing obligation in the guarded block.
    assert_eq!(control_flow_safe.obligations, 1);
}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=test
//# Indexing MUST require the proven index upper bound to be strictly less than the indexed array length.
#[test]
fn requirement_llg_proof_01_requires_index_upper_bound_below_array_length() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4], exact: u32, range: u32) {
    values[4];

    observe exact <= 4 else {
        return;
    }
    values[exact];

    observe range < 5 else {
        return;
    }
    values[range];
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "4",
    );
    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "exact",
    );
    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "range",
    );
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Control-flow comparisons over mutable bindings MUST be tracked for diagnostics but MUST NOT discharge proof obligations.
#[test]
fn requirement_llg_proof_02_does_not_use_mutable_control_flow_facts_to_discharge_obligations() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 {
        values[index];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert_primary_diagnostic_with_severity(
        &checked,
        &proof,
        Severity::Warning,
        "mutable control-flow comparison cannot discharge this proof obligation",
        "index < 4",
    );
    assert_eq!(proof.obligations, 1);
    assert_eq!(proof.diagnostics.len(), 2);
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Warnings about mutable control-flow facts MUST appear only when such a fact would otherwise discharge a real obligation.
#[test]
fn requirement_llg_proof_02_warns_only_when_mutable_control_flow_would_discharge_an_obligation() {
    let (_, no_obligation_warning) = check_ok(
        r#"
fn main() {
    let mut index = 0;
    if index < 4 {
        index;
    }
    if index < 4 {
        [10, 20, 30, 40][0];
    }
}
"#,
    );

    assert!(
        !no_obligation_warning.has_warnings(),
        "{:#?}",
        no_obligation_warning.diagnostics
    );

    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 {
        values[index];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert_primary_diagnostic_with_severity(
        &checked,
        &proof,
        Severity::Warning,
        "mutable control-flow comparison cannot discharge this proof obligation",
        "index < 4",
    );
    assert!(
        proof.has_warnings(),
        "expected mutable-control-flow warning: {:#?}",
        proof.diagnostics
    );
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# A mutable control-flow warning MUST be reported when mutable facts would discharge a proof obligation.
#[test]
fn requirement_llg_proof_02_reports_warning_when_mutable_facts_would_discharge_obligation() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 {
        values[index];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert!(proof.has_warnings(), "{:#?}", proof.diagnostics);
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Redundant mutable control-flow hints MUST NOT produce extra warnings for an obligation that is already explained by another mutable hint.
#[test]
fn requirement_llg_proof_02_suppresses_redundant_mutable_control_flow_warnings() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 && index <= 3 {
        values[index];
    }
}
"#,
    );

    let warning_count = proof
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();
    assert!(warning_count <= 1, "{:#?}", proof.diagnostics);
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Proof checking MUST inspect obligations inside `else` branches.
#[test]
fn requirement_llg_proof_02_checks_obligations_inside_else_branches() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        return;
    } else {
        values[index];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Proof facts MUST be available for bindings introduced inside `else` branches, loop patterns, match patterns, and expression blocks.
#[test]
fn requirement_llg_proof_02_collects_bindings_from_nested_fact_sources() {
    let proof = check_ok(
        r#"
fn main(values: [u32; 4], flag: bool) {
    if flag {
        return;
    } else {
        let nested_index = 1;
        if nested_index < 4 {
            values[nested_index];
        }
    }

    for index in [0, 1] {
        if index < 4 {
            values[index];
        }
    }

    match 1 {
        captured => {
            if captured < 4 {
                values[captured];
            }
        }
    }

    {
        let block_index = 1;
        if block_index < 4 {
            values[block_index];
        }
    };
}
"#,
    )
    .1;

    assert!(!proof.has_errors(), "{:#?}", proof.diagnostics);
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Mutable control-flow facts MUST NOT survive reassignment as if they were stable proofs.
#[test]
fn requirement_llg_proof_02_rejects_mutable_reassignment_regressions() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 {
        index = 4;
        values[index];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );

    assert!(proof.has_warnings(), "{:#?}", proof.diagnostics);
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# Binding-based proof facts MUST attach to binding identity rather than identifier text so shadowing does not inherit outer facts.
#[test]
fn requirement_llg_proof_02_distinguishes_shadowed_bindings_when_applying_facts() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let index = 0;
    observe index < 4 else {
        return;
    }
    {
        let index = 4;
        values[index];
    };
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert!(
        !proof.has_warnings(),
        "shadowing regressions should fail without mutable-control-flow warnings: {:#?}",
        proof.diagnostics
    );
}
