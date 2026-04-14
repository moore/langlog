use langlog_sema::{analyze, BindingKind, CheckedProgram};
use langlog_syntax::ast::{Block, Expr, ExprKind, ForStmt, Item, LetStmt, Stmt};
use langlog_syntax::{parse, LabelStyle, Span};

fn analyze_ok(source: &str) -> CheckedProgram {
    let parsed = parse("requirement.llg", source);
    assert!(!parsed.has_errors(), "{:#?}", parsed.diagnostics);
    analyze(parsed)
}

fn function<'a>(checked: &'a CheckedProgram, name: &str) -> &'a langlog_syntax::Function {
    checked
        .parsed
        .module
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(function) if function.name.value == name => Some(function),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function {name:?}"))
}

fn name_span(expr: &Expr) -> Span {
    match &expr.kind {
        ExprKind::Name(name) => name.span,
        other => panic!("expected name expression, got {other:?}"),
    }
}

fn call_callee(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Call { callee, .. } => callee,
        other => panic!("expected call expression, got {other:?}"),
    }
}

fn let_stmt(block: &Block, index: usize) -> &LetStmt {
    match &block.statements[index] {
        Stmt::Let(stmt) => stmt,
        other => panic!("expected let statement at index {index}, got {other:?}"),
    }
}

fn expr_stmt(block: &Block, index: usize) -> &Expr {
    match &block.statements[index] {
        Stmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected expression statement at index {index}, got {other:?}"),
    }
}

fn block_expr(block: &Block, index: usize) -> &Block {
    match &expr_stmt(block, index).kind {
        ExprKind::Block(block) => block,
        other => panic!("expected block expression at index {index}, got {other:?}"),
    }
}

fn first_for_stmt(block: &Block) -> &ForStmt {
    block
        .statements
        .iter()
        .find_map(|statement| match statement {
            Stmt::For(stmt) => Some(stmt),
            _ => None,
        })
        .expect("expected a for statement")
}

fn observe_stmt(block: &Block, index: usize) -> &langlog_syntax::ast::ObserveStmt {
    match &block.statements[index] {
        Stmt::Observe(stmt) => stmt,
        other => panic!("expected observe statement at index {index}, got {other:?}"),
    }
}

fn assign_target(block: &Block, index: usize) -> &Expr {
    match &block.statements[index] {
        Stmt::Assign(stmt) => &stmt.target,
        other => panic!("expected assignment statement at index {index}, got {other:?}"),
    }
}

fn assert_resolves_to(
    checked: &CheckedProgram,
    expr: &Expr,
    expected_kind: BindingKind,
    expected_declaration_span: Span,
    context: &str,
) {
    let resolution = checked
        .resolution(name_span(expr))
        .unwrap_or_else(|| panic!("expected resolution for {context}"));
    assert_eq!(
        resolution.kind, expected_kind,
        "wrong binding kind for {context}"
    );
    assert_eq!(
        resolution.declaration_span, expected_declaration_span,
        "wrong declaration span for {context}"
    );
}

fn assert_undefined_name(checked: &CheckedProgram, expr: &Expr, name: &str) {
    let span = name_span(expr);
    assert!(
        checked.resolution(span).is_none(),
        "unexpected resolution for undefined name {name:?}"
    );
    assert!(checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == format!("undefined binding `{name}`")
            && diagnostic
                .labels
                .iter()
                .any(|label| label.style == LabelStyle::Primary && label.span == span)
    }));
}

fn assert_primary_diagnostic(checked: &CheckedProgram, message: &str, span: Span) {
    assert!(checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == message
            && diagnostic
                .labels
                .iter()
                .any(|label| label.style == LabelStyle::Primary && label.span == span)
    }));
}

fn assert_rejects_unbounded_iterable(source: &str, expected_iterable: &str) {
    let checked = analyze_ok(source);
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let loop_stmt = first_for_stmt(&main.body);

    assert_eq!(checked.diagnostics.len(), 1);
    assert_primary_diagnostic(
        &checked,
        "unbounded iteration is not allowed in phase 1",
        loop_stmt.iterable.span,
    );
    assert_eq!(
        checked.parsed.source.span_text(loop_stmt.iterable.span),
        Some(expected_iterable)
    );
}

