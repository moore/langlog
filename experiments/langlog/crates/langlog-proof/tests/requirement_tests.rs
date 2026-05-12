use langlog_proof::{
    check, CheckedProof, FactSource, ProofBlock, ProofEntry, ProofExpr, ProofExprKind,
    ProofObligationKind, ProofProgram,
};
use langlog_sema::{analyze, CheckedProgram, HirBindingId, HirType};
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

fn proof_ir(proof: &CheckedProof) -> &ProofProgram {
    proof.proof_ir.as_ref().expect("expected lowered Proof IR")
}

fn collect_entries<'a>(block: &'a ProofBlock, entries: &mut Vec<&'a ProofEntry>) {
    for entry in &block.entries {
        entries.push(entry);
        match entry {
            ProofEntry::Branch {
                then_block,
                else_block,
                ..
            } => {
                collect_entries(then_block, entries);
                if let Some(else_block) = else_block {
                    collect_entries(else_block, entries);
                }
            }
            ProofEntry::Observe { else_block, .. } => collect_entries(else_block, entries),
            ProofEntry::For { body, .. } | ProofEntry::Scope { block: body, .. } => {
                collect_entries(body, entries);
            }
            ProofEntry::Obligation { .. } | ProofEntry::Eval { .. } => {}
        }
    }
}

fn proof_entries(proof: &CheckedProof) -> Vec<&ProofEntry> {
    let mut entries = Vec::new();
    for function in &proof_ir(proof).functions {
        collect_entries(&function.body, &mut entries);
    }
    entries
}

fn assert_non_empty_span(span: langlog_syntax::Span, context: &str) {
    assert!(!span.is_empty(), "{context} should have a non-empty span");
}

fn assert_expr_spans(expr: &ProofExpr) {
    assert_non_empty_span(expr.span, "ProofExpr");
    match &expr.kind {
        ProofExprKind::Binding(_)
        | ProofExprKind::Item
        | ProofExprKind::HostBuiltin
        | ProofExprKind::Int(_)
        | ProofExprKind::Bool(_) => {}
        ProofExprKind::Tuple(elements) | ProofExprKind::Array(elements) => {
            for element in elements {
                assert_expr_spans(element);
            }
        }
        ProofExprKind::Unary { expr } => assert_expr_spans(expr),
        ProofExprKind::Binary { left, right, .. } => {
            assert_expr_spans(left);
            assert_expr_spans(right);
        }
        ProofExprKind::Call { callee, args } => {
            assert_expr_spans(callee);
            for arg in args {
                assert_expr_spans(arg);
            }
        }
        ProofExprKind::Index { target, index } => {
            assert_expr_spans(target);
            assert_expr_spans(index);
        }
    }
}

fn assert_type_has_no_named_unknown(ty: &HirType) {
    match ty {
        HirType::Named(name) => panic!("Proof IR should not contain unresolved type {name:?}"),
        HirType::Tuple(elements) => {
            for element in elements {
                assert_type_has_no_named_unknown(element);
            }
        }
        HirType::Array { element, .. }
        | HirType::Option(element)
        | HirType::Range(element)
        | HirType::Set { element, .. } => assert_type_has_no_named_unknown(element),
        HirType::Result { ok, err } => {
            assert_type_has_no_named_unknown(ok);
            assert_type_has_no_named_unknown(err);
        }
        HirType::Map { key, value, .. } => {
            assert_type_has_no_named_unknown(key);
            assert_type_has_no_named_unknown(value);
        }
        HirType::Function(function) => {
            for param in &function.params {
                assert_type_has_no_named_unknown(param);
            }
            assert_type_has_no_named_unknown(&function.return_type);
        }
        HirType::Unit | HirType::Bool | HirType::U32 | HirType::ArithmeticError => {}
    }
}

