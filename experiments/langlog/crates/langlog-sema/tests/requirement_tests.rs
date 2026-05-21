use langlog_sema::{
    analyze, BindingKind, CheckedProgram, HirBlock, HirExpr, HirExprKind, HirFunction,
    HirMarkerFamily, HirMarkerRuleStmt, HirMarkerTemplateArg, HirParamTransfer, HirPlaceMode,
    HirStmt, HirTask, HirType, HostBuiltin,
};
use langlog_syntax::ast::{Block, Expr, ExprKind, ForStmt, Item, LetStmt, Stmt, Task};
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

fn task<'a>(checked: &'a CheckedProgram, name: &str) -> &'a Task {
    checked
        .parsed
        .module
        .items
        .iter()
        .find_map(|item| match item {
            Item::Task(task) if task.name.value == name => Some(task),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing task {name:?}"))
}

fn hir_function<'a>(checked: &'a CheckedProgram, name: &str) -> &'a HirFunction {
    checked
        .hir
        .as_ref()
        .expect("expected lowered HIR")
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing HIR function {name:?}"))
}

fn hir_task<'a>(checked: &'a CheckedProgram, name: &str) -> &'a HirTask {
    checked
        .hir
        .as_ref()
        .expect("expected lowered HIR")
        .tasks
        .iter()
        .find(|task| task.name == name)
        .unwrap_or_else(|| panic!("missing HIR task {name:?}"))
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

fn hir_let_stmt(block: &HirBlock, index: usize) -> &langlog_sema::HirLetStmt {
    match &block.statements[index] {
        HirStmt::Let(stmt) => stmt,
        other => panic!("expected HIR let statement at index {index}, got {other:?}"),
    }
}

fn hir_expr_stmt(block: &HirBlock, index: usize) -> &HirExpr {
    match &block.statements[index] {
        HirStmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected HIR expression statement at index {index}, got {other:?}"),
    }
}

fn hir_observe_stmt(block: &HirBlock, index: usize) -> &langlog_sema::HirObserveStmt {
    match &block.statements[index] {
        HirStmt::Observe(stmt) => stmt,
        other => panic!("expected HIR observe statement at index {index}, got {other:?}"),
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

fn assert_no_diagnostic_message(checked: &CheckedProgram, message: &str) {
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.message != message),
        "unexpected diagnostic {message:?}: {:#?}",
        checked.diagnostics
    );
}

