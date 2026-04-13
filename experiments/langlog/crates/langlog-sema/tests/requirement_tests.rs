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
        observe value >= 0;
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
        observe value >= 0;
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
        observe value >= 0;
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
        observe value >= 0;
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
        observe value >= 0;
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
        observe value >= 0;
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
        observe value >= 0;
    }
}
"#,
        "helper()",
    );
}
