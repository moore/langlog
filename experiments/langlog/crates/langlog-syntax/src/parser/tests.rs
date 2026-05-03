use crate::ast::{BinaryOp, ExprKind, Item, ObserveOp, PatternKind, Stmt, TypeKind};
use crate::lexer::lex;
use crate::parser::{parse_lexed, Parser};
use crate::token::TokenTag;

#[test]
fn parses_a_function_with_core_statements() {
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

    let Item::Function(function) = &parsed.module.items[0];
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