fn assert_expr_well_formed(expr: &ProofExpr) {
    assert_type_has_no_named_unknown(&expr.ty);
    match &expr.kind {
        ProofExprKind::Binding(_)
        | ProofExprKind::Item
        | ProofExprKind::HostBuiltin
        | ProofExprKind::Int(_)
        | ProofExprKind::Bool(_) => {}
        ProofExprKind::Tuple(elements) | ProofExprKind::Array(elements) => {
            for element in elements {
                assert_expr_well_formed(element);
            }
        }
        ProofExprKind::Unary { expr } => assert_expr_well_formed(expr),
        ProofExprKind::Binary { left, right, .. } => {
            assert_expr_well_formed(left);
            assert_expr_well_formed(right);
        }
        ProofExprKind::Call { callee, args } => {
            assert_expr_well_formed(callee);
            for arg in args {
                assert_expr_well_formed(arg);
            }
        }
        ProofExprKind::Index { target, index } => {
            assert_expr_well_formed(target);
            assert_expr_well_formed(index);
        }
    }
}

fn binding_id(expr: &ProofExpr) -> HirBindingId {
    match expr.kind {
        ProofExprKind::Binding(id) => id,
        ref other => panic!("expected binding proof expression, got {other:?}"),
    }
}

//= PROOF_IR.md#llg-pir-01-pipeline-and-lowering
//= type=test
//# Successfully checked HIR MUST lower into Proof IR before proof obligation discharge runs.
#[test]
fn requirement_llg_pir_01_lowers_checked_hir_into_proof_ir_before_discharge() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    values[index];
}
"#,
    );

    let proof_ir = proof_ir(&proof);
    assert_eq!(proof_ir.functions.len(), 1);
    assert!(proof_entries(&proof).iter().any(|entry| {
        matches!(
            entry,
            ProofEntry::Obligation {
                kind: ProofObligationKind::InBounds { .. },
                ..
            }
        )
    }));
}

//= SPEC.md#llg-proof-01-proof-required-operations
//= type=test
//# Proof checking MUST traverse task bodies, including `forever` bodies, `exit` values, and `delegate` arguments.
#[test]
fn requirement_llg_proof_01_checks_indexing_inside_task_bodies() {
    let (_, proven) = check_ok(
        r#"
task main(values: [u32; 4], index: u32) -> u32 {
    forever {
        observe index < 4 else {
            exit 1;
        }
        values[index];
    }
}
"#,
    );
    assert_eq!(proof_ir(&proven).functions.len(), 1);

    let (checked, unproven_forever) = check_err(
        r#"
task main(values: [u32; 4], index: u32) -> u32 {
    forever {
        values[index];
    }
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &unproven_forever,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );

    let (checked, unproven_exit) = check_err(
        r#"
task main(values: [u32; 4], index: u32) -> u32 {
    exit values[index];
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &unproven_exit,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );

    let (checked, unproven_delegate) = check_err(
        r#"
task worker(value: u32) -> u32 {
    exit value;
}

task main(values: [u32; 4], index: u32) -> u32 {
    delegate worker(values[index]);
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &unproven_delegate,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
}

//= PROOF_IR.md#llg-pir-01-pipeline-and-lowering
//= type=test
//# Every Proof IR node MUST preserve a source span sufficient for diagnostics and traceability.
#[test]
fn requirement_llg_pir_01_preserves_proof_ir_source_spans() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    } else {
        values[4];
    }
}
"#,
    );

    for function in &proof_ir(&proof).functions {
        assert_non_empty_span(function.span, "ProofFunction");
        assert_non_empty_span(function.body.span, "ProofBlock");
    }
    for entry in proof_entries(&proof) {
        assert_non_empty_span(entry.span(), "ProofEntry");
        match entry {
            ProofEntry::Branch {
                condition, facts, ..
            } => {
                assert_expr_spans(condition);
                for fact in facts {
                    assert_non_empty_span(fact.origin_span, "ProofRelation origin");
                    assert_non_empty_span(fact.left_span, "ProofRelation left");
                    assert_non_empty_span(fact.right_span, "ProofRelation right");
                    assert_expr_spans(&fact.right);
                }
            }
            ProofEntry::Observe {
                left, right, fact, ..
            } => {
                assert_expr_spans(left);
                assert_expr_spans(right);
                assert_non_empty_span(fact.origin_span, "ProofRelation origin");
            }
            ProofEntry::For { iterable, .. } | ProofEntry::Eval { expr: iterable, .. } => {
                assert_expr_spans(iterable);
            }
            ProofEntry::Obligation { kind, .. } => match kind {
                ProofObligationKind::InBounds { target, index, .. } => {
                    assert_expr_spans(target);
                    assert_expr_spans(index);
                }
                ProofObligationKind::MapPresence { target, key } => {
                    assert_expr_spans(target);
                    assert_expr_spans(key);
                }
            },
            ProofEntry::Scope { block, .. } => assert_non_empty_span(block.span, "ProofBlock"),
        }
    }
}

