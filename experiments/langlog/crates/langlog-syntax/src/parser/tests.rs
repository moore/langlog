use crate::ast::{BinaryOp, ExprKind, Item, MarkerArgKind, ObserveOp, PatternKind, Stmt, TypeKind};
use crate::lexer::lex;
use crate::parser::{parse_lexed, Parser};
use crate::token::TokenTag;

//= SPEC.md#llg-syn-02-statements
//= type=test
//# The parser MUST preserve accepted statement forms and their nested expression shapes in the AST.
#[test]
fn requirement_llg_syn_02_preserves_statement_ast_shapes() {
    let parsed = parse_lexed(lex(
        "smoke.llg",
        r#"
fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;
    for value in values {
        total = total + value;
    }
    observe total < 100 else {
        return total;
    }
    return total;
}
"#,
    ));

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.module.items.len(), 1);

    let function = match &parsed.module.items[0] {
        Item::Function(function) => function,
        other => panic!("expected function item, got {other:?}"),
    };
    assert_eq!(function.name.value, "sum");
    assert_eq!(function.params.len(), 1);
    assert!(matches!(
        function.return_type.as_ref().map(|ty| &ty.kind),
        Some(TypeKind::Named(_))
    ));
    assert_eq!(function.body.statements.len(), 4);

    assert!(matches!(&function.body.statements[0], Stmt::Let(_)));
    match &function.body.statements[1] {
        Stmt::For(stmt) => {
            assert!(matches!(stmt.binding.kind, PatternKind::Binding(_)));
            assert_eq!(stmt.body.statements.len(), 1);
            match &stmt.body.statements[0] {
                Stmt::Assign(assign) => match &assign.value.kind {
                    ExprKind::Binary { op, .. } => assert_eq!(*op, BinaryOp::Add),
                    other => panic!("expected binary expression, got {other:?}"),
                },
                other => panic!("expected assignment statement, got {other:?}"),
            }
        }
        other => panic!("expected for statement, got {other:?}"),
    }
    match &function.body.statements[2] {
        Stmt::Observe(stmt) => {
            assert!(matches!(stmt.left.kind, ExprKind::Name(_)));
            assert_eq!(stmt.op, ObserveOp::Lt);
            assert!(matches!(stmt.right.kind, ExprKind::Int(100)));
            assert!(matches!(
                stmt.else_block.statements.as_slice(),
                [Stmt::Return(_)]
            ));
        }
        other => panic!("expected observe statement, got {other:?}"),
    }
    assert!(matches!(&function.body.statements[3], Stmt::Return(_)));
}

//= SPEC.md#llg-mark-01-marker-model
//= type=test
//# The parser MUST preserve marker-qualified types, marker names, and marker place arguments in the AST.
#[test]
fn requirement_llg_mark_01_preserves_marker_qualified_types_in_ast() {
    let parsed = parse_lexed(lex(
        "markers.llg",
        r#"
fn keep(value: u32 with (Event, LessThan(value, values.length)), values: [u32; 4]) -> u32 with Event {
    unsafe { Event::mark(value); }
    let copied: u32 with Event = unsafe { Event::mark(value) };
    copied
}
"#,
    ));

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let function = match &parsed.module.items[0] {
        Item::Function(function) => function,
        other => panic!("expected function item, got {other:?}"),
    };

    let TypeKind::With { markers, .. } = &function.params[0].ty.kind else {
        panic!("expected marker-qualified parameter type");
    };
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].name.value, "Event");
    assert_eq!(markers[1].name.value, "LessThan");
    assert!(matches!(markers[1].args[0].kind, MarkerArgKind::Name(_)));
    assert!(matches!(
        markers[1].args[1].kind,
        MarkerArgKind::Field { .. }
    ));

    assert!(matches!(
        function.return_type.as_ref().map(|ty| &ty.kind),
        Some(TypeKind::With { .. })
    ));
    assert!(matches!(
        function.body.statements.as_slice(),
        [Stmt::UnsafeMarker(_), Stmt::Let(_)]
    ));
    let Stmt::Let(stmt) = &function.body.statements[1] else {
        panic!("expected marker let statement");
    };
    assert!(matches!(
        stmt.value.as_ref().map(|expr| &expr.kind),
        Some(ExprKind::UnsafeMarker(_))
    ));
}

//= SPEC.md#llg-mark-03-marker-construction
//= type=test
//# Marker constructor syntax outside `unsafe` MUST be rejected with a syntax diagnostic.
#[test]
fn requirement_llg_mark_03_rejects_marker_constructors_outside_unsafe() {
    let parsed = parse_lexed(lex(
        "markers.llg",
        "fn main(value: u32) { Event::mark(value); }",
    ));

    assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("marker constructors must be inside `unsafe`")));
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# The AST for a binary expression MUST group operands according to the specified operator precedence and associativity rules.
#[test]
fn requirement_llg_syn_03_groups_binary_expressions_by_operator_precedence() {
    let lexed = lex("expr.llg", "a + b * c - d");
    let mut parser = Parser::new(&lexed.source, &lexed.tokens, lexed.diagnostics.clone());
    let expr = parser
        .parse_expression(11)
        .expect("expected expression at minimum binding power");

    assert!(parser.diagnostics.is_empty(), "{:#?}", parser.diagnostics);
    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Sub,
            left,
            right,
        } => {
            assert!(matches!(right.kind, ExprKind::Name(_)));
            match left.kind {
                ExprKind::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                } => {
                    assert!(matches!(left.kind, ExprKind::Name(_)));
                    assert!(matches!(
                        right.kind,
                        ExprKind::Binary {
                            op: BinaryOp::Mul,
                            ..
                        }
                    ));
                }
                other => panic!("expected additive left operand, got {other:?}"),
            }
        }
        other => panic!("expected left-associated subtract expression, got {other:?}"),
    }
    assert!(parser.at(TokenTag::Eof));
}
