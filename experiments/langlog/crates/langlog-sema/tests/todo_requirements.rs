use langlog_sema::{analyze, BindingKind, CheckedProgram};
use langlog_syntax::ast::{Expr, ExprKind, Item, Stmt};
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

//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=test
//# The semantic phase MUST resolve item, parameter, and local bindings according to lexical scope.
#[test]
fn requirement_llg_sema_01_resolves_bindings_according_to_lexical_scope() {
    let checked = analyze_ok(
        r#"
fn helper() {}

fn main(param: u32) {
    let local = helper;
    {
        let helper = local;
        helper
    };
    helper;
    param;
    local;
}
"#,
    );
    assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);

    let helper = function(&checked, "helper");
    let main = function(&checked, "main");
    let outer_let = match &main.body.statements[0] {
        Stmt::Let(stmt) => stmt,
        other => panic!("expected outer let statement, got {other:?}"),
    };
    let inner_block = match &main.body.statements[1] {
        Stmt::Expr(stmt) => match &stmt.expr.kind {
            ExprKind::Block(block) => block,
            other => panic!("expected block expression, got {other:?}"),
        },
        other => panic!("expected block expression statement, got {other:?}"),
    };
    let inner_let = match &inner_block.statements[0] {
        Stmt::Let(stmt) => stmt,
        other => panic!("expected inner let statement, got {other:?}"),
    };
    let outer_helper_expr = match &main.body.statements[2] {
        Stmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected outer helper expression statement, got {other:?}"),
    };
    let param_expr = match &main.body.statements[3] {
        Stmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected param expression statement, got {other:?}"),
    };
    let local_expr = match &main.body.statements[4] {
        Stmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected local expression statement, got {other:?}"),
    };
    let inner_helper_expr = inner_block
        .trailing_expr
        .as_deref()
        .expect("expected trailing expr in inner block");

    let item_resolution = checked
        .resolution(name_span(
            outer_let.value.as_ref().expect("expected let initializer"),
        ))
        .expect("expected helper item resolution");
    assert_eq!(item_resolution.kind, BindingKind::Item);
    assert_eq!(item_resolution.declaration_span, helper.name.span);

    let inner_initializer_resolution = checked
        .resolution(name_span(
            inner_let
                .value
                .as_ref()
                .expect("expected inner let initializer"),
        ))
        .expect("expected outer local resolution inside inner let");
    assert_eq!(inner_initializer_resolution.kind, BindingKind::Local);
    assert_eq!(
        inner_initializer_resolution.declaration_span,
        outer_let.name.span
    );

    let inner_shadow_resolution = checked
        .resolution(name_span(inner_helper_expr))
        .expect("expected inner helper resolution");
    assert_eq!(inner_shadow_resolution.kind, BindingKind::Local);
    assert_eq!(
        inner_shadow_resolution.declaration_span,
        inner_let.name.span
    );

    let outer_helper_resolution = checked
        .resolution(name_span(outer_helper_expr))
        .expect("expected outer helper resolution");
    assert_eq!(outer_helper_resolution.kind, BindingKind::Item);
    assert_eq!(outer_helper_resolution.declaration_span, helper.name.span);

    let param_resolution = checked
        .resolution(name_span(param_expr))
        .expect("expected param resolution");
    assert_eq!(param_resolution.kind, BindingKind::Param);
    assert_eq!(param_resolution.declaration_span, main.params[0].name.span);

    let local_resolution = checked
        .resolution(name_span(local_expr))
        .expect("expected local resolution");
    assert_eq!(local_resolution.kind, BindingKind::Local);
    assert_eq!(local_resolution.declaration_span, outer_let.name.span);
}

//= SPEC.md#llg-sema-01-name-resolution-and-scopes
//= type=test
//# The semantic phase MUST reject references to undefined bindings.
#[test]
fn requirement_llg_sema_01_rejects_references_to_undefined_bindings() {
    let checked = analyze_ok(
        r#"
fn main() {
    missing;
    let local = other;
}
"#,
    );
    assert!(checked.has_errors());

    let main = function(&checked, "main");
    let missing_expr = match &main.body.statements[0] {
        Stmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected missing name expression, got {other:?}"),
    };
    let other_expr = match &main.body.statements[1] {
        Stmt::Let(stmt) => stmt.value.as_ref().expect("expected initializer"),
        other => panic!("expected let statement, got {other:?}"),
    };

    assert_eq!(checked.diagnostics.len(), 2);
    assert!(checked.resolution(name_span(missing_expr)).is_none());
    assert!(checked.resolution(name_span(other_expr)).is_none());

    let missing_span = name_span(missing_expr);
    let other_span = name_span(other_expr);
    assert!(checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "undefined binding `missing`"
            && diagnostic
                .labels
                .iter()
                .any(|label| label.style == LabelStyle::Primary && label.span == missing_span)
    }));
    assert!(checked.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "undefined binding `other`"
            && diagnostic
                .labels
                .iter()
                .any(|label| label.style == LabelStyle::Primary && label.span == other_span)
    }));
}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=todo
//# The semantic phase MUST reject direct recursion.
#[test]
#[ignore = "semantic analysis requirements are not implemented"]
fn todo_llg_sema_02_rejects_direct_recursion() {}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=todo
//# The semantic phase MUST reject indirect recursion.
#[test]
#[ignore = "semantic analysis requirements are not implemented"]
fn todo_llg_sema_02_rejects_indirect_recursion() {}

//= SPEC.md#llg-sema-02-totality-constraints
//= type=todo
//# The semantic phase MUST reject unbounded iteration forms that are outside the bounded phase 1 loop model.
#[test]
#[ignore = "semantic analysis requirements are not implemented"]
fn todo_llg_sema_02_rejects_unbounded_iteration_forms() {}