//= PROOF_IR.md#llg-pir-02-fact-subjects-and-stability
//= type=test
//# Every proof fact subject in Proof IR MUST reference binding identity rather than identifier text.
#[test]
fn requirement_llg_pir_02_references_binding_identity_for_fact_subjects() {
    let (_, proof) = check_err(
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

    let observe_subject = proof_entries(&proof)
        .iter()
        .find_map(|entry| match entry {
            ProofEntry::Observe { fact, .. } => fact.subject,
            _ => None,
        })
        .expect("expected observe fact subject");
    let obligation_index = proof_entries(&proof)
        .iter()
        .find_map(|entry| match entry {
            ProofEntry::Obligation {
                kind: ProofObligationKind::InBounds { index, .. },
                ..
            } => Some(binding_id(index)),
            _ => None,
        })
        .expect("expected indexed binding");

    assert_ne!(
        observe_subject, obligation_index,
        "shadowed bindings must not collapse to identifier text"
    );
}

//= PROOF_IR.md#llg-pir-02-fact-subjects-and-stability
//= type=test
//# Proof IR MUST distinguish stable facts from mutable diagnostic-only hints so mutable comparisons cannot discharge obligations.
#[test]
fn requirement_llg_pir_02_distinguishes_stable_facts_from_mutable_hints() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 {
        values[index];
    }
}
"#,
    );

    let branch = proof_entries(&proof)
        .into_iter()
        .find_map(|entry| match entry {
            ProofEntry::Branch {
                facts,
                mutable_hints,
                ..
            } => Some((facts, mutable_hints)),
            _ => None,
        })
        .expect("expected branch Proof IR entry");

    assert!(branch.0.is_empty(), "mutable fact should not be stable");
    assert_eq!(branch.1.len(), 1);
    assert!(!branch.1[0].stable);
}

//= PROOF_IR.md#llg-pir-03-obligations-and-fact-sources
//= type=test
//# Proof-required operations, including indexing and map-presence checks, MUST lower to explicit proof obligations that preserve the originating operation span.
#[test]
fn requirement_llg_pir_03_lowers_operations_to_explicit_proof_obligations() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4], table: Map<u32, bool, 16>, index: u32) {
    values[index];
    table[index];
}
"#,
    );

    let mut has_in_bounds = false;
    let mut has_map_presence = false;
    for entry in proof_entries(&proof) {
        if let ProofEntry::Obligation { kind, span } = entry {
            assert_non_empty_span(*span, "Proof obligation");
            match kind {
                ProofObligationKind::InBounds { .. } => has_in_bounds = true,
                ProofObligationKind::MapPresence { .. } => has_map_presence = true,
            }
        }
    }

    assert!(has_in_bounds, "expected array-indexing obligation");
    assert!(has_map_presence, "expected map-presence obligation");
}