//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=test
//# The semantic phase MUST resolve item, parameter, and local bindings according to lexical scope.
#[test]
fn requirement_llg_sema_01_resolves_bindings_according_to_lexical_scope() {
    let checked = analyze_ok(
        r#"
fn helper() {}

fn main(param: u32) {
    let local = helper; // item binding
    {
        let helper = local; // outer local binding
        helper              // inner local binding
    };
    helper; // item binding again after leaving the inner block
    param;  // parameter binding
    local;  // outer local binding
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let helper = function(&checked, "helper");
    let main = function(&checked, "main");
    let outer_let = let_stmt(&main.body, 0);
    let inner_block = block_expr(&main.body, 1);
    let inner_let = let_stmt(inner_block, 0);
    let inner_helper_expr = inner_block
        .trailing_expr
        .as_deref()
        .expect("expected trailing expr in inner block");

    // `let local = helper;` should resolve `helper` to the top-level item.
    assert_resolves_to(
        &checked,
        outer_let.value.as_ref().expect("expected let initializer"),
        BindingKind::Item,
        helper.name.span,
        "outer let initializer",
    );

    // `let helper = local;` should resolve `local` to the outer local binding.
    assert_resolves_to(
        &checked,
        inner_let
            .value
            .as_ref()
            .expect("expected inner let initializer"),
        BindingKind::Local,
        outer_let.name.span,
        "inner let initializer",
    );

    // The trailing `helper` inside the inner block should resolve to the shadowing inner local.
    assert_resolves_to(
        &checked,
        inner_helper_expr,
        BindingKind::Local,
        inner_let.name.span,
        "inner block trailing expression",
    );

    // After the inner block ends, `helper;` should resolve back to the top-level item.
    assert_resolves_to(
        &checked,
        expr_stmt(&main.body, 2),
        BindingKind::Item,
        helper.name.span,
        "outer helper expression",
    );

    // `param;` should resolve to the function parameter.
    assert_resolves_to(
        &checked,
        expr_stmt(&main.body, 3),
        BindingKind::Param,
        main.params[0].name.span,
        "parameter expression",
    );

    // `local;` should resolve to the outer local binding introduced by the first let.
    assert_resolves_to(
        &checked,
        expr_stmt(&main.body, 4),
        BindingKind::Local,
        outer_let.name.span,
        "outer local expression",
    );
}

//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=test
//# The semantic phase MUST reject references to undefined bindings.
#[test]
fn requirement_llg_sema_01_rejects_references_to_undefined_bindings() {
    let checked = analyze_ok(
        r#"
fn main() {
    missing;           // no matching item, parameter, or local binding
    let local = other; // initializer also refers to an undefined name
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let missing_expr = expr_stmt(&main.body, 0);
    let other_expr = let_stmt(&main.body, 1)
        .value
        .as_ref()
        .expect("expected initializer");

    // Both unresolved names should produce diagnostics and no successful resolutions.
    assert_eq!(checked.diagnostics.len(), 2);

    // The standalone `missing;` expression should be reported as undefined.
    assert_undefined_name(&checked, missing_expr, "missing");

    // The initializer `other` should also be reported as undefined.
    assert_undefined_name(&checked, other_expr, "other");
}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=test
//# The semantic phase MUST reject direct recursion.
#[test]
fn requirement_llg_sema_02_rejects_direct_recursion() {
    let checked = analyze_ok(
        r#"
fn main() {
    main(); // direct recursive call
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let recursive_call = expr_stmt(&main.body, 0);
    let recursive_callee = call_callee(recursive_call);

    // The call target still resolves to the current function item.
    assert_resolves_to(
        &checked,
        recursive_callee,
        BindingKind::Item,
        main.name.span,
        "recursive call callee",
    );

    // The self-call should be rejected with a direct-recursion diagnostic.
    assert_eq!(checked.diagnostics.len(), 1);
    assert_primary_diagnostic(
        &checked,
        "direct recursion is not allowed for `main`",
        recursive_callee.span,
    );
}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=test
//# The semantic phase MUST reject indirect recursion.
#[test]
fn requirement_llg_sema_02_rejects_indirect_recursion() {
    let checked = analyze_ok(
        r#"
fn alpha() {
    beta(); // depth 1
}

fn beta() {
    gamma(); // depth 2
}

fn gamma() {
    delta(); // depth 3
}

fn delta() {
    alpha(); // closes the cycle after multiple calls
}
"#,
    );
    assert!(checked.has_errors());

    let alpha = function(&checked, "alpha");
    let beta = function(&checked, "beta");
    let gamma = function(&checked, "gamma");
    let delta = function(&checked, "delta");

    // Each call target should still resolve as a top-level function item.
    assert_resolves_to(
        &checked,
        call_callee(expr_stmt(&alpha.body, 0)),
        BindingKind::Item,
        beta.name.span,
        "alpha calling beta",
    );
    assert_resolves_to(
        &checked,
        call_callee(expr_stmt(&beta.body, 0)),
        BindingKind::Item,
        gamma.name.span,
        "beta calling gamma",
    );
    assert_resolves_to(
        &checked,
        call_callee(expr_stmt(&gamma.body, 0)),
        BindingKind::Item,
        delta.name.span,
        "gamma calling delta",
    );
    let closing_callee = call_callee(expr_stmt(&delta.body, 0));
    assert_resolves_to(
        &checked,
        closing_callee,
        BindingKind::Item,
        alpha.name.span,
        "delta calling alpha",
    );

    // The closing edge should be rejected as indirect recursion, regardless of path depth.
    assert_eq!(checked.diagnostics.len(), 1);
    assert_primary_diagnostic(
        &checked,
        "indirect recursion is not allowed: alpha -> beta -> gamma -> delta -> alpha",
        closing_callee.span,
    );
}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=test
//# The semantic phase MUST reject `for` iterables outside the bounded phase 1 loop model; phase 1 bounded iterables are range expressions, array literals, and bindings whose declared types or initializers make them fixed arrays or explicit-capacity `Set`/`Map` values.
#[test]
fn requirement_llg_sema_02_rejects_unbounded_iteration_forms() {
    let bounded_range = analyze_ok(
        r#"
fn main() {
    for value in 0 .. 4 {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
    );
    // Accept a range expression as a bounded phase 1 iterable.
    assert!(
        !bounded_range.has_errors(),
        "{:#?}",
        bounded_range.diagnostics
    );

    let bounded_array_literal = analyze_ok(
        r#"
fn main() {
    for value in [1, 2, 3] {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
    );
    // Accept an array literal as a bounded phase 1 iterable.
    assert!(
        !bounded_array_literal.has_errors(),
        "{:#?}",
        bounded_array_literal.diagnostics
    );

    let bounded_array_binding = analyze_ok(
        r#"
fn main(values: [u32; 4]) {
    for value in values {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
    );
    // Accept a binding whose declared type is a fixed-size array.
    assert!(
        !bounded_array_binding.has_errors(),
        "{:#?}",
        bounded_array_binding.diagnostics
    );

    let bounded_set_binding = analyze_ok(
        r#"
fn main(values: Set<u32, 16>) {
    for value in values {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
    );
    // Accept a binding whose declared type is an explicit-capacity set.
    assert!(
        !bounded_set_binding.has_errors(),
        "{:#?}",
        bounded_set_binding.diagnostics
    );

    let bounded_initializer_binding = analyze_ok(
        r#"
fn main() {
    let values = [1, 2, 3];
    for value in values {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
    );
    // Accept a binding whose initializer proves it is a bounded array value.
    assert!(
        !bounded_initializer_binding.has_errors(),
        "{:#?}",
        bounded_initializer_binding.diagnostics
    );

    // Reject a scalar binding because it is not a bounded iterable.
    assert_rejects_unbounded_iterable(
        r#"
fn main() {
    let count = 3;
    for value in count {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
        "count",
    );

    // Reject an arbitrary computed expression because it is outside the bounded loop model.
    assert_rejects_unbounded_iterable(
        r#"
fn helper() -> u32 { 4 }

fn main() {
    for value in helper() {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
        "helper()",
    );
}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=test
//# The semantic phase MUST require the `else` block of `observe` to be terminal so control cannot continue after a failed observation.
#[test]
fn requirement_llg_sema_02_requires_observe_else_blocks_to_be_terminal() {
    let terminal_else = analyze_ok(
        r#"
fn main(total: u32, limit: u32) {
    observe total < limit else {
        if limit == 0 {
            return;
        } else {
            return;
        }
    }

    total;
}
"#,
    );
    // Accept an observe else-block when every path in that block terminates.
    assert!(
        !terminal_else.has_errors(),
        "{:#?}",
        terminal_else.diagnostics
    );

    let non_terminal_else = analyze_ok(
        r#"
fn main(total: u32, limit: u32) {
    observe total < limit else {
        let fallback = 0;
    }

    total;
}
"#,
    );
    assert!(non_terminal_else.has_errors());

    let main = function(&non_terminal_else, "main");
    let Stmt::Observe(observe) = &main.body.statements[0] else {
        panic!("expected observe statement");
    };

    // Reject an observe else-block when the false path can fall through.
    assert_primary_diagnostic(
        &non_terminal_else,
        "`observe` `else` blocks must be terminal in phase 1",
        observe.else_block.span,
    );
}

//= SPEC.md#llg-sema-03-mutability-and-stable-facts
//= type=test
//# The semantic phase MUST reject assignment to immutable bindings.
#[test]
fn requirement_llg_sema_03_rejects_assignment_to_immutable_bindings() {
    let checked = analyze_ok(
        r#"
fn main(param: u32) {
    let value = 0;
    value = 1;
    param = value;
    let mut total = 0;
    total = param;
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");

    // Reject assignment to an immutable local binding.
    assert_primary_diagnostic(
        &checked,
        "assignment to an immutable binding is not allowed",
        assign_target(&main.body, 1).span,
    );

    // Reject assignment to an immutable parameter binding.
    assert_primary_diagnostic(
        &checked,
        "assignment to an immutable binding is not allowed",
        assign_target(&main.body, 2).span,
    );

    // The mutable local assignment should be the only assignment that survives this program.
    assert_eq!(checked.diagnostics.len(), 2);
}

//= SPEC.md#llg-sema-03-mutability-and-stable-facts
//= type=test
//# In phase 1, the semantic phase MUST reject `observe` proof expressions that directly reference mutable bindings.
#[test]
fn requirement_llg_sema_03_rejects_observe_proof_expressions_that_directly_reference_mutable_bindings(
) {
    let checked = analyze_ok(
        r#"
fn main(limit: u32) {
    let mut total = 0;
    let snapshot = total;
    let stable = limit;

    observe total < limit else {
        return;
    }
    observe snapshot < limit else {
        return;
    }
    observe stable < limit else {
        return;
    }
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");

    // Reject a proof expression that directly mentions a mutable binding.
    assert_primary_diagnostic(
        &checked,
        "mutable bindings are not allowed in `observe` proof expressions",
        name_span(&observe_stmt(&main.body, 3).left),
    );

    // Accept an immutable snapshot of a mutable value in phase 1.
    assert!(
        checked
            .resolution(name_span(&observe_stmt(&main.body, 4).left))
            .is_some(),
        "expected `snapshot` observe to remain valid"
    );

    // The immutable snapshot and immutable parameter binding should not add more errors.
    assert_eq!(checked.diagnostics.len(), 1);
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# The semantic phase MUST reject `let` annotations, assignments, returns, and call arguments whose types do not match declared annotations or function signatures.
#[test]
fn requirement_llg_sema_04_rejects_mismatched_annotations_assignments_returns_and_calls() {
    let valid = analyze_ok(
        r#"
fn id(value: u32) -> u32 {
    value
}

fn main(flag: bool) -> u32 {
    let count: u32 = 1;
    let mut total = count;
    total = id(count);
    if flag {
        return total;
    }
    id(total)
}
"#,
    );
    // Accept matching let annotations, assignments, returns, and call arguments.
    assert!(!valid.has_errors(), "{:#?}", valid.diagnostics);

    let invalid = analyze_ok(
        r#"
fn id(value: u32) -> u32 {
    value
}

fn main(flag: bool) -> u32 {
    let total: u32 = true;
    let mut count = 0;
    count = false;
    id(true);
    return flag;
}
"#,
    );
    assert!(invalid.has_errors());

    let main = function(&invalid, "main");
    let annotated_let = let_stmt(&main.body, 0);
    let assign_value = match &main.body.statements[2] {
        Stmt::Assign(stmt) => &stmt.value,
        other => panic!("expected assignment statement, got {other:?}"),
    };
    let call_arg = match &expr_stmt(&main.body, 3).kind {
        ExprKind::Call { args, .. } => &args[0],
        other => panic!("expected call expression, got {other:?}"),
    };
    let return_stmt = match &main.body.statements[4] {
        Stmt::Return(stmt) => stmt,
        other => panic!("expected return statement, got {other:?}"),
    };

    // Reject a let initializer that does not match its annotation.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected u32, found bool",
        annotated_let.span,
    );

    // Reject an assignment whose value does not match the target type.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected u32, found bool",
        assign_value.span,
    );

    // Reject a call argument that does not match the declared parameter type.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected u32, found bool",
        call_arg.span,
    );

    // Reject a return value that does not match the declared function return type.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected u32, found bool",
        return_stmt.span,
    );
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# The semantic phase MUST require `if` conditions and logical operators to use `bool`.
#[test]
fn requirement_llg_sema_04_requires_bool_conditions_and_logical_operators() {
    let valid = analyze_ok(
        r#"
fn main(flag: bool, other: bool) {
    if flag && other {
        return;
    }
}
"#,
    );
    // Accept boolean conditions and logical operators.
    assert!(!valid.has_errors(), "{:#?}", valid.diagnostics);

    let invalid = analyze_ok(
        r#"
fn main(flag: bool, count: u32) {
    if count {
        return;
    }
    flag && count;
}
"#,
    );
    assert!(invalid.has_errors());

    let main = function(&invalid, "main");
    let if_stmt = match &main.body.statements[0] {
        Stmt::If(stmt) => stmt,
        other => panic!("expected if statement, got {other:?}"),
    };

    // Reject a non-boolean `if` condition.
    assert_primary_diagnostic(
        &invalid,
        "if conditions must have type bool",
        if_stmt.condition.span,
    );

    // Reject a logical expression that uses a non-boolean operand.
    assert_primary_diagnostic(
        &invalid,
        "logical operators must have type bool",
        expr_stmt(&main.body, 1).span,
    );
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# The semantic phase MUST require arithmetic operators, ordering comparisons, and range bounds to use `u32`.
#[test]
fn requirement_llg_sema_04_requires_u32_for_arithmetic_ordering_and_ranges() {
    let valid = analyze_ok(
        r#"
fn main(value: u32, limit: u32) {
    value + limit;
    value < limit;
    for index in 0 .. limit {
        observe index < limit else {
            return;
        }
    }
}
"#,
    );
    // Accept arithmetic, ordering, and range bounds when they use `u32`.
    assert!(!valid.has_errors(), "{:#?}", valid.diagnostics);

    let invalid = analyze_ok(
        r#"
fn main(flag: bool, limit: u32) {
    flag + limit;
    flag < limit;
    for index in true .. limit {
        return;
    }
}
"#,
    );
    assert!(invalid.has_errors());

    let main = function(&invalid, "main");
    let loop_stmt = first_for_stmt(&main.body);

    // Reject arithmetic operands that are not `u32`.
    assert_primary_diagnostic(
        &invalid,
        "arithmetic operators must have type u32",
        expr_stmt(&main.body, 0).span,
    );

    // Reject ordering comparisons that are not `u32`.
    assert_primary_diagnostic(
        &invalid,
        "ordering comparisons must have type u32",
        expr_stmt(&main.body, 1).span,
    );

    // Reject range bounds that are not `u32`.
    assert_primary_diagnostic(
        &invalid,
        "range expressions must have type u32",
        loop_stmt.iterable.span,
    );
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# The semantic phase MUST require array literals to have a homogeneous element type, and MUST require indexing to use an array target plus a `u32` index.
#[test]
fn requirement_llg_sema_04_requires_homogeneous_arrays_and_typed_indexing() {
    let valid = analyze_ok(
        r#"
fn main(index: u32) -> u32 {
    let values = [1, 2, 3];
    values[index]
}
"#,
    );
    // Accept homogeneous arrays and `u32` indexing into arrays.
    assert!(!valid.has_errors(), "{:#?}", valid.diagnostics);

    let invalid = analyze_ok(
        r#"
fn main(flag: bool) {
    let values = [1, false];
    values[flag];
    flag[0];
}
"#,
    );
    assert!(invalid.has_errors());

    let main = function(&invalid, "main");
    let heterogenous_values = let_stmt(&main.body, 0)
        .value
        .as_ref()
        .expect("expected array literal");
    let second_array_element = match &heterogenous_values.kind {
        ExprKind::Array(elements) => &elements[1],
        other => panic!("expected array literal, got {other:?}"),
    };
    let bad_index = match &expr_stmt(&main.body, 1).kind {
        ExprKind::Index { index, .. } => index,
        other => panic!("expected index expression, got {other:?}"),
    };

    // Reject array literals whose elements do not share a single type.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected u32, found bool",
        second_array_element.span,
    );

    // Reject non-`u32` array indices.
    assert_primary_diagnostic(&invalid, "array indices must have type u32", bad_index.span);

    // Reject indexing on a non-array target.
    assert_primary_diagnostic(
        &invalid,
        "indexing requires an array target",
        expr_stmt(&main.body, 2).span,
    );
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# The semantic phase MUST recognize tuple, `Option`, `Result`, `Set`, and `Map` types in bindings, returns, call compatibility, and equality checks.
#[test]
fn requirement_llg_sema_04_recognizes_structured_and_builtin_generic_types() {
    let valid = analyze_ok(
        r#"
fn pair_id(value: (u32, bool)) -> (u32, bool) { value }
fn maybe_id(value: Option<u32>) -> Option<u32> { value }
fn outcome_id(value: Result<u32, Error>) -> Result<u32, Error> { value }
fn set_id(value: Set<u32, 16>) -> Set<u32, 16> { value }
fn map_id(value: Map<u32, bool, 32>) -> Map<u32, bool, 32> { value }

fn main(
    pair: (u32, bool),
    maybe: Option<u32>,
    outcome: Result<u32, Error>,
    members: Set<u32, 16>,
    table: Map<u32, bool, 32>,
) {
    let pair_copy: (u32, bool) = pair_id(pair);
    let maybe_copy: Option<u32> = maybe_id(maybe);
    let outcome_copy: Result<u32, Error> = outcome_id(outcome);
    let members_copy: Set<u32, 16> = set_id(members);
    let table_copy: Map<u32, bool, 32> = map_id(table);
    pair_copy == pair;
    maybe_copy == maybe;
    outcome_copy == outcome;
    members_copy == members;
    table_copy == table;
}
"#,
    );
    // Accept tuple and built-in generic shell types in lets, calls, returns, and equality checks.
    assert!(!valid.has_errors(), "{:#?}", valid.diagnostics);

    let invalid = analyze_ok(
        r#"
fn maybe_id(value: Option<u32>) -> Option<u32> { value }

fn main(
    pair: (u32, bool),
    maybe: Option<u32>,
    outcome: Result<u32, Error>,
    members: Set<u32, 16>,
    table: Map<u32, bool, 32>,
) {
    let wrong_pair: (u32, u32) = pair;
    maybe_id(pair);
    let wrong_members: Set<bool, 16> = members;
    let wrong_map: Map<u32, u32, 32> = table;
    maybe == outcome;
}
"#,
    );
    assert!(invalid.has_errors());

    let main = function(&invalid, "main");
    let wrong_pair = let_stmt(&main.body, 0);
    let wrong_call_arg = match &expr_stmt(&main.body, 1).kind {
        ExprKind::Call { args, .. } => &args[0],
        other => panic!("expected call expression, got {other:?}"),
    };
    let wrong_members = let_stmt(&main.body, 2);
    let wrong_map = let_stmt(&main.body, 3);

    // Reject tuple bindings whose annotation does not match the tuple value.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected (u32, u32), found (u32, bool)",
        wrong_pair.span,
    );

    // Reject calls that pass a tuple where `Option<u32>` is required.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected Option<u32>, found (u32, bool)",
        wrong_call_arg.span,
    );

    // Reject `Set` bindings whose element type does not match.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected Set<bool, 16>, found Set<u32, 16>",
        wrong_members.span,
    );

    // Reject `Map` bindings whose value type does not match.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected Map<u32, u32, 32>, found Map<u32, bool, 32>",
        wrong_map.span,
    );

    // Reject equality checks across different built-in generic shells.
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected Option<u32>, found Result<u32, Error>",
        expr_stmt(&main.body, 4).span,
    );
}
