use langlog_proof::{
    check, CheckedProof, MarkerFactSource, MarkerPattern, ObligationSource, ProofBlock, ProofEntry,
    ProofExpr, ProofExprKind, ProofProgram,
};
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
            ProofEntry::For { body, .. } | ProofEntry::Scope { block: body, .. } => {
                collect_entries(body, entries);
            }
            ProofEntry::Let { .. }
            | ProofEntry::Assign { .. }
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

fn assert_expr_well_formed(expr: &ProofExpr, proof: &CheckedProof) {
    assert_type_has_no_named_unknown(&expr.ty);
    assert!(
        expr.place.index < proof_ir(proof).places.len(),
        "ProofExpr should reference an existing place"
    );
    match &expr.kind {
        ProofExprKind::Binding(_)
        | ProofExprKind::Item
        | ProofExprKind::HostBuiltin
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
            ProofEntry::Let { .. } | ProofEntry::Assign { .. } => {}
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
            && then_facts.iter().any(|fact| matches!(fact.marker, MarkerPattern::LessThan { .. }))
    )));
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
            ProofEntry::For { iterable, .. } | ProofEntry::Eval { expr: iterable, .. } => {
                assert_expr_well_formed(iterable, &proof);
            }
            ProofEntry::Let { .. }
            | ProofEntry::Assign { .. }
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
        fact.source == MarkerFactSource::ControlFlowFalse
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