//= PROOF_IR.md#llg-pir-03-obligations-and-fact-sources
//= type=test
//# Successful `observe` statements and comparison-based control-flow tests MUST lower to explicit fact-producing nodes that preserve the originating relation spans.
#[test]
fn requirement_llg_pir_03_lowers_observe_and_control_flow_tests_to_fact_nodes() {
    let (_, proof) = check_ok(
        r#"
fn main(value: u32) {
    observe value < 4 else {
        return;
    }
    if value != 0 && value <= 4 {
        value;
    }
}
"#,
    );

    let entries = proof_entries(&proof);
    assert!(entries.iter().any(|entry| matches!(
        entry,
        ProofEntry::Observe {
            fact,
            ..
        } if fact.source == FactSource::Observe && !fact.left_span.is_empty() && !fact.right_span.is_empty()
    )));
    assert!(entries.iter().any(|entry| matches!(
        entry,
        ProofEntry::Branch {
            facts,
            ..
        } if facts.len() == 2 && facts.iter().all(|fact| fact.source == FactSource::ControlFlow
            && !fact.left_span.is_empty()
            && !fact.right_span.is_empty())
    )));
}

//= PROOF_IR.md#llg-pir-04-normalization-boundary
//= type=test
//# Proof IR MUST retain only proof-relevant control flow, obligations, fact sources, and proof expressions; non-proof statements MAY be omitted unless needed to preserve proof scope.
#[test]
fn requirement_llg_pir_04_retains_only_proof_relevant_structure() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    let ignored = 1;
    ignored;
    if index < 4 {
        values[index];
    }
}
"#,
    );

    let entries = proof_entries(&proof);
    assert!(entries
        .iter()
        .any(|entry| matches!(entry, ProofEntry::Branch { .. })));
    assert!(entries
        .iter()
        .any(|entry| matches!(entry, ProofEntry::Obligation { .. })));
    assert!(entries
        .iter()
        .all(|entry| !matches!(entry, ProofEntry::Eval { expr, .. } if matches!(expr.kind, ProofExprKind::Int(1)))));
}

//= PROOF_IR.md#llg-pir-04-normalization-boundary
//= type=test
//# Grouped expressions and other parser- or HIR-only wrapper nodes MUST NOT survive as distinct Proof IR nodes.
#[test]
fn requirement_llg_pir_04_removes_grouping_and_wrapper_nodes() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    values[(index)];
}
"#,
    );

    let index_expr = proof_entries(&proof)
        .iter()
        .find_map(|entry| match entry {
            ProofEntry::Obligation {
                kind: ProofObligationKind::InBounds { index, .. },
                ..
            } => Some(index),
            _ => None,
        })
        .expect("expected index proof expression");
    assert!(matches!(index_expr.kind, ProofExprKind::Binding(_)));
}

//= PROOF_IR.md#llg-pir-05-successful-proof-ir-well-formedness
//= type=test
//# Successfully lowered Proof IR MUST NOT contain unresolved names, identifier-text fact subjects, or `Unknown` or otherwise untyped proof expressions.
#[test]
fn requirement_llg_pir_05_excludes_unresolved_names_identifier_text_and_untyped_exprs() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    observe index < 4 else {
        return;
    }
    values[index];
}
"#,
    );

    for entry in proof_entries(&proof) {
        match entry {
            ProofEntry::Branch {
                condition,
                facts,
                mutable_hints,
                ..
            } => {
                assert_expr_well_formed(condition);
                for fact in facts.iter().chain(mutable_hints.iter()) {
                    assert!(fact.subject.is_some());
                    assert_expr_well_formed(&fact.right);
                }
            }
            ProofEntry::Observe {
                left, right, fact, ..
            } => {
                assert_expr_well_formed(left);
                assert_expr_well_formed(right);
                assert!(fact.subject.is_some());
                assert_expr_well_formed(&fact.right);
            }
            ProofEntry::For { iterable, .. } | ProofEntry::Eval { expr: iterable, .. } => {
                assert_expr_well_formed(iterable);
            }
            ProofEntry::Obligation { kind, .. } => match kind {
                ProofObligationKind::InBounds { target, index, .. } => {
                    assert_expr_well_formed(target);
                    assert_expr_well_formed(index);
                }
                ProofObligationKind::MapPresence { target, key } => {
                    assert_expr_well_formed(target);
                    assert_expr_well_formed(key);
                }
            },
            ProofEntry::Scope { .. } => {}
        }
    }
}

