use langlog_proof::{
    check, CheckedProof, MarkerFactSource, MarkerPattern, ObligationSource, ProofBlock, ProofEntry,
    ProofExpr, ProofExprKind, ProofMarkerRuleStmt, ProofMarkerTemplateArg, ProofProgram,
};
use langlog_sema::HirMarkerFamily;
use langlog_sema::{analyze, CheckedProgram, HirType};
use langlog_syntax::{parse, LabelStyle, Severity};

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

fn assert_primary_diagnostic(
    checked: &CheckedProgram,
    proof: &CheckedProof,
    message: &str,
    expected_span_text: &str,
) {
    assert!(proof.diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == Severity::Error
            && diagnostic.message == message
            && diagnostic.labels.iter().any(|label| {
                label.style == LabelStyle::Primary
                    && checked.parsed.source.span_text(label.span) == Some(expected_span_text)
            })
    }));
}

fn assert_note_contains(proof: &CheckedProof, text: &str) {
    assert!(
        proof
            .diagnostics
            .iter()
            .flat_map(|diagnostic| diagnostic.notes.iter())
            .any(|note| note.contains(text)),
        "missing diagnostic note containing {text:?}: {:#?}",
        proof.diagnostics
    );
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
            ProofEntry::Recover { fallback, .. } => collect_entries(&fallback.block, entries),
            ProofEntry::For { body, .. } | ProofEntry::Scope { block: body, .. } => {
                collect_entries(body, entries);
            }
            ProofEntry::Let { .. }
            | ProofEntry::Assign { .. }
            | ProofEntry::Fact { .. }
            | ProofEntry::Obligation { .. }
            | ProofEntry::Eval { .. } => {}
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
        | ProofExprKind::Item(_)
        | ProofExprKind::HostBuiltin(_)
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
        ProofExprKind::Recover { result, fallback } => {
            assert_expr_spans(result);
            assert_expr_spans(fallback);
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
        ProofExprKind::UnsafeMarker { args, .. } => {
            for arg in args {
                assert_expr_spans(arg);
            }
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

fn assert_expr_well_formed(expr: &ProofExpr, proof: &CheckedProof) {
    assert_type_has_no_named_unknown(&expr.ty);
    assert!(
        expr.place.index < proof_ir(proof).places.len(),
        "ProofExpr should reference an existing place"
    );
    match &expr.kind {
        ProofExprKind::Binding(_)
        | ProofExprKind::Item(_)
        | ProofExprKind::HostBuiltin(_)
        | ProofExprKind::Int(_)
        | ProofExprKind::Bool(_) => {}
        ProofExprKind::Tuple(elements) | ProofExprKind::Array(elements) => {
            for element in elements {
                assert_expr_well_formed(element, proof);
            }
        }
        ProofExprKind::Unary { expr } => assert_expr_well_formed(expr, proof),
        ProofExprKind::Binary { left, right, .. } => {
            assert_expr_well_formed(left, proof);
            assert_expr_well_formed(right, proof);
        }
        ProofExprKind::Recover { result, fallback } => {
            assert_expr_well_formed(result, proof);
            assert_expr_well_formed(fallback, proof);
        }
        ProofExprKind::Call { callee, args } => {
            assert_expr_well_formed(callee, proof);
            for arg in args {
                assert_expr_well_formed(arg, proof);
            }
        }
        ProofExprKind::Index { target, index } => {
            assert_expr_well_formed(target, proof);
            assert_expr_well_formed(index, proof);
        }
        ProofExprKind::UnsafeMarker { args, .. } => {
            for arg in args {
                assert_expr_well_formed(arg, proof);
            }
        }
    }
}

fn has_fact(proof: &CheckedProof, predicate: impl Fn(&MarkerPattern) -> bool) -> bool {
    proof.facts.iter().any(|fact| predicate(&fact.marker))
}

//= PROOF_IR.md#llg-pir-01-pipeline-and-lowering
//= type=test
//# Successfully checked HIR MUST lower into Proof IR before marker-obligation discharge runs.
#[test]
fn requirement_llg_pir_01_lowers_checked_hir_into_marker_proof_ir_before_discharge() {
    let (_, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    values[index];
}
"#,
    );

    let proof_ir = proof_ir(&proof);
    assert_eq!(proof_ir.functions.len(), 1);
    assert!(!proof_ir.places.is_empty());
    assert!(proof_entries(&proof).iter().any(|entry| {
        matches!(
            entry,
            ProofEntry::Obligation {
                obligation,
                ..
            } if matches!(obligation.source, ObligationSource::Index { .. })
                && matches!(obligation.required, MarkerPattern::LessThan { .. })
        )
    }));
}

//= SPEC.md#llg-proof-01-marker-required-operations
//= type=test
//# Marker checking MUST traverse task bodies, including `forever` bodies, `exit` values, and `delegate` arguments.
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
fn requirement_llg_pir_02_preserves_marker_proof_ir_source_spans() {
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

    for place in &proof_ir(&proof).places {
        assert_non_empty_span(place.span, "ProofPlace");
    }
    for function in &proof_ir(&proof).functions {
        assert_non_empty_span(function.span, "ProofFunction");
        assert_non_empty_span(function.body.span, "ProofBlock");
    }
    for entry in proof_entries(&proof) {
        assert_non_empty_span(entry.span(), "ProofEntry");
        match entry {
            ProofEntry::Let { .. } | ProofEntry::Assign { .. } | ProofEntry::Fact { .. } => {}
            ProofEntry::Branch {
                condition,
                then_facts,
                else_facts,
                ..
            } => {
                assert_expr_spans(condition);
                for fact in then_facts.iter().chain(else_facts.iter()) {
                    assert_non_empty_span(fact.origin_span, "MarkerFact origin");
                }
            }
            ProofEntry::Observe {
                left, right, facts, ..
            } => {
                assert_expr_spans(left);
                assert_expr_spans(right);
                for fact in facts {
                    assert_non_empty_span(fact.origin_span, "MarkerFact origin");
                }
            }
            ProofEntry::Recover {
                result, fallback, ..
            } => {
                assert_expr_spans(result);
                assert_expr_spans(&fallback.value);
                assert_non_empty_span(fallback.block.span, "ProofBlock");
            }
            ProofEntry::For { iterable, .. } | ProofEntry::Eval { expr: iterable, .. } => {
                assert_expr_spans(iterable);
            }
            ProofEntry::Obligation { obligation, span } => {
                assert_non_empty_span(*span, "MarkerObligation entry");
                assert_non_empty_span(obligation.span, "MarkerObligation origin");
            }
            ProofEntry::Scope { block, .. } => assert_non_empty_span(block.span, "ProofBlock"),
        }
    }
}

//= PROOF_IR.md#llg-pir-03-marker-obligations-and-fact-sources
//= type=test
//# Marker-required operations, including indexing and map-presence checks, MUST lower to explicit marker obligations that preserve the originating operation span.
#[test]
fn requirement_llg_pir_03_lowers_operations_to_marker_obligations() {
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
        if let ProofEntry::Obligation { obligation, .. } = entry {
            match (&obligation.source, &obligation.required) {
                (ObligationSource::Index { .. }, MarkerPattern::LessThan { .. }) => {
                    has_in_bounds = true
                }
                (ObligationSource::MapLookup { .. }, MarkerPattern::MemberOf { .. }) => {
                    has_map_presence = true
                }
                _ => {}
            }
        }
    }

    assert!(has_in_bounds, "expected array-indexing marker obligation");
    assert!(has_map_presence, "expected map-presence marker obligation");
}

//= PROOF_IR.md#llg-pir-03-marker-obligations-and-fact-sources
//= type=test
//# Marker fact sources MUST include control-flow truth markers, successful `observe` statements, unsafe marker construction, companion-rule implications, assignment identity, and immutable marker carry-forward.
#[test]
fn requirement_llg_pir_03_lowers_observe_and_control_flow_tests_to_marker_facts() {
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

    assert!(proof_entries(&proof).iter().any(|entry| matches!(
        entry,
        ProofEntry::Observe {
            facts,
            ..
        } if facts.iter().any(|fact| fact.source == MarkerFactSource::Observe)
    )));
    assert!(proof_entries(&proof).iter().any(|entry| matches!(
        entry,
        ProofEntry::Branch {
            then_facts,
            ..
        } if then_facts.iter().any(|fact| fact.source == MarkerFactSource::ControlFlowTruth)
            && then_facts.iter().any(|fact| matches!(fact.marker, MarkerPattern::True { .. }))
    )));
    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::CompanionRule
            && matches!(fact.marker, MarkerPattern::LessThan { .. })
    }));
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Companion marker rules MUST lower refinement-pattern bindings and implications into Proof IR marker-rule templates.
#[test]
fn requirement_llg_mark_06_lowers_companion_rule_patterns_into_proof_ir() {
    let (_, proof) = check_ok(
        r#"
mark LessThan(a: place, b: place, result: place) {
    if a with LessThan(a, ?bound) {
        implies LessThan(result, bound) for result;
    }
}
"#,
    );

    let proof_ir = proof_ir(&proof);
    assert_eq!(proof_ir.marker_rules.len(), 1);
    let rule = &proof_ir.marker_rules[0];
    assert_eq!(rule.name, "LessThan");

    let [ProofMarkerRuleStmt::If(stmt)] = rule.body.statements.as_slice() else {
        panic!("expected one marker refinement rule");
    };
    assert_eq!(stmt.subject, "a");
    assert_eq!(stmt.marker.family, HirMarkerFamily::LessThan);
    assert_eq!(
        stmt.marker.args[1],
        ProofMarkerTemplateArg::Binding("bound".to_owned())
    );

    let [ProofMarkerRuleStmt::Implies(implication)] = stmt.body.statements.as_slice() else {
        panic!("expected one marker implication");
    };
    assert_eq!(implication.target, "result");
    assert_eq!(
        implication.marker.args[1],
        ProofMarkerTemplateArg::Place("bound".to_owned())
    );
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Control-flow comparison marker facts MUST be emitted as companion-rule implications.
#[test]
fn requirement_llg_mark_06_emits_comparison_facts_as_companion_rule_facts() {
    let (_, proof) = check_ok(
        r#"
fn main(left: u32, right: u32) {
    if left < right {
        left;
    }
}
"#,
    );

    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::CompanionRule
            && matches!(fact.marker, MarkerPattern::LessThan { .. })
    }));
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Trusted builtin comparison companion rules MUST be active by default.
#[test]
fn requirement_llg_mark_06_uses_trusted_builtin_companion_rules_by_default() {
    let (_, less_than) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    }
}
"#,
    );
    assert_eq!(less_than.obligations, 1);

    let (_, inverted_else) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index >= 4 {
        return;
    } else {
        values[index];
    }
}
"#,
    );
    assert_eq!(inverted_else.obligations, 1);

    let (_, normalized_less_or_equal) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index <= 3 {
        values[index];
    }
}
"#,
    );
    assert_eq!(normalized_less_or_equal.obligations, 1);

    let (_, observed) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    observe index < 4 else {
        return;
    }
    values[index];
}
"#,
    );
    assert_eq!(observed.obligations, 1);
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# For `u32` literal bounds, the marker checker MAY normalize `LessOrEqual(left, N)` into `LessThan(left, N + 1)` when `N + 1` is representable and available as a place.
#[test]
fn requirement_llg_mark_06_normalizes_less_or_equal_successor_bounds() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index <= 3 {
        values[index];
    }
}
"#,
    );

    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::TrustedBuiltin
            && matches!(
                fact.marker,
                MarkerPattern::LessThan { right, .. }
                    if proof_ir(&proof).places[right.index].value == Some(langlog_proof::PlaceValue::U32(4))
            )
    }));
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# A source companion marker rule with a builtin companion name MUST override the trusted builtin rule with the same name.
#[test]
fn requirement_llg_mark_06_source_companion_rules_override_builtin_rules() {
    let (checked, missing_direct_fact) = check_err(
        r#"
mark LessThan(a: place, b: place, result: place) {
    if result with True() {
        implies GreaterThan(b, a) for b;
    }
}

fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    }
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &missing_direct_fact,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );

    let (_, restored_direct_fact) = check_ok(
        r#"
mark LessThan(a: place, b: place, result: place) {
    if result with True() {
        implies LessThan(a, b) for a;
    }
}

fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    }
}
"#,
    );
    assert_eq!(restored_direct_fact.obligations, 1);

    let (checked, missing_sub_fact) = check_err(
        r#"
mark Sub(a: place, amount: place, result: place) {}

fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let fallback = 0;
        unsafe { LessThan::mark(fallback, 4); }
        let smaller = index - 1 or(err) fallback;
        values[smaller];
    }
}
"#,
    );
    assert_primary_diagnostic(
        &checked,
        &missing_sub_fact,
        "possible out-of-bounds indexing is not proven safe",
        "smaller",
    );

    let (_, restored_sub_fact) = check_ok(
        r#"
mark Sub(a: place, amount: place, result: place) {
    if a with LessThan(a, ?bound) {
        implies LessThan(result, bound) for result;
    }
}

fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let fallback = 0;
        unsafe { LessThan::mark(fallback, 4); }
        let smaller = index - 1 or(err) fallback;
        values[smaller];
    }
}
"#,
    );
    assert_eq!(restored_sub_fact.obligations, 1);
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# The trusted builtin `Sub` companion rule MUST preserve `LessThan(result, bound)` from `LessThan(a, bound)` for the successful checked subtraction payload.
#[test]
fn requirement_llg_mark_06_trusted_sub_rule_preserves_success_bounds() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let fallback = 0;
        unsafe { LessThan::mark(fallback, 4); }
        let smaller = index - 1 or(err) fallback;
        values[smaller];
    }
}
"#,
    );

    assert_eq!(proof.obligations, 1);
    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::CompanionRule
            && matches!(fact.marker, MarkerPattern::LessThan { .. })
    }));
}