fn assert_diagnostic_message_contains(checked: &CheckedProgram, message: &str) {
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(message)),
        "missing diagnostic containing {message:?}: {:#?}",
        checked.diagnostics
    );
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
//# Bindings introduced by block, loop, and match scopes MUST NOT be visible after those scopes end.
#[test]
fn requirement_llg_sema_01_drops_bindings_after_nested_scopes_end() {
    let checked = analyze_ok(
        r#"
fn main(flag: bool, values: [u32; 1]) {
    {
        let block_value = 1;
        block_value;
    };

    for loop_value in values {
        loop_value;
    }

    match flag {
        true => { let match_value = 1; match_value; },
        false => { return; }
    }

    block_value;
    loop_value;
    match_value;
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    assert_undefined_name(&checked, expr_stmt(&main.body, 3), "block_value");
    assert_undefined_name(&checked, expr_stmt(&main.body, 4), "loop_value");
    assert_undefined_name(&checked, expr_stmt(&main.body, 5), "match_value");
}

//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=test
//# Type information for block-scoped bindings MUST NOT be visible after the block scope ends.
#[test]
fn requirement_llg_sema_01_drops_binding_types_after_block_scopes_end() {
    let checked = analyze_ok(
        r#"
fn main() {
    {
        let block_value = 1;
        block_value;
    };

    let leaked: bool = block_value;
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let leaked_value = let_stmt(&main.body, 1)
        .value
        .as_ref()
        .expect("expected leaked initializer");
    assert_undefined_name(&checked, leaked_value, "block_value");
    assert_no_diagnostic_message(&checked, "type mismatch: expected bool, found u32");
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

    let bounded_map_binding = analyze_ok(
        r#"
fn main(entries: Map<u32, bool, 16>) {
    for entry in entries {
        entry == entry;
    }
}
"#,
    );
    assert!(
        !bounded_map_binding.has_errors(),
        "{:#?}",
        bounded_map_binding.diagnostics
    );
    let hir_main = hir_function(&bounded_map_binding, "main");
    let HirStmt::For(hir_for) = &hir_main.body.statements[0] else {
        panic!("expected HIR for statement");
    };
    let langlog_sema::HirPatternKind::Binding(binding) = &hir_for.binding.kind else {
        panic!("expected HIR binding pattern");
    };
    assert_eq!(
        binding.ty,
        HirType::Tuple(vec![HirType::U32, HirType::Bool])
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

    // Reject a scalar parameter binding even when its declared type is known.
    assert_rejects_unbounded_iterable(
        r#"
fn main(count: u32) {
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
//# Binary expressions are bounded iterables only when they are range expressions.
#[test]
fn requirement_llg_sema_02_rejects_non_range_binary_iterables() {
    assert_rejects_unbounded_iterable(
        r#"
fn main() {
    for value in 1 + 2 {
        observe value >= 0 else {
            return;
        }
    }
}
"#,
        "1 + 2",
    );
}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=test
//# Scalar declared types MUST NOT be accepted as bounded iterables.
#[test]
fn requirement_llg_sema_02_rejects_scalar_declared_iterables() {
    assert_rejects_unbounded_iterable(
        r#"
fn main(value: u32) {
    for item in value {
        observe item >= 0 else {
            return;
        }
    }
}
"#,
        "value",
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

//= SPEC.md#llg-sema-02-totality-constraints
//= type=test
//# Terminal `observe` else-blocks MAY terminate through nested `if` and `match` statements only when every branch terminates.
#[test]
fn requirement_llg_sema_02_accepts_only_fully_terminal_nested_observe_else_blocks() {
    let terminal_match_else = analyze_ok(
        r#"
fn main(total: u32, flag: bool) {
    observe total > 0 else {
        match flag {
            true => { return; },
            false => { return; }
        }
    }
}
"#,
    );
    assert!(
        !terminal_match_else.has_errors(),
        "{:#?}",
        terminal_match_else.diagnostics
    );

    let terminal_else_if = analyze_ok(
        r#"
fn main(total: u32, flag: bool) {
    observe total > 0 else {
        if flag {
            return;
        } else if false {
            return;
        } else {
            return;
        }
    }
}
"#,
    );
    assert!(
        !terminal_else_if.has_errors(),
        "{:#?}",
        terminal_else_if.diagnostics
    );

    let non_terminal_match_else = analyze_ok(
        r#"
fn main(total: u32, flag: bool) {
    observe total > 0 else {
        match flag {
            true => { return; },
            false => { total; }
        }
    }
}
"#,
    );
    assert!(non_terminal_match_else.has_errors());

    let observe = observe_stmt(&function(&non_terminal_match_else, "main").body, 0);
    assert_primary_diagnostic(
        &non_terminal_match_else,
        "`observe` `else` blocks must be terminal in phase 1",
        observe.else_block.span,
    );

    let non_terminal_else_if = analyze_ok(
        r#"
fn main(total: u32, flag: bool) {
    observe total > 0 else {
        if flag {
            return;
        } else if false {
            total;
        } else {
            return;
        }
    }
}
"#,
    );
    assert!(non_terminal_else_if.has_errors());
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
//# The semantic phase MUST allow `observe` proof expressions to reference mutable bindings; marker validity is enforced by SSA place versioning.
#[test]
fn requirement_llg_sema_03_allows_observe_proof_expressions_that_reference_mutable_bindings() {
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
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let main = function(&checked, "main");
    assert!(
        checked
            .resolution(name_span(&observe_stmt(&main.body, 3).left))
            .is_some(),
        "expected mutable `total` observe to resolve"
    );
    assert!(
        checked
            .resolution(name_span(&observe_stmt(&main.body, 4).left))
            .is_some(),
        "expected immutable `snapshot` observe to resolve"
    );
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Marker-rule conditions of the form `a with Marker(...)` MUST be marker refinement patterns.
#[test]
fn requirement_llg_mark_06_lowers_marker_refinement_patterns_into_hir() {
    let checked = analyze_ok(
        r#"
mark LessThan(a: place, b: place, result: place) {
    if a with LessThan(a, ?bound) {
        implies LessThan(result, bound) for result;
    }
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = checked.hir.as_ref().expect("expected lowered HIR");
    assert_eq!(hir.marker_rules.len(), 1);
    let rule = &hir.marker_rules[0];
    assert_eq!(rule.name, "LessThan");

    let [HirMarkerRuleStmt::If(stmt)] = rule.body.statements.as_slice() else {
        panic!("expected one marker refinement rule");
    };
    assert_eq!(stmt.refinement.subject, "a");
    assert_eq!(stmt.refinement.marker.family, HirMarkerFamily::LessThan);
    assert_eq!(
        stmt.refinement.marker.args[1],
        HirMarkerTemplateArg::Binding("bound".to_owned())
    );

    let [HirMarkerRuleStmt::Implies(implication)] = stmt.body.statements.as_slice() else {
        panic!("expected one marker implication");
    };
    assert_eq!(implication.target, "result");
    assert_eq!(implication.marker.family, HirMarkerFamily::LessThan);
    assert_eq!(
        implication.marker.args[1],
        HirMarkerTemplateArg::Place("bound".to_owned())
    );
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# User marker families MAY appear in marker-qualified types, unsafe marker construction, and companion rule refinements and implications.
#[test]
fn requirement_llg_mark_04_accepts_user_marker_families_in_marker_positions() {
    let checked = analyze_ok(
        r#"
marker Trusted();
marker Bounded(value: place, bound: place);

mark Sub(a: place, amount: place, result: place) {
    if a with Bounded(a, ?bound) {
        implies Bounded(result, bound) for result;
    }
}

fn keep(value: u32 with Trusted, bound: u32) -> u32 with Trusted {
    let copied: u32 with Bounded(bound) = unsafe { Bounded::mark(value, bound) };
    return unsafe { Trusted::mark(copied) };
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = checked.hir.as_ref().expect("expected lowered HIR");
    assert_eq!(hir.marker_families.len(), 2);
    assert_eq!(hir.marker_families[0].name, "Trusted");
    assert_eq!(hir.marker_families[1].name, "Bounded");
    assert_eq!(hir.marker_families[1].params.len(), 2);

    let [HirMarkerRuleStmt::If(stmt)] = hir.marker_rules[0].body.statements.as_slice() else {
        panic!("expected one custom-marker refinement");
    };
    assert_eq!(
        stmt.refinement.marker.family,
        HirMarkerFamily::User("Bounded".to_owned())
    );
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# User marker family declarations MUST NOT duplicate another source marker family or shadow a builtin marker family.
#[test]
fn requirement_llg_mark_04_rejects_duplicate_and_builtin_shadowing_marker_families() {
    let checked = analyze_ok(
        r#"
marker Trusted();
marker Trusted();
marker Event();
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "duplicate marker family `Trusted`");
    assert_diagnostic_message_contains(
        &checked,
        "marker family `Event` conflicts with a builtin marker family",
    );
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# User marker family parameter names MUST be unique and every parameter MUST have type `place`.
#[test]
fn requirement_llg_mark_04_rejects_duplicate_marker_family_parameters() {
    let checked = analyze_ok("marker OwnedBy(value: place, value: place);");

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "duplicate marker family parameter `value`");
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# In marker-qualified value types, a user marker family MAY either use its full declared arity or elide the first declared place, which is then supplied by the qualified subject value.
#[test]
fn requirement_llg_mark_04_allows_user_marker_subject_elision_in_value_types() {
    let checked = analyze_ok(
        r#"
marker Bounded(value: place, bound: place);

fn main(value: u32, bound: u32) {
    let full: u32 with Bounded(value, bound) = value;
    let elided: u32 with Bounded(bound) = value;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# In marker rule bodies, a user marker family MUST be written with its full declared arity.
#[test]
fn requirement_llg_mark_04_requires_full_user_marker_arity_in_marker_rules() {
    let checked = analyze_ok(
        r#"
marker Bounded(value: place, bound: place);

mark Sub(a: place, amount: place, result: place) {
    if a with Bounded(?bound) {
        implies Bounded(result, bound) for result;
    }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "wrong number of arguments for marker `Bounded`");
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# Unsafe construction of a zero-parameter user marker family MUST use one target argument.
#[test]
fn requirement_llg_mark_04_checks_zero_parameter_user_marker_constructor_arity() {
    let checked = analyze_ok(
        r#"
marker Trusted();

fn main(value: u32) {
    unsafe { Trusted::mark(); }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "wrong number of arguments for unsafe marker `Trusted`: expected 1, found 0",
    );
}

//= SPEC.md#llg-mark-04-builtin-marker-families
//= type=test
//# Unsafe construction of a nonzero-parameter user marker family MUST use the declared arity.
#[test]
fn requirement_llg_mark_04_checks_nonzero_user_marker_constructor_arity() {
    let checked = analyze_ok(
        r#"
marker Bounded(value: place, bound: place);

fn main(value: u32) {
    unsafe { Bounded::mark(value); }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "wrong number of arguments for unsafe marker `Bounded`: expected 2, found 1",
    );
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# This marker slice MUST accept builtin comparison companion rule names and the direct checked-arithmetic companion rule names `Add`, `Sub`, `Mul`, `Div`, and `Rem`.
#[test]
fn requirement_llg_mark_06_accepts_checked_arithmetic_companion_rule_names() {
    for name in ["Add", "Sub", "Mul", "Div", "Rem"] {
        let checked = analyze_ok(&format!(
            "mark {name}(a: place, amount: place, result: place) {{}}"
        ));

        assert!(!checked.has_errors(), "{name}: {:#?}", checked.diagnostics);
    }
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Unknown companion marker rule names MUST be rejected.
#[test]
fn requirement_llg_mark_06_rejects_unknown_companion_rule_names() {
    let checked = analyze_ok(
        r#"
marker Product(a: place);
mark Product(a: place, result: place) {}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "unknown companion marker rule `Product`");
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Each accepted builtin companion marker rule name MUST use exactly three `place` parameters.
#[test]
fn requirement_llg_mark_06_rejects_wrong_checked_arithmetic_companion_rule_arity() {
    for name in ["Add", "Sub", "Mul", "Div", "Rem"] {
        let checked = analyze_ok(&format!("mark {name}(a: place, result: place) {{}}"));

        assert!(checked.has_errors(), "{name}: expected arity error");
        assert_diagnostic_message_contains(
            &checked,
            &format!(
                "wrong number of parameters for companion marker rule `{name}`: expected 3, found 2"
            ),
        );
    }
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Duplicate source companion marker rules MUST be rejected.
#[test]
fn requirement_llg_mark_06_rejects_duplicate_source_companion_rules() {
    let checked = analyze_ok(
        r#"
mark LessThan(a: place, b: place, result: place) {}
mark LessThan(a: place, b: place, result: place) {}
mark Add(a: place, amount: place, result: place) {}
mark Add(a: place, amount: place, result: place) {}
mark Sub(a: place, amount: place, result: place) {}
mark Sub(a: place, amount: place, result: place) {}
mark Mul(a: place, amount: place, result: place) {}
mark Mul(a: place, amount: place, result: place) {}
mark Div(a: place, amount: place, result: place) {}
mark Div(a: place, amount: place, result: place) {}
mark Rem(a: place, amount: place, result: place) {}
mark Rem(a: place, amount: place, result: place) {}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "duplicate companion marker rule `LessThan`");
    assert_diagnostic_message_contains(&checked, "duplicate companion marker rule `Add`");
    assert_diagnostic_message_contains(&checked, "duplicate companion marker rule `Sub`");
    assert_diagnostic_message_contains(&checked, "duplicate companion marker rule `Mul`");
    assert_diagnostic_message_contains(&checked, "duplicate companion marker rule `Div`");
    assert_diagnostic_message_contains(&checked, "duplicate companion marker rule `Rem`");
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Duplicate marker rule parameter names MUST be rejected.
#[test]
fn requirement_llg_mark_06_rejects_duplicate_marker_rule_parameter_names() {
    let checked = analyze_ok("mark LessThan(a: place, a: place, result: place) {}");

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "duplicate marker rule parameter `a`");
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Companion marker rules MUST reject unknown marker families in refinements and implications.
#[test]
fn requirement_llg_mark_06_rejects_unknown_marker_families_in_rules() {
    let checked = analyze_ok(
        r#"
mark LessThan(a: place, b: place) {
    implies Unknown(a, b) for a;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "unknown marker `Unknown`");
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Marker-pattern bindings MUST use `?name` at the binding site.
#[test]
fn requirement_llg_mark_06_rejects_marker_pattern_bindings_outside_refinements() {
    let checked = analyze_ok(
        r#"
fn main(value: u32) {
    let copy: u32 with LessThan(value, ?bound) = value;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "marker-pattern bindings are only allowed in marker refinements",
    );
}

//= SPEC.md#llg-mark-06-companion-marker-rules
//= type=test
//# Marker refinement patterns MUST be accepted only inside marker companion-rule bodies.
#[test]
fn requirement_llg_mark_06_rejects_marker_refinements_in_runtime_code() {
    let checked = analyze_ok(
        r#"
fn main(value: u32) {
    if value with Event {
        return;
    }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "marker refinements are only allowed inside marker rules",
    );
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
//# The semantic phase MUST reject calls to non-function values and calls with the wrong number of arguments.
#[test]
fn requirement_llg_sema_04_rejects_non_function_and_wrong_arity_calls() {
    let checked = analyze_ok(
        r#"
fn one(value: u32) -> u32 { value }

fn main(value: u32) {
    value();
    one();
    one(1, 2);
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let non_function_callee = call_callee(expr_stmt(&main.body, 0));
    let missing_arg_callee = call_callee(expr_stmt(&main.body, 1));
    let extra_arg_callee = call_callee(expr_stmt(&main.body, 2));

    assert_primary_diagnostic(
        &checked,
        "calls require a function-valued callee",
        non_function_callee.span,
    );
    assert_primary_diagnostic(
        &checked,
        "call arity mismatch: expected 1 argument(s), found 0",
        missing_arg_callee.span,
    );
    assert_primary_diagnostic(
        &checked,
        "call arity mismatch: expected 1 argument(s), found 2",
        extra_arg_callee.span,
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
//# The semantic phase MUST require arithmetic operands to use `u32` or `Result<u32, ArithmeticError>`, and ordering comparisons and range bounds to use `u32`.
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

    // Reject arithmetic operands that are neither `u32` nor `Result<u32, ArithmeticError>`.
    assert_primary_diagnostic(
        &invalid,
        "arithmetic operators must have type u32 or Result<u32, ArithmeticError>",
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
//# The semantic phase MUST require `observe` equality operands to have matching types and ordering operands to use `u32`.
#[test]
fn requirement_llg_sema_04_requires_typed_observe_operands() {
    let valid = analyze_ok(
        r#"
fn main(value: u32, flag: bool) {
    observe value == 1 else {
        return;
    }
    observe flag != false else {
        return;
    }
    observe value < 10 else {
        return;
    }
}
"#,
    );
    assert!(!valid.has_errors(), "{:#?}", valid.diagnostics);

    let invalid = analyze_ok(
        r#"
fn main(value: u32, flag: bool) {
    observe value == flag else {
        return;
    }
    observe flag < true else {
        return;
    }
}
"#,
    );
    assert!(invalid.has_errors());

    let main = function(&invalid, "main");
    assert_primary_diagnostic(
        &invalid,
        "type mismatch: expected u32, found bool",
        observe_stmt(&main.body, 0).span,
    );
    assert_primary_diagnostic(
        &invalid,
        "observe ordering comparisons must have type u32",
        observe_stmt(&main.body, 1).span,
    );
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# Type compatibility checks MUST NOT emit mismatch diagnostics when either side is already unknown.
#[test]
fn requirement_llg_sema_04_suppresses_type_mismatch_cascades_for_unknown_types() {
    let checked = analyze_ok(
        r#"
fn main() {
    let annotated: u32 = missing;
    let assigned: bool = missing;
    missing + 1;
    missing[0];
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let annotated_value = let_stmt(&main.body, 0)
        .value
        .as_ref()
        .expect("expected annotated initializer");
    let assigned_value = let_stmt(&main.body, 1)
        .value
        .as_ref()
        .expect("expected assigned initializer");
    assert_undefined_name(&checked, annotated_value, "missing");
    assert_undefined_name(&checked, assigned_value, "missing");
    assert_no_diagnostic_message(&checked, "type mismatch: expected u32, found <unknown>");
    assert_no_diagnostic_message(&checked, "type mismatch: expected bool, found <unknown>");
    assert_no_diagnostic_message(
        &checked,
        "arithmetic operators must have type u32 or Result<u32, ArithmeticError>",
    );
    assert_no_diagnostic_message(&checked, "indexing requires an array or map target");
}

//= SPEC.md#llg-sema-04-initial-type-checking
//= type=test
//# The semantic phase MUST require array literals to have a homogeneous element type, and MUST require indexing to use either an array target plus a `u32` index or a `Map<K, V, N>` target plus a `K` key.
#[test]
fn requirement_llg_sema_04_requires_homogeneous_arrays_and_typed_indexing() {
    let valid = analyze_ok(
        r#"
fn main(index: u32, entries: Map<u32, bool, 16>) -> bool {
    let values = [1, 2, 3];
    values[index];
    entries[index]
}
"#,
    );
    // Accept homogeneous arrays, `u32` indexing into arrays, and key indexing into maps.
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

    // Reject indexing on a non-array, non-map target.
    assert_primary_diagnostic(
        &invalid,
        "indexing requires an array or map target",
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

//= SEMANTICS.md#llg-sem-01-builtin-result-types
//= type=test
//# `Option<T>`, `Result<T, E>`, and `ArithmeticError` MUST be builtin semantic types in the first checked-arithmetic phase.
#[test]
fn requirement_llg_sem_01_adds_builtin_option_result_and_arithmetic_error_types() {
    let checked = analyze_ok(
        r#"
fn main(error: ArithmeticError, maybe: Option<u32>, result: Result<u32, ArithmeticError>) {
    error;
    maybe;
    result;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = hir_function(&checked, "main");
    assert_eq!(hir.params[0].ty, HirType::ArithmeticError);
    assert_eq!(hir.params[1].ty, HirType::Option(Box::new(HirType::U32)));
    assert_eq!(
        hir.params[2].ty,
        HirType::Result {
            ok: Box::new(HirType::U32),
            err: Box::new(HirType::ArithmeticError),
        }
    );
}

//= SEMANTICS.md#llg-sem-01-builtin-result-types
//= type=test
//# `ArithmeticError` MUST represent arithmetic overflow, arithmetic underflow, divide-by-zero, and remainder-by-zero failures.
#[test]
fn requirement_llg_sem_01_represents_arithmetic_error_kinds() {
    let checked = analyze_ok(
        r#"
fn main() {
    let overflow: ArithmeticError = arithmetic_overflow();
    let underflow: ArithmeticError = arithmetic_underflow();
    let divide: ArithmeticError = divide_by_zero();
    let remainder: ArithmeticError = remainder_by_zero();
    overflow == underflow;
    divide != remainder;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
}

//= SEMANTICS.md#llg-sem-01-builtin-result-types
//= type=test
//# Builtin `Option` and `Result` types MUST use explicit type arguments without requiring user-defined enum or generic declarations.
#[test]
fn requirement_llg_sem_01_accepts_builtin_option_and_result_type_arguments() {
    let checked = analyze_ok(
        r#"
fn choose(value: Option<u32>) -> Option<u32> { value }
fn compute(value: Result<u32, ArithmeticError>) -> Result<u32, ArithmeticError> { value }
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
}

//= SEMANTICS.md#llg-sem-01-builtin-result-types
//= type=test
//# Builtin constructors `some`, `none`, `ok`, and `err` MUST construct builtin `Option` and `Result` values without requiring user-defined enum variants.
#[test]
fn requirement_llg_sem_01_provides_option_and_result_constructors() {
    let checked = analyze_ok(
        r#"
fn main() {
    let present: Option<u32> = some(1);
    let grouped: Option<u32> = (some)(2);
    let absent: Option<u32> = none();
    let nested_option: Option<Option<u32>> = some(none());
    let success: Result<u32, ArithmeticError> = ok(2);
    let nested_result: Result<Option<u32>, ArithmeticError> = ok(none());
    let failure: Result<u32, ArithmeticError> = err(arithmetic_overflow());
    let bool_error_success: Result<u32, bool> = ok(3);
    let bool_error_failure: Result<u32, bool> = err(false);
    let nested_bool_error: Result<Option<u32>, bool> = ok(none());
    let nested_bool_error_copy: Result<Option<u32>, bool> = ok(some(1));
    let recovered: Option<u32> = nested_result or(err) some(0);
    present == absent;
    present == grouped;
    nested_option == some(recovered);
    success == failure;
    bool_error_success == bool_error_failure;
    nested_bool_error == nested_bool_error_copy;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let invalid = analyze_ok(
        r#"
fn main() {
    let unknown_option = none();
    let unknown_ok = ok(1);
    let unknown_result = err(arithmetic_overflow());
    let wrong_error: Result<u32, bool> = err(arithmetic_overflow());
    let wrong_ok: Result<Option<u32>, bool> = ok(true);
    some();
    none(1);
    ok();
    err();
}
"#,
    );
    assert!(invalid.has_errors());
    assert!(
        invalid
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message == "cannot infer type for builtin `none`")
            .count()
            >= 1
    );
    assert!(invalid
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "cannot infer type for builtin `ok`"));
    assert!(
        invalid
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.message == "cannot infer type for builtin `err`")
            .count()
            >= 1
    );
    assert!(invalid
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message
            == "type mismatch: expected bool, found ArithmeticError"));
    assert!(invalid
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "type mismatch: expected Option<u32>, found bool"));
    assert!(invalid
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message
            == "call arity mismatch: expected 1 argument(s), found 0"));
    assert!(invalid
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message
            == "call arity mismatch: expected 0 argument(s), found 1"));
}

//= SEMANTICS.md#llg-sem-01-builtin-result-types
//= type=test
//# Builtin constructors `arithmetic_overflow`, `arithmetic_underflow`, `divide_by_zero`, and `remainder_by_zero` MUST construct the corresponding `ArithmeticError` values.
#[test]
fn requirement_llg_sem_01_provides_arithmetic_error_constructors() {
    let checked = analyze_ok(
        r#"
fn main() {
    let overflow: ArithmeticError = arithmetic_overflow();
    let underflow: ArithmeticError = arithmetic_underflow();
    let divide: ArithmeticError = divide_by_zero();
    let remainder: ArithmeticError = remainder_by_zero();
    overflow != underflow;
    divide != remainder;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
}

//= SEMANTICS.md#llg-sem-02-recovery-expressions
//= type=test
//# Recovery expressions MUST support `option_expr or fallback_expr`, producing `T` from `Option<T>` when `fallback_expr` has type `T`.
#[test]
fn requirement_llg_sem_02_supports_option_recovery_expressions() {
    let checked = analyze_ok(
        r#"
fn main() -> u32 {
    let maybe: Option<u32> = none();
    maybe or 7
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = hir_function(&checked, "main");
    assert_eq!(
        hir.body.result.as_ref().expect("expected result").ty,
        HirType::U32
    );
}

//= SEMANTICS.md#llg-sem-02-recovery-expressions
//= type=test
//# Recovery expressions MUST support `result_expr or(err) fallback_expr`, producing `T` from `Result<T, E>` when `fallback_expr` has type `T`.
#[test]
fn requirement_llg_sem_02_supports_result_recovery_expressions() {
    let checked = analyze_ok(
        r#"
fn main() -> u32 {
    1 + 2 or(err) 0
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = hir_function(&checked, "main");
    assert_eq!(
        hir.body.result.as_ref().expect("expected result").ty,
        HirType::U32
    );
}

//= SEMANTICS.md#llg-sem-02-recovery-expressions
//= type=test
//# In a result recovery expression, the error binding MUST be scoped only inside the fallback expression and MUST have the result error type.
#[test]
fn requirement_llg_sem_02_scopes_result_recovery_error_binding() {
    let checked = analyze_ok(
        r#"
fn main() -> u32 {
    let recovered = 1 / 0 or(err) {
        let captured: ArithmeticError = err;
        0
    };
    let wrapped: Result<u32, ArithmeticError> = err(arithmetic_overflow());
    recovered
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let main = function(&checked, "main");
    let wrapped = let_stmt(&main.body, 1);
    let callee = call_callee(wrapped.value.as_ref().expect("expected constructor call"));
    assert_resolves_to(
        &checked,
        callee,
        BindingKind::HostBuiltin,
        name_span(callee),
        "outer err constructor",
    );
}

//= SEMANTICS.md#llg-sem-03-checked-arithmetic
//= type=test
//# Ordinary `+`, `-`, `*`, `/`, and `%` operations on `u32` operands MUST return `Result<u32, ArithmeticError>`.
#[test]
fn requirement_llg_sem_03_makes_u32_arithmetic_return_result() {
    let checked = analyze_ok(
        r#"
fn main(left: u32, right: u32) {
    let sum = left + right;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = hir_function(&checked, "main");
    assert_eq!(
        hir_let_stmt(&hir.body, 0).binding.ty,
        HirType::Result {
            ok: Box::new(HirType::U32),
            err: Box::new(HirType::ArithmeticError),
        }
    );
}

//= SEMANTICS.md#llg-sem-04-result-lifting
//= type=test
//# Arithmetic operators MUST lift over operands of the same numeric type when either operand is `Result<T, ArithmeticError>`.
#[test]
fn requirement_llg_sem_04_lifts_arithmetic_over_result_operands() {
    let checked = analyze_ok(
        r#"
fn main() {
    let left: Result<u32, ArithmeticError> = ok(40);
    let lifted = left + 2;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
}

//= SEMANTICS.md#llg-sem-04-result-lifting
//= type=test
//# Result-lifted arithmetic MUST produce `Result<T, ArithmeticError>` for the shared numeric type `T`.
#[test]
fn requirement_llg_sem_04_returns_result_for_lifted_arithmetic() {
    let checked = analyze_ok(
        r#"
fn main() {
    let left: Result<u32, ArithmeticError> = ok(40);
    let lifted = left + 2;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let hir = hir_function(&checked, "main");
    assert_eq!(
        hir_let_stmt(&hir.body, 1).binding.ty,
        HirType::Result {
            ok: Box::new(HirType::U32),
            err: Box::new(HirType::ArithmeticError),
        }
    );
}

//= SEMANTICS.md#llg-sem-05-numeric-type-discipline
//= type=test
//# Numeric operators MUST NOT perform implicit numeric promotion.
#[test]
fn requirement_llg_sem_05_rejects_implicit_numeric_promotion() {
    let checked = analyze_ok("fn main(flag: bool, value: u32) { flag + value; }");

    assert!(checked.has_errors());
    let main = function(&checked, "main");
    assert_primary_diagnostic(
        &checked,
        "arithmetic operators must have type u32 or Result<u32, ArithmeticError>",
        expr_stmt(&main.body, 0).span,
    );
}

//= SEMANTICS.md#llg-sem-05-numeric-type-discipline
//= type=test
//# Numeric operators MUST require the same underlying numeric type after stripping any compatible `Result<T, ArithmeticError>` layer.
#[test]
fn requirement_llg_sem_05_requires_matching_underlying_numeric_types() {
    let checked = analyze_ok(
        r#"
fn main() {
    let left: Result<bool, ArithmeticError> = ok(true);
    left + 1;
}
"#,
    );

    assert!(checked.has_errors());
    let main = function(&checked, "main");
    assert_primary_diagnostic(
        &checked,
        "arithmetic operators must have type u32 or Result<u32, ArithmeticError>",
        expr_stmt(&main.body, 1).span,
    );
}

//= SEMANTICS.md#llg-sem-06-raw-arithmetic-reservation
//= type=test
//# This checked-arithmetic phase MUST NOT reserve or recognize exact surface names for raw or proof-backed arithmetic operations.
#[test]
fn requirement_llg_sem_06_does_not_recognize_raw_arithmetic_surface_names() {
    let checked = analyze_ok(
        r#"
fn main(left: u32, right: u32) {
    raw_add(left, right);
    let raw_sub = left;
    raw_sub;
}
"#,
    );

    assert!(checked.has_errors());
    let main = function(&checked, "main");
    let raw_call = expr_stmt(&main.body, 0);
    let ExprKind::Call { callee, .. } = &raw_call.kind else {
        panic!("expected raw_add call");
    };
    assert_undefined_name(&checked, callee, "raw_add");

    assert_no_diagnostic_message(&checked, "reserved host builtin name `raw_sub`");
    let raw_sub_use = expr_stmt(&main.body, 2);
    assert!(checked
        .resolution(name_span(raw_sub_use))
        .is_some_and(|resolution| resolution.kind == BindingKind::Local));
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# Every reachable state control path MUST end in an `exit` statement or a same-task terminal `go` statement.
#[test]
fn requirement_llg_sema_06_accepts_terminal_task_states_and_lowers_hir() {
    let checked = analyze_ok(
        r#"
fn id(value: u32) -> u32 { value }

task main(value: u32) -> u32 {
    let mut total: u32 = 0;

    state start(value: u32) {
        let next = id(value);
        total = next;
        go done(next);
    }

    state done(result: u32) {
        exit result;
    }
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let ast_main = task(&checked, "main");
    let hir_main = hir_task(&checked, "main");

    assert_eq!(hir_main.return_type, HirType::U32);
    assert_eq!(hir_main.fields.len(), 1);
    assert_eq!(hir_main.states.len(), 2);
    assert_eq!(hir_main.states[0].name, "start");
    assert!(matches!(
        hir_main.states[0].body.statements[2],
        HirStmt::Go(_)
    ));
    assert!(matches!(
        hir_main.states[1].body.statements[0],
        HirStmt::Exit(_)
    ));
    let next_value = match &ast_main.states[0].body.statements[0] {
        Stmt::Let(stmt) => stmt.value.as_ref().expect("expected initializer"),
        other => panic!("expected let statement, got {other:?}"),
    };
    assert_resolves_to(
        &checked,
        call_callee(next_value),
        BindingKind::Item,
        function(&checked, "id").name.span,
        "task ordinary function call",
    );
}

//= HIR.md#llg-hir-06-task-runtime-lowering-support
//= type=test
//# HIR MUST preserve task fields, task-local declarations, state identity, and `go` argument values sufficiently for later task-state enum lowering.
#[test]
fn requirement_llg_hir_06_preserves_task_state_lowering_inputs() {
    let checked = analyze_ok(
        r#"
task main(value: u32) -> u32 {
    let mut saved: u32 = value;

    state start(value: u32) {
        let next = value;
        saved = next;
        go beta(next);
    }

    state beta(value: u32) {
        exit value;
    }
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let main = task(&checked, "main");
    let beta = &main.states[1];
    let local = match &main.states[0].body.statements[0] {
        Stmt::Let(stmt) => stmt,
        other => panic!("expected task-local let, got {other:?}"),
    };
    let hir_main = hir_task(&checked, "main");

    assert_eq!(checked.hir.as_ref().unwrap().functions.len(), 0);
    let HirStmt::Let(hir_local) = &hir_main.states[0].body.statements[0] else {
        panic!("expected HIR task-local let");
    };
    assert_eq!(hir_local.binding.id.declaration_span, local.name.span);

    let HirStmt::Go(go) = &hir_main.states[0].body.statements[2] else {
        panic!("expected HIR go statement");
    };
    assert_eq!(go.target.declaration_span, beta.name.span);
    let HirExprKind::Binding(arg_id) = go.args[0].kind else {
        panic!("expected HIR go argument to resolve to local binding");
    };
    assert_eq!(arg_id.declaration_span, local.name.span);
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A `go` statement MUST target a state in the same task.
#[test]
fn requirement_llg_sema_06_rejects_plain_task_calls_and_invalid_go() {
    let checked = analyze_ok(
        r#"
task worker(value: u32) -> u32 {
    state start(value: u32) { exit value; }
}

fn function_calls_task() {
    worker(0);
}

task task_calls_task() -> u32 {
    state start() {
        worker(0);
        go missing();
    }

    state target(value: u32) {
        exit value;
    }
}

task go_wrong_arity() -> u32 {
    state start() { go target(); }
    state target(value: u32) { exit value; }
}

task go_wrong_arg() -> u32 {
    state start() { go target(true); }
    state target(value: u32) { exit value; }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "task items cannot be used as expressions");
    assert_diagnostic_message_contains(&checked, "unknown task state `missing`");
    assert_diagnostic_message_contains(
        &checked,
        "go arity mismatch: expected 1 argument(s), found 0",
    );
    assert_diagnostic_message_contains(&checked, "type mismatch: expected u32, found bool");
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A `go` statement MUST NOT return, push a stack frame, or retain previous state arguments.
#[test]
fn requirement_llg_sema_06_accepts_cyclic_state_go() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    state start() {
        go beta();
    }

    state beta() {
        go start();
    }
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    assert!(matches!(
        hir_task(&checked, "main").states[0].body.statements[0],
        HirStmt::Go(_)
    ));
    assert!(matches!(
        hir_task(&checked, "main").states[1].body.statements[0],
        HirStmt::Go(_)
    ));
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A task MUST declare exactly one state named `start`, and `start` MUST be the task entry state.
#[test]
fn requirement_llg_sema_06_requires_exactly_one_start_state() {
    let checked = analyze_ok(
        r#"
task missing() -> u32 {
    state other() { exit 0; }
}

task duplicate() -> u32 {
    state start() { exit 0; }
    state start() { exit 1; }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "tasks must declare a `start` state");
    assert_diagnostic_message_contains(&checked, "duplicate task state `start`");
    assert_diagnostic_message_contains(&checked, "tasks must declare exactly one `start` state");
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# The task entry state's parameter list MUST match the enclosing task parameter list in arity and type.
#[test]
fn requirement_llg_sema_06_requires_start_params_to_match_task_params() {
    let checked = analyze_ok(
        r#"
task wrong_arity(value: u32) -> u32 {
    state start() { exit 0; }
}

task wrong_type(value: u32) -> u32 {
    state start(value: bool) { exit 0; }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "`start` state arity mismatch");
    assert_diagnostic_message_contains(
        &checked,
        "`start` state parameter 0 has type bool, expected u32",
    );
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# Task fields MUST be visible to every state in the same task.
#[test]
fn requirement_llg_sema_06_task_fields_are_visible_to_states() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    let mut total: u32 = 1;

    state start() {
        total = total + 1 or(err) total;
        go done();
    }

    state done() {
        exit total;
    }
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    assert_eq!(hir_task(&checked, "main").fields[0].binding.name, "total");
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A state body MUST contain only bounded ordinary Langlog work before its terminal `go` or `exit` path.
#[test]
fn requirement_llg_sema_06_rejects_unbounded_state_work() {
    let checked = analyze_ok(
        r#"
fn values() -> [u32; 2] { [1, 2] }

task main(items: [u32; 2]) -> u32 {
    state start(items: [u32; 2]) {
        for value in values() {
            exit value;
        }
        exit 0;
    }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "unbounded iteration is not allowed in phase 1");
}

//= SPEC.md#llg-sema-05-legacy-task-statement-rejection
//= type=test
//# A task state body MUST NOT fall through accidentally; every reachable state control path MUST end in `exit` or same-task `go`.
#[test]
fn requirement_llg_sema_06_rejects_state_fallthrough_paths() {
    let checked = analyze_ok(
        r#"
task direct() -> u32 {
    state start() { let value = 0; }
}

task branch(flag: bool) -> u32 {
    state start(flag: bool) {
        if flag { exit 0; }
    }
}

task choose(flag: bool) -> u32 {
    state start(flag: bool) {
        match flag {
            true => { exit 0; },
            false => { let value = 1; }
        }
    }
}
"#,
    );

    assert!(checked.has_errors());
    let fallthrough_count = checked
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message == "task states must not fall through")
        .count();
    assert_eq!(fallthrough_count, 3, "{:#?}", checked.diagnostics);
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A `go` target that is not declared in the same task MUST be rejected.
#[test]
fn requirement_llg_sema_06_go_targets_same_task_state() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    state start() { go other(); }
}

task other() -> u32 {
    state start() { exit 0; }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "unknown task state `other`");
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# HIR lowering MUST represent `go` as a task-state transition rather than an ordinary function call.
#[test]
fn requirement_llg_sema_06_go_lowers_as_task_state_transition() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    state start() { go next(42); }
    state next(value: u32) { exit value; }
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    assert!(checked.hir.as_ref().unwrap().functions.is_empty());
    assert!(matches!(
        hir_task(&checked, "main").states[0].body.statements[0],
        HirStmt::Go(_)
    ));
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A `go` statement MUST preserve task fields and replace only the active state arguments with the evaluated target arguments.
#[test]
fn requirement_llg_sema_06_go_preserves_fields_and_replaces_state_args() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    let mut saved: u32 = 0;

    state start() {
        saved = 7;
        go next(42);
    }

    state next(value: u32) {
        exit saved + value or(err) 0;
    }
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    let task = hir_task(&checked, "main");
    assert_eq!(task.fields[0].binding.name, "saved");
    let HirStmt::Go(go) = &task.states[0].body.statements[1] else {
        panic!("expected go");
    };
    assert_eq!(go.args.len(), 1);
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A task instance MUST be representable as a finite tagged union of task states.
#[test]
fn requirement_llg_sema_06_hir_represents_task_as_finite_state_set() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    state start() { go done(); }
    state done() { exit 0; }
}
"#,
    );

    assert_eq!(hir_task(&checked, "main").states.len(), 2);
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# Each task-state variant MUST represent one state item in the reachable `go` graph for that task instance.
#[test]
fn requirement_llg_sema_06_hir_state_variants_have_stable_ids() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    state start() { go done(); }
    state done() { exit 0; }
}
"#,
    );
    let task = hir_task(&checked, "main");

    assert_eq!(task.states[0].id.task, task.id);
    assert_eq!(task.states[1].id.task, task.id);
    assert_ne!(task.states[0].id, task.states[1].id);
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# At runtime, exactly one task-state variant MUST be active per task instance.
#[test]
fn requirement_llg_sema_06_hir_go_names_one_target_state_variant() {
    let checked = analyze_ok(
        r#"
task main() -> u32 {
    state start() { go done(); }
    state done() { exit 0; }
}
"#,
    );
    let task = hir_task(&checked, "main");
    let HirStmt::Go(go) = &task.states[0].body.statements[0] else {
        panic!("expected go");
    };

    assert_eq!(go.target, task.states[1].id);
}

//= SPEC.md#llg-sema-06-target-task-state-semantics
//= type=test
//# A state body with a reachable fallthrough path MUST be rejected.
#[test]
fn requirement_llg_sema_05_rejects_task_fallthrough_paths() {
    let checked = analyze_ok(
        r#"
fn tick() {}

task direct() -> u32 {
    state start() { let value = 0; }
}

task branch(flag: bool) -> u32 {
    state start(flag: bool) {
        if flag { exit 0; }
    }
}

task choose(flag: bool) -> u32 {
    state start(flag: bool) {
        match flag {
            true => { exit 0; },
            false => { tick(); }
        }
    }
}
"#,
    );

    assert!(checked.has_errors());
    let fallthrough_count = checked
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.message == "task states must not fall through")
        .count();
    assert_eq!(fallthrough_count, 3, "{:#?}", checked.diagnostics);
}

//= SPEC.md#llg-sema-05-legacy-task-statement-rejection
//= type=test
//# An `exit` statement MUST appear only inside a task state body.
#[test]
fn requirement_llg_sema_05_rejects_exit_outside_tasks() {
    let checked = analyze_ok(
        r#"
fn bad_exit() {
    exit 0;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "`exit` is only valid inside task states");
}

//= SPEC.md#llg-sema-05-legacy-task-statement-rejection
//= type=test
//# A nested `forever` statement MUST be rejected.
#[test]
fn requirement_llg_sema_05_rejects_forever_outside_tasks() {
    let checked = analyze_ok(
        r#"
fn bad_forever() {
    forever {}
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "`forever` is not part of target task states");
}

//= SPEC.md#llg-sema-05-legacy-task-statement-rejection
//= type=test
//# `delegate` MUST be rejected as legacy syntax that is not part of target task states.
#[test]
fn requirement_llg_sema_05_rejects_delegate_outside_tasks() {
    let checked = analyze_ok(
        r#"
task worker() -> u32 {
    state start() { exit 0; }
}

fn bad_delegate() {
    delegate worker();
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "`delegate` is not part of target task states");
}

//= SPEC.md#llg-sema-05-legacy-task-statement-rejection
//= type=test
//# A `return` statement MUST be rejected inside a task state body.
#[test]
fn requirement_llg_sema_05_rejects_return_inside_tasks() {
    let checked = analyze_ok(
        r#"
task bad_return() -> u32 {
    state start() { return 0; }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "`return` is not allowed inside a task");
}

//= SPEC.md#llg-sema-05-legacy-task-statement-rejection
//= type=test
//# `forever` MUST be rejected as legacy syntax that is not part of target task states.
#[test]
fn requirement_llg_sema_05_rejects_nested_forever() {
    let checked = analyze_ok(
        r#"
task bad_nested() -> u32 {
    state start() {
        forever {
            forever {}
        }
    }
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "nested `forever` loops are not allowed");
}

//= HIR.md#llg-hir-01-pipeline-and-lowering
//= type=test
//# The front end MUST lower successfully checked programs from AST into typed HIR before generating proof IR or MIR.
#[test]
fn requirement_llg_hir_01_lowers_checked_programs_into_typed_hir_before_later_irs() {
    let checked = analyze_ok(
        r#"
fn main(value: u32) -> u32 {
    let copy = value;
    copy
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let hir_main = hir_function(&checked, "main");
    assert_eq!(hir_main.name, "main");
    assert_eq!(hir_main.return_type, HirType::U32);
}

//= HIR.md#llg-hir-01-pipeline-and-lowering
//= type=test
//# Every HIR node MUST preserve a source span sufficient for diagnostics and traceability.
#[test]
fn requirement_llg_hir_01_preserves_hir_source_spans_for_diagnostics_and_traceability() {
    let checked = analyze_ok(
        r#"
fn main(value: u32) {
    let copy = value;
    observe copy < 10 else { return; }
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let main = function(&checked, "main");
    let hir_main = hir_function(&checked, "main");
    let ast_let = let_stmt(&main.body, 0);
    let hir_let = hir_let_stmt(&hir_main.body, 0);
    let ast_observe = observe_stmt(&main.body, 1);
    let hir_observe = hir_observe_stmt(&hir_main.body, 1);

    assert_eq!(hir_main.span, main.span);
    assert_eq!(hir_main.body.span, main.body.span);
    assert_eq!(hir_let.span, ast_let.span);
    assert_eq!(hir_let.binding.span, ast_let.name.span);
    assert_eq!(
        checked.parsed.source.span_text(hir_let.binding.span),
        Some("copy")
    );
    assert_eq!(hir_observe.span, ast_observe.span);
    assert_eq!(hir_observe.left.span, ast_observe.left.span);
    assert_eq!(hir_observe.right.span, ast_observe.right.span);
    assert_eq!(hir_observe.else_block.span, ast_observe.else_block.span);
}

//= HIR.md#llg-hir-02-identities-and-resolution
//= type=test
//# Every HIR function item, parameter, and local binding MUST carry a stable semantic identity, and every HIR name use MUST resolve to either an item identity or a binding identity.
#[test]
fn requirement_llg_hir_02_attaches_stable_identities_and_resolved_references() {
    let checked = analyze_ok(
        r#"
fn helper(value: u32) -> u32 { value }

fn main(param: u32) {
    let local = helper(param);
    helper(local);
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let helper = function(&checked, "helper");
    let main = function(&checked, "main");
    let hir_helper = hir_function(&checked, "helper");
    let hir_main = hir_function(&checked, "main");
    let hir_local = hir_let_stmt(&hir_main.body, 0);

    assert_eq!(hir_helper.id.declaration_span, helper.name.span);
    assert_eq!(
        hir_helper.params[0].id.declaration_span,
        helper.params[0].name.span
    );
    assert_eq!(hir_main.id.declaration_span, main.name.span);
    assert_eq!(
        hir_main.params[0].id.declaration_span,
        main.params[0].name.span
    );
    assert_eq!(
        hir_local.binding.id.declaration_span,
        let_stmt(&main.body, 0).name.span
    );

    let initializer = hir_local.value.as_ref().expect("expected HIR initializer");
    let HirExprKind::Call { callee, args } = &initializer.kind else {
        panic!("expected HIR call initializer, got {:?}", initializer.kind);
    };
    let HirExprKind::Item(item_id) = callee.kind else {
        panic!("expected HIR item callee, got {:?}", callee.kind);
    };
    let HirExprKind::Binding(param_id) = args[0].kind else {
        panic!("expected HIR parameter argument, got {:?}", args[0].kind);
    };
    assert_eq!(item_id.declaration_span, helper.name.span);
    assert_eq!(param_id.declaration_span, main.params[0].name.span);

    let call_expr = hir_expr_stmt(&hir_main.body, 1);
    let HirExprKind::Call { callee, args } = &call_expr.kind else {
        panic!("expected HIR call expression, got {:?}", call_expr.kind);
    };
    let HirExprKind::Item(item_id) = callee.kind else {
        panic!("expected HIR item callee, got {:?}", callee.kind);
    };
    let HirExprKind::Binding(local_id) = args[0].kind else {
        panic!(
            "expected HIR local binding argument, got {:?}",
            args[0].kind
        );
    };
    assert_eq!(item_id.declaration_span, helper.name.span);
    assert_eq!(local_id.declaration_span, let_stmt(&main.body, 0).name.span);
}

//= HIR.md#llg-hir-03-types-and-mutability
//= type=test
//# Every HIR binding MUST record its mutability and type directly, and every HIR expression MUST record its type directly.
#[test]
fn requirement_llg_hir_03_records_mutability_and_types_directly_on_hir_nodes() {
    let checked = analyze_ok(
        r#"
fn add_one(value: u32) -> u32 { value + 1 or(err) 0 }

fn main(flag: bool, input: u32) {
    let mut total = add_one(input);
    let pair = (total, flag);
    pair;
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let hir_main = hir_function(&checked, "main");
    let total = hir_let_stmt(&hir_main.body, 0);
    let pair = hir_let_stmt(&hir_main.body, 1);

    assert_eq!(hir_main.params[0].ty, HirType::Bool);
    assert_eq!(hir_main.params[1].ty, HirType::U32);
    assert!(total.binding.mutable);
    assert_eq!(total.binding.ty, HirType::U32);
    assert_eq!(
        total.value.as_ref().expect("expected HIR initializer").ty,
        HirType::U32
    );
    assert_eq!(
        pair.binding.ty,
        HirType::Tuple(vec![HirType::U32, HirType::Bool])
    );
    assert_eq!(
        hir_expr_stmt(&hir_main.body, 2).ty,
        HirType::Tuple(vec![HirType::U32, HirType::Bool])
    );
}

//= HIR.md#llg-hir-03-types-and-mutability
//= type=test
//# HIR bindings and return slots MUST record structural place modes separately from concrete types.
#[test]
fn requirement_llg_hir_03_records_place_modes_separately_from_types() {
    let checked = analyze_ok(
        r#"
fn close(take handle: linear Handle) -> affine u32 {
    let token: relevant u32 = 1;
    0
}

task main(seed: relevant u32) -> linear u32 {
    let field: affine u32 = 1;

    state start(seed: relevant u32) {
        exit 0;
    }
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let close = hir_function(&checked, "close");
    assert_eq!(close.params[0].ty, HirType::Named("Handle".to_owned()));
    assert_eq!(close.params[0].place_mode, Some(HirPlaceMode::Linear));
    assert_eq!(close.params[0].param_transfer, Some(HirParamTransfer::Take));
    assert_eq!(close.return_mode, HirPlaceMode::Affine);
    assert_eq!(close.return_type, HirType::U32);
    let token = hir_let_stmt(&close.body, 0);
    assert_eq!(token.binding.place_mode, Some(HirPlaceMode::Relevant));
    assert_eq!(token.binding.ty, HirType::U32);

    let task = hir_task(&checked, "main");
    assert_eq!(task.params[0].place_mode, Some(HirPlaceMode::Relevant));
    assert_eq!(
        task.fields[0].binding.place_mode,
        Some(HirPlaceMode::Affine)
    );
    assert_eq!(
        task.states[0].params[0].place_mode,
        Some(HirPlaceMode::Relevant)
    );
    assert_eq!(task.return_mode, HirPlaceMode::Linear);
}

//= SPEC.md#llg-type-04-place-mode-annotations
//= type=test
//# Explicit modes MUST NOT weaken an incoming restrictive source place.
#[test]
fn requirement_llg_type_04_rejects_explicit_mode_weakening() {
    let checked = analyze_ok(
        r#"
fn main() {
    let event: relevant u32 = 1;
    let plain: unrestricted u32 = event;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "cannot place relevant value into unrestricted place",
    );
}

//= MARKER_MODES.md#llg-mm-06-function-parameters
//= type=test
//# A parameter whose mode is affine or linear MUST receive a moved argument, even when `take` is omitted.
#[test]
fn requirement_llg_mm_06_linear_parameter_implies_move_without_take() {
    let checked = analyze_ok(
        r#"
fn close(handle: linear u32) {}

fn main() {
    let handle: linear u32 = 1;
    close(handle);
    handle;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "use of moved place");
}

//= MARKER_MODES.md#llg-mm-06-function-parameters
//= type=test
//# Passing an unrestricted argument to a linear parameter MUST be accepted and must move the source into a linear receiving place.
#[test]
fn requirement_llg_mm_06_unrestricted_argument_can_move_to_linear_parameter() {
    let checked = analyze_ok(
        r#"
fn close(handle: linear u32) {}

fn main() {
    let handle = 1;
    close(handle);
    handle;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "use of moved place");
    assert_no_diagnostic_message(
        &checked,
        "cannot place unrestricted value into linear place",
    );
}

//= MARKER_MODES.md#llg-mm-06-function-parameters
//= type=test
//# Passing an argument whose current mode is affine or linear to a copying parameter MUST be rejected because affine and linear places cannot be copied.
#[test]
fn requirement_llg_mm_06_rejects_linear_argument_for_unrestricted_parameter() {
    let checked = analyze_ok(
        r#"
fn inspect(value: u32) {}

fn main() {
    let handle: linear u32 = 1;
    inspect(handle);
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "cannot place linear value into unrestricted place",
    );
    assert_diagnostic_message_contains(&checked, "cannot copy linear place");
}

//= MARKER_MODES.md#llg-mm-06-function-parameters
//= type=test
//# A parameter whose mode is unrestricted or relevant and which is not written with `take` MUST receive a copied argument.
#[test]
fn requirement_llg_mm_06_relevant_parameter_copies_without_take() {
    let checked = analyze_ok(
        r#"
fn remember(value: relevant u32) {}

fn main() {
    let event: relevant u32 = 1;
    remember(event);
    event;
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
}

//= MARKER_MODES.md#llg-mm-06-function-parameters
//= type=test
//# A parameter written with `take` MUST receive a moved argument.
#[test]
fn requirement_llg_mm_06_take_relevant_parameter_moves() {
    let checked = analyze_ok(
        r#"
fn handle(take event: relevant u32) {}

fn main() {
    let event: relevant u32 = 1;
    handle(event);
    event;
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(&checked, "use of moved place");
}

//= SPEC.md#llg-type-04-place-mode-annotations
//= type=test
//# Local `let` annotations MAY omit the structural mode so mode can be inferred from the initializer.
#[test]
fn requirement_llg_type_04_omitted_local_mode_infers_from_initializer() {
    let checked = analyze_ok(
        r#"
fn inspect(value: u32) {}

fn main() {
    let event: relevant u32 = 1;
    let copy = event;
    inspect(copy);
}
"#,
    );

    assert!(checked.has_errors());
    assert_diagnostic_message_contains(
        &checked,
        "cannot place relevant value into unrestricted place",
    );
}

//= HIR.md#llg-hir-04-normalization-boundary
//= type=test
//# Omitted surface function return types MUST lower to explicit `()` return types in HIR, grouped expressions MUST NOT survive as distinct HIR nodes, and HIR blocks MUST represent trailing result positions explicitly.
#[test]
fn requirement_llg_hir_04_normalizes_returns_grouping_and_block_results() {
    let checked = analyze_ok(
        r#"
fn helper(value: u32) -> u32 { value }

fn main(input: u32) {
    let value = (helper(input));
    {
        value
    };
}

fn unit() {}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let main = function(&checked, "main");
    let hir_main = hir_function(&checked, "main");
    let grouped_value = let_stmt(&main.body, 0)
        .value
        .as_ref()
        .expect("expected grouped initializer");
    let hir_value = hir_let_stmt(&hir_main.body, 0)
        .value
        .as_ref()
        .expect("expected HIR grouped initializer");

    assert_eq!(hir_function(&checked, "unit").return_type, HirType::Unit);
    assert_eq!(hir_value.span, grouped_value.span);
    assert!(matches!(hir_value.kind, HirExprKind::Call { .. }));

    let block_expr = hir_expr_stmt(&hir_main.body, 1);
    let HirExprKind::Block(block) = &block_expr.kind else {
        panic!("expected HIR block expression, got {:?}", block_expr.kind);
    };
    assert!(block.result.is_some(), "expected explicit HIR block result");
}

//= HIR.md#llg-hir-04-normalization-boundary
//= type=test
//# In HIR v0, `observe` MUST remain an explicit HIR statement that preserves both proof expressions and the guarded `else` block.
#[test]
fn requirement_llg_hir_04_preserves_observe_as_an_explicit_hir_statement() {
    let checked = analyze_ok(
        r#"
fn main(value: u32) {
    observe value < 10 else {
        return;
    }
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let main = function(&checked, "main");
    let hir_main = hir_function(&checked, "main");
    let ast_observe = observe_stmt(&main.body, 0);
    let hir_observe = hir_observe_stmt(&hir_main.body, 0);

    assert_eq!(hir_observe.span, ast_observe.span);
    assert!(matches!(hir_observe.left.kind, HirExprKind::Binding(_)));
    assert!(matches!(hir_observe.right.kind, HirExprKind::Int(10)));
    assert_eq!(hir_observe.else_block.statements.len(), 1);
    assert!(matches!(
        hir_observe.else_block.statements[0],
        HirStmt::Return(_)
    ));
}

//= HIR.md#llg-hir-05-successful-hir-well-formedness
//= type=test
//# Successfully checked HIR MUST NOT contain unresolved names or `Unknown` types.
#[test]
fn requirement_llg_hir_05_excludes_unresolved_names_and_unknown_types() {
    let checked = analyze_ok(
        r#"
fn helper(value: u32) -> u32 { value }

fn main(input: u32) {
    let copy = helper(input);
    copy;
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    assert!(
        checked.hir.is_some(),
        "expected well-formed HIR for checked program"
    );

    let missing_type = analyze_ok(
        r#"
fn main() {
    let pending;
}
"#,
    );
    assert!(missing_type.has_errors());
    assert!(missing_type.hir.is_none());
    let pending = let_stmt(&function(&missing_type, "main").body, 0);
    assert_primary_diagnostic(
        &missing_type,
        "let bindings require a type annotation or initializer",
        pending.name.span,
    );

    let empty_array = analyze_ok(
        r#"
fn main() {
    let items = [];
}
"#,
    );
    assert!(empty_array.has_errors());
    assert!(empty_array.hir.is_none());
    let items = let_stmt(&function(&empty_array, "main").body, 0);
    assert_primary_diagnostic(
        &empty_array,
        "empty array literals require an explicit element type",
        items
            .value
            .as_ref()
            .expect("expected empty array initializer")
            .span,
    );
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# The semantic phase MUST resolve host builtin calls without user declarations.
#[test]
fn requirement_llg_wasm_05_resolves_host_builtins_without_user_declarations() {
    let checked = analyze_ok(
        r#"
fn main() -> u32 {
    let value: u32 = read_u32();
    print_u32(value);
    print_bool(true);
    print_newline();
    value
}
"#,
    );

    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
    assert!(checked.resolutions.iter().any(|resolution| {
        resolution.name == "read_u32" && resolution.kind == BindingKind::HostBuiltin
    }));
    assert!(checked.resolutions.iter().any(|resolution| {
        resolution.name == "print_u32" && resolution.kind == BindingKind::HostBuiltin
    }));
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# HIR MUST lower host builtin calls to explicit host builtin callees.
#[test]
fn requirement_llg_wasm_05_lowers_host_builtin_calls_to_explicit_hir_callees() {
    let checked = analyze_ok(
        r#"
fn main() -> u32 {
    print_u32(1);
    0
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let hir_main = hir_function(&checked, "main");
    let call_expr = hir_expr_stmt(&hir_main.body, 0);
    let HirExprKind::Call { callee, args } = &call_expr.kind else {
        panic!("expected HIR call expression, got {:?}", call_expr.kind);
    };

    assert!(matches!(
        callee.kind,
        HirExprKind::HostBuiltin(HostBuiltin::PrintU32)
    ));
    assert_eq!(args.len(), 1);
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# User functions MUST NOT use reserved host builtin names.
#[test]
fn requirement_llg_wasm_05_reserves_host_builtin_names_for_functions() {
    let checked = analyze_ok("fn print_u32(value: u32) {}\n");

    assert!(checked.has_errors());
    assert!(checked
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("reserved for a host builtin")));
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# Host builtin calls MUST NOT create recursion edges.
#[test]
fn requirement_llg_wasm_05_excludes_host_builtin_calls_from_recursion_edges() {
    let checked = analyze_ok(
        r#"
fn main() -> u32 {
    print_u32(1);
    0
}
"#,
    );

    assert!(
        !checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("recursion")),
        "{:#?}",
        checked.diagnostics
    );
}