//= PROOF_IR.md#llg-pir-05-successful-proof-ir-well-formedness
//= type=test
//# Every proof obligation and fact in successfully lowered Proof IR MUST be attributable to a source span in the originating HIR.
#[test]
fn requirement_llg_pir_05_preserves_source_attribution_for_obligations_and_facts() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    }
}
"#,
    );

    for entry in proof_entries(&proof) {
        match entry {
            ProofEntry::Branch {
                facts,
                mutable_hints,
                ..
            } => {
                for fact in facts.iter().chain(mutable_hints.iter()) {
                    assert_non_empty_span(fact.origin_span, "fact origin");
                    assert_non_empty_span(fact.left_span, "fact left");
                    assert_non_empty_span(fact.right_span, "fact right");
                }
            }
            ProofEntry::Observe { fact, .. } => {
                assert_non_empty_span(fact.origin_span, "fact origin");
                assert_non_empty_span(fact.left_span, "fact left");
                assert_non_empty_span(fact.right_span, "fact right");
            }
            ProofEntry::Obligation { span, .. } => {
                assert_non_empty_span(*span, "obligation origin");
            }
            ProofEntry::For { .. } | ProofEntry::Eval { .. } | ProofEntry::Scope { .. } => {}
        }
    }
}

//= SPEC.md#llg-rel-01-collections-and-relations
//= type=test
//# The first enforced relation MUST allow a key introduced by iterating a `Set<K, N>` to imply presence in a `Map<K, V, M>`.
#[test]
fn requirement_llg_rel_01_propagates_set_membership_to_map_presence() {
    let (_, proven) = check_ok(
        r#"
fn main(keys: Set<u32, 16>, table: Map<u32, bool, 32>) {
    for key in keys {
        table[key];
    }
}
"#,
    );
    assert_eq!(proven.obligations, 1);
    assert!(proof_entries(&proven).iter().any(|entry| matches!(
        entry,
        ProofEntry::For {
            membership: Some(_),
            ..
        }
    )));

    let (checked, copied_key) = check_err(
        r#"
fn main(keys: Set<u32, 16>, table: Map<u32, bool, 32>) {
    for key in keys {
        let copied = key;
        table[copied];
    }
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &copied_key,
        "possible missing map key is not proven present",
        "copied",
    );

    let (checked, unproven) = check_err(
        r#"
fn main(key: u32, table: Map<u32, bool, 32>) {
    table[key];
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &unproven,
        "possible missing map key is not proven present",
        "key",
    );
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
    if limit <= total && baseline > 0 {
        limit;
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

    assert_fact(
        &checked,
        &proof,
        FactSource::ControlFlow,
        "limit",
        ObserveOp::LtEq,
        "total",
    );
    assert_fact(
        &checked,
        &proof,
        FactSource::ControlFlow,
        "baseline",
        ObserveOp::Gt,
        "0",
    );

    // Only the four comparison tests should be recorded for this example.
    assert_eq!(control_flow_facts, 4);
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

//= SEMANTICS.md#llg-sem-06-raw-arithmetic-reservation
//= type=test
//# Future raw or proof-backed arithmetic MUST be explicit at the operation site and MUST NOT be inferred from ordinary arithmetic operators.
#[test]
fn requirement_llg_sem_06_does_not_infer_raw_arithmetic_from_ordinary_operators() {
    let (_, proof) = check_ok(
        r#"
fn main(left: u32, right: u32) -> u32 {
    left + right or(err) 0
}
"#,
    );

    assert_eq!(proof.obligations, 0);
    assert!(!proof.has_errors(), "{:#?}", proof.diagnostics);
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