//= PROOF_IR.md#llg-pir-03-marker-obligations-and-fact-sources
//= type=test
//# Checked subtraction lowers to a successful payload place that can receive marker facts from the active `Sub` companion rule.
#[test]
fn requirement_llg_pir_03_checked_sub_lowers_success_payload_place() {
    let (_, proof) = check_ok(
        r#"
fn main(index: u32) {
    let fallback = 0;
    let smaller = index - 1 or(err) fallback;
}
"#,
    );

    assert!(proof_entries(&proof).iter().any(|entry| {
        matches!(
            entry,
            ProofEntry::Recover {
                result: ProofExpr {
                    kind: ProofExprKind::Binary {
                        op: langlog_syntax::ast::BinaryOp::Sub,
                        success_place: Some(_),
                        ..
                    },
                    ..
                },
                ..
            }
        )
    }));
}

//= PROOF_IR.md#llg-pir-03-marker-obligations-and-fact-sources
//= type=test
//# Result recovery lowers to separate success and fallback marker paths, and the recovered place receives only marker facts proven on both paths.
#[test]
fn requirement_llg_pir_03_recovery_keeps_only_common_success_and_fallback_markers() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let fallback = 0;
        let smaller = index - 1 or(err) fallback;
        values[smaller];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "smaller",
    );
    assert!(!proof
        .facts
        .iter()
        .any(|fact| fact.source == MarkerFactSource::RecoveryMerge));
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# `Sub` marker transfer MUST apply only to direct checked `u32 - u32` subtraction success payloads in this slice.
#[test]
fn requirement_llg_mark_06_sub_transfer_excludes_result_lifted_subtraction() {
    let (checked, proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let checked: Result<u32, ArithmeticError> = ok(index);
        let fallback = 0;
        unsafe { LessThan::mark(fallback, 4); }
        let smaller = checked - 1 or(err) fallback;
        values[smaller];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "smaller",
    );
}

