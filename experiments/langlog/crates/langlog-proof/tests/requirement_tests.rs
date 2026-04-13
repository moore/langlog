use langlog_proof::{check, CheckedProof, FactSource};
use langlog_sema::{analyze, CheckedProgram};
use langlog_syntax::{parse, LabelStyle, ObserveOp};

fn check_ok(source: &str) -> (CheckedProgram, CheckedProof) {
    let parsed = parse("requirement.llg", source);
    assert!(!parsed.has_errors(), "{:#?}", parsed.diagnostics);

    let checked = analyze(parsed);
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let proof = check(&checked);
    assert!(!proof.has_errors(), "{:#?}", proof.diagnostics);

    (checked, proof)
}

fn check_err(source: &str) -> (CheckedProgram, CheckedProof) {
    let parsed = parse("requirement.llg", source);
    assert!(!parsed.has_errors(), "{:#?}", parsed.diagnostics);

    let checked = analyze(parsed);
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let proof = check(&checked);
    assert!(proof.has_errors(), "{:#?}", proof.diagnostics);

    (checked, proof)
}

fn assert_fact(
    checked: &CheckedProgram,
    proof: &CheckedProof,
    expected_source: FactSource,
    expected_subject: &str,
    expected_op: ObserveOp,
    expected_value: &str,
) {
    assert!(
        proof.facts.iter().any(|fact| {
            fact.source == expected_source
                && fact.subject_name == expected_subject
                && fact.op == expected_op
                && checked.parsed.source.span_text(fact.subject_span) == Some(expected_subject)
                && checked.parsed.source.span_text(fact.value_span) == Some(expected_value)
        }),
        "missing fact {expected_source:?} {expected_subject} {expected_op:?} {expected_value:?}: {:#?}",
        proof.facts
    );
}

fn assert_primary_diagnostic(
    checked: &CheckedProgram,
    proof: &CheckedProof,
    message: &str,
    expected_span_text: &str,
) {
    assert!(proof.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == message
            && diagnostic.labels.iter().any(|label| {
                label.style == LabelStyle::Primary
                    && checked.parsed.source.span_text(label.span) == Some(expected_span_text)
            })
    }));
}

//= SPEC.md#llg-proof-02-observations
//= type=test
//# The proof phase MUST derive facts from control-flow tests such as comparisons, range checks, length checks, and membership tests.
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
//# The proof phase MUST incorporate explicit `observe` statements into the fact model.
#[test]
fn requirement_llg_proof_02_incorporates_observe_statements() {
    let (checked, proof) = check_ok(
        r#"
fn main(total: u32, limit: u32) {
    observe total <= limit;
    observe total != 0;
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
//# In phase 1, an `observe` fact MUST relate a named left-hand side symbol to a scalar-valued right-hand side expression.
#[test]
fn requirement_llg_proof_02_represents_phase_1_observe_facts_as_relations() {
    let (checked, proof) = check_ok(
        r#"
fn main(total: u32, limit: u32, one: u32) {
    observe total <= limit + one;
}
"#,
    );

    // The fact model should preserve the named subject from the left-hand side.
    assert_fact(
        &checked,
        &proof,
        FactSource::Observe,
        "total",
        ObserveOp::LtEq,
        "limit + one",
    );

    // This example introduces exactly one phase 1 observe relation.
    assert_eq!(proof.observations, 1);
}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=test
//# The proof phase MUST reject division or remainder operations that may divide by zero unless safety is proven.
#[test]
fn requirement_llg_proof_01_rejects_possible_divide_by_zero_without_proof() {
    let (checked, failing_proof) = check_err(
        r#"
fn main(total: u32, denom: u32) {
    total / denom;
    total % denom;
}
"#,
    );

    // Reject an unproven divisor in a division operation.
    assert_primary_diagnostic(
        &checked,
        &failing_proof,
        "possible divide-by-zero is not proven safe",
        "denom",
    );

    // Reject an unproven divisor in a remainder operation too.
    assert_eq!(failing_proof.diagnostics.len(), 2);

    let (_, observed_safe) = check_ok(
        r#"
fn main(total: u32, denom: u32) {
    observe denom != 0;
    total / denom;
}
"#,
    );

    // An explicit non-zero observe should discharge the obligation.
    assert_eq!(observed_safe.obligations, 1);

    let (_, control_flow_safe) = check_ok(
        r#"
fn main(total: u32, denom: u32) {
    if denom > 0 {
        total % denom;
    }
}
"#,
    );

    // A control-flow comparison should also discharge the obligation in the guarded block.
    assert_eq!(control_flow_safe.obligations, 1);
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
    observe index < 4;
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