//= PROOF_IR.md#llg-pir-03-marker-obligations-and-fact-sources
//= type=test
//# Marker facts that survive result recovery merging MUST use a recovery-merge fact source.
#[test]
fn requirement_llg_pir_03_recovery_merge_facts_use_recovery_merge_source() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let fallback = 0;
        unsafe { LessThan::mark(fallback, 4); }
        let smaller = index - 1 or(err) fallback;
        values[smaller];
    }
}
"#,
    );

    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::RecoveryMerge
            && matches!(fact.marker, MarkerPattern::LessThan { .. })
    }));
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# The condition succeeds only if the current marker environment already contains a matching marker attached to `a`; it MUST NOT create the marker.
#[test]
fn requirement_llg_mark_06_evaluates_marker_pattern_bindings_in_source_rules() {
    let (_, proof) = check_ok(
        r#"
mark LessThan(a: place, b: place, result: place) {
    if a with LessThan(a, ?bound) {
        implies LessThan(a, bound) for a;
    }
}

fn main(index: u32) {
    unsafe { LessThan::mark(index, 4); }
    if index < 10 {
        index;
    }
}
"#,
    );

    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::CompanionRule
            && matches!(
                fact.marker,
                MarkerPattern::LessThan { right, .. }
                    if proof_ir(&proof).places[right.index].value == Some(langlog_proof::PlaceValue::U32(4))
            )
    }));
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Observe comparisons MUST use the same companion rule semantics as `if` conditions.
#[test]
fn requirement_llg_mark_06_observe_uses_overridden_companion_rules() {
    let (checked, proof) = check_err(
        r#"
mark LessThan(a: place, b: place, result: place) {
    if result with True() {
        implies GreaterThan(b, a) for b;
    }
}

fn main(values: [u32; 4], index: u32) {
    observe index < 4 else {
        return;
    }
    values[index];
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

//= PROOF_IR.md#llg-pir-05-successful-proof-ir-well-formedness
//= type=test
//# Successfully lowered Proof IR MUST NOT contain unresolved names, identifier-text marker targets, unresolved marker patterns, or `Unknown` or otherwise untyped marker expressions.
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
            ProofEntry::Branch { condition, .. } => assert_expr_well_formed(condition, &proof),
            ProofEntry::Observe { left, right, .. } => {
                assert_expr_well_formed(left, &proof);
                assert_expr_well_formed(right, &proof);
            }
            ProofEntry::Recover {
                result, fallback, ..
            } => {
                assert_expr_well_formed(result, &proof);
                assert_expr_well_formed(&fallback.value, &proof);
            }
            ProofEntry::For { iterable, .. } | ProofEntry::Eval { expr: iterable, .. } => {
                assert_expr_well_formed(iterable, &proof);
            }
            ProofEntry::Let { .. }
            | ProofEntry::Assign { .. }
            | ProofEntry::Fact { .. }
            | ProofEntry::Obligation { .. }
            | ProofEntry::Scope { .. } => {}
        }
    }
}

//= SPEC.md#llg-rel-01-collections-and-relations
//= type=test
//# A key introduced by iterating a `Set<K, N>` MAY imply a `MemberOf(key, map)` marker for a related `Map<K, V, M>` only when the relation has been declared by the language or a trusted builtin rule.
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
    assert!(has_fact(&proven, |marker| matches!(
        marker,
        MarkerPattern::MemberOf { .. }
    )));

    let (_, copied_key) = check_ok(
        r#"
fn main(keys: Set<u32, 16>, table: Map<u32, bool, 32>) {
    for key in keys {
        let copied = key;
        table[copied];
    }
}
"#,
    );
    assert_eq!(copied_key.obligations, 1);

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
    assert_note_contains(&unproven, "required marker: MemberOf");
}

//= SPEC.md#llg-proof-02-marker-introduction-and-discharge
//= type=test
//# Marker obligations MUST be discharged only by a direct marker match, possibly after applying declared companion marker transfer rules.
#[test]
fn requirement_llg_proof_02_discharges_array_bounds_with_direct_less_than_markers() {
    let (_, literal_safe) = check_ok(
        r#"
fn main() {
    [10, 20, 30][2];
}
"#,
    );
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
    assert_eq!(observed_safe.obligations, 1);
    assert!(has_fact(&observed_safe, |marker| matches!(
        marker,
        MarkerPattern::LessThan { .. }
    )));

    let (_, control_flow_safe) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        values[index];
    }
}
"#,
    );
    assert_eq!(control_flow_safe.obligations, 1);

    let (_, normalized_safe) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index <= 3 {
        values[index];
    }
}
"#,
    );
    assert_eq!(normalized_safe.obligations, 1);
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Control flow MUST mark the condition result with `True()` in the then branch and `False()` in the else branch.
#[test]
fn requirement_llg_mark_06_inverts_simple_comparison_facts_for_else_branch() {
    let (_, proof) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index >= 4 {
        return;
    } else {
        values[index];
    }
}
"#,
    );

    assert_eq!(proof.obligations, 1);
    assert!(proof.facts.iter().any(|fact| {
        fact.source == MarkerFactSource::CompanionRule
            && matches!(fact.marker, MarkerPattern::LessThan { .. })
    }));
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# `Equal(left, right)` MUST mark that `left` is equal to `right`.
#[test]
fn requirement_llg_mark_04_introduces_equal_marker_facts() {
    let (_, proof) = check_ok(
        r#"
fn main(left: u32, right: u32) {
    if left == right {
        left;
    }
}
"#,
    );

    assert!(has_fact(&proof, |marker| matches!(
        marker,
        MarkerPattern::Equal { .. }
    )));
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# The trusted `read_u32()` host builtin MUST return a value marked with `Event`.
#[test]
fn requirement_llg_mark_04_read_u32_produces_event_marker() {
    let (_, proof) = check_ok(
        r#"
fn main() {
    let value: u32 with Event = read_u32();
}
"#,
    );

    assert_eq!(proof.obligations, 1);
    assert!(proof
        .facts
        .iter()
        .any(|fact| fact.source == MarkerFactSource::TrustedBuiltin
            && matches!(fact.marker, MarkerPattern::Event)));
}

//= SPEC.md#llg-mark-01-marker-model
//= type=test
//# A value without a required marker fact MUST NOT be used where that marker is required.
#[test]
fn requirement_llg_mark_01_rejects_marker_qualified_let_without_required_fact() {
    let (checked, proof) = check_err(
        r#"
fn main() {
    let value: u32 with Event = 1;
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "required marker is not proven for this value",
        "let value: u32 with Event = 1;",
    );
    assert_note_contains(&proof, "required marker: Event");
}

//= SPEC.md#llg-mark-02-function-boundaries
//= type=test
//# A marker-qualified function parameter MUST create a call-site obligation for each required marker on the corresponding argument.
#[test]
fn requirement_llg_mark_02_requires_call_site_markers_for_marker_qualified_parameters() {
    check_ok(
        r#"
fn needs_event(value: u32 with Event) {}

fn main() {
    needs_event(read_u32());
}
"#,
    );

    let (_, proof) = check_err(
        r#"
fn needs_event(value: u32 with Event) {}

fn main() {
    needs_event(1);
}
"#,
    );

    assert_note_contains(&proof, "required marker: Event");
}

//= SPEC.md#llg-mark-02-function-boundaries
//= type=test
//# A marker-qualified return type MUST require the returned expression to provide each named marker and MUST provide those markers to callers after the call succeeds.
#[test]
fn requirement_llg_mark_02_function_returns_only_explicitly_declared_markers() {
    let (_, stripped) = check_err(
        r#"
fn strip(value: u32 with Event) -> u32 {
    value
}

fn main() {
    let value: u32 with Event = strip(read_u32());
}
"#,
    );
    assert_note_contains(&stripped, "required marker: Event");

    check_ok(
        r#"
fn keep(value: u32 with Event) -> u32 with Event {
    value
}

fn main() {
    let value: u32 with Event = keep(read_u32());
}
"#,
    );
}

//= SPEC.md#llg-mark-03-marker-construction
//= type=test
//# Code that creates a marker fact MUST do so inside an `unsafe` block.
#[test]
fn requirement_llg_mark_03_unsafe_marker_construction_emits_facts() {
    let (_, statement_proof) = check_ok(
        r#"
fn main() {
    let value: u32 = 1;
    unsafe { Event::mark(value); }
    let copied: u32 with Event = value;
}
"#,
    );
    assert!(statement_proof
        .facts
        .iter()
        .any(|fact| fact.source == MarkerFactSource::UnsafeConstruction
            && matches!(fact.marker, MarkerPattern::Event)));

    let (_, expression_proof) = check_ok(
        r#"
fn main() {
    let copied: u32 with Event = unsafe { Event::mark(1) };
}
"#,
    );
    assert!(expression_proof
        .facts
        .iter()
        .any(|fact| fact.source == MarkerFactSource::UnsafeConstruction
            && matches!(fact.marker, MarkerPattern::Event)));
}

//= SPEC.md#llg-proof-01-marker-required-operations
//= type=test
//# Array indexing MUST require a marker obligation equivalent to `index with LessThan(index, array.length)`.
#[test]
fn requirement_llg_proof_01_rejects_possible_out_of_bounds_indexing_without_marker() {
    let (checked, failing_proof) = check_err(
        r#"
fn main(values: [u32; 4], index: u32) {
    values[index];
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &failing_proof,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert_note_contains(&failing_proof, "required marker: LessThan");
    assert_note_contains(&failing_proof, "target place: index");

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

//= SPEC.md#llg-mark-05-marker-transfer
//= type=test
//# Assignment MUST preserve marker facts because it preserves place identity.
#[test]
fn requirement_llg_mark_05_assignment_copies_markers_and_mutation_creates_new_places() {
    let (_, copied_index) = check_ok(
        r#"
fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let copied = index;
        values[copied];
    }
}
"#,
    );
    assert_eq!(copied_index.obligations, 1);
    assert!(copied_index
        .facts
        .iter()
        .any(|fact| fact.source == MarkerFactSource::AssignmentIdentity));

    let (_, mutable_safe) = check_ok(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    if index < 4 {
        values[index];
    }
}
"#,
    );
    assert_eq!(mutable_safe.obligations, 1);
    assert!(
        !mutable_safe.has_warnings(),
        "{:#?}",
        mutable_safe.diagnostics
    );

    let (_, mutable_observe_safe) = check_ok(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    observe index < 4 else {
        return;
    }
    values[index];
}
"#,
    );
    assert_eq!(mutable_observe_safe.obligations, 1);
    assert!(mutable_observe_safe
        .facts
        .iter()
        .any(|fact| fact.source == MarkerFactSource::CompanionRule
            && matches!(fact.marker, MarkerPattern::LessThan { .. })));

    let (checked, stale_after_assignment) = check_err(
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
        &stale_after_assignment,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert_note_contains(&stale_after_assignment, "known near-miss marker");
    assert_note_contains(&stale_after_assignment, "index#1");
    assert!(!stale_after_assignment.has_warnings());

    let (checked, stale_after_observe_assignment) = check_err(
        r#"
fn main(values: [u32; 4]) {
    let mut index = 0;
    observe index < 4 else {
        return;
    }
    index = 4;
    values[index];
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &stale_after_observe_assignment,
        "possible out-of-bounds indexing is not proven safe",
        "index",
    );
    assert_note_contains(&stale_after_observe_assignment, "known near-miss marker");
    assert_note_contains(&stale_after_observe_assignment, "index#1");
    assert!(!stale_after_observe_assignment.has_warnings());
}

//= SPEC.md#llg-mark-02-function-boundaries
//= type=test
//# Function return values MUST carry only the marker facts named by the function signature.
#[test]
fn requirement_llg_mark_02_function_calls_do_not_preserve_unmentioned_markers() {
    let (checked, proof) = check_err(
        r#"
fn id(value: u32) -> u32 {
    value
}

fn main(values: [u32; 4], index: u32) {
    if index < 4 {
        let copied = id(index);
        values[copied];
    }
}
"#,
    );

    assert_primary_diagnostic(
        &checked,
        &proof,
        "possible out-of-bounds indexing is not proven safe",
        "copied",
    );
    assert_note_contains(&proof, "required marker: LessThan");
}
