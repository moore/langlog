use crate::ast::{
    BinaryOp, Expr, ExprKind, Function, GenericArg, Item, ObserveOp, PatternKind, Stmt, Task,
    TypeKind, UnaryOp,
};
use crate::diagnostic::LabelStyle;
use crate::lexer::lex;
use crate::parser::ParsedModule;
use crate::token::{TokenKind, TokenTag};

fn parse_program(source: &str) -> ParsedModule {
    crate::parse("requirement.llg", source)
}

fn parse_ok(source: &str) -> ParsedModule {
    let parsed = parse_program(source);
    assert!(
        !parsed.has_errors(),
        "expected parse success, got diagnostics: {:#?}",
        parsed.diagnostics
    );
    parsed
}

fn parse_err(source: &str) -> ParsedModule {
    let parsed = parse_program(source);
    assert!(parsed.has_errors(), "expected parse error");
    parsed
}

fn first_function(parsed: &ParsedModule) -> &Function {
    match &parsed.module.items[0] {
        Item::Function(function) => function,
        other => panic!("expected function item, got {other:?}"),
    }
}

fn first_task(parsed: &ParsedModule) -> &Task {
    match &parsed.module.items[0] {
        Item::Task(task) => task,
        other => panic!("expected task item, got {other:?}"),
    }
}

fn parse_trailing_expr(expr_source: &str) -> Expr {
    let parsed = parse_ok(&format!("fn main() {{ {expr_source} }}"));
    first_function(&parsed)
        .body
        .trailing_expr
        .as_deref()
        .expect("expected trailing expression")
        .clone()
}

fn expr_stmt(stmt: &Stmt) -> &Expr {
    match stmt {
        Stmt::Expr(stmt) => &stmt.expr,
        other => panic!("expected expression statement, got {other:?}"),
    }
}

fn observe_stmt(stmt: &Stmt) -> &crate::ast::ObserveStmt {
    match stmt {
        Stmt::Observe(stmt) => stmt,
        other => panic!("expected observe statement, got {other:?}"),
    }
}

fn assert_diagnostic_contains(parsed: &ParsedModule, expected: &str) {
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(expected)),
        "missing diagnostic containing {expected:?}: {:#?}",
        parsed.diagnostics
    );
}

//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST ignore line comments beginning with `//`.
#[test]
fn requirement_llg_lex_01_ignores_line_comments() {
    let lexed = lex("requirement.llg", "// comment\nfn main() {}");

    assert!(lexed.diagnostics.is_empty());
    assert_eq!(lexed.tokens[0].tag(), TokenTag::Fn);
}

//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST ignore block comments delimited by `/*` and `*/`.
#[test]
fn requirement_llg_lex_01_ignores_block_comments() {
    let lexed = lex("requirement.llg", "/* comment */ fn main() {}");

    assert!(lexed.diagnostics.is_empty());
    assert_eq!(lexed.tokens[0].tag(), TokenTag::Fn);
}

//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST support nested block comments.
#[test]
fn requirement_llg_lex_01_supports_nested_block_comments() {
    let lexed = lex(
        "requirement.llg",
        "/* outer /* inner */ still outer */ fn main() {}",
    );

    assert!(lexed.diagnostics.is_empty());
    assert_eq!(lexed.tokens[0].tag(), TokenTag::Fn);
}

//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST report an error for an unterminated block comment.
#[test]
fn requirement_llg_lex_01_reports_unterminated_block_comments() {
    let lexed = lex("requirement.llg", "/* unterminated");

    assert_eq!(lexed.diagnostics.len(), 1);
    assert!(lexed.diagnostics[0]
        .message
        .contains("unterminated block comment"));
}

//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII letters, digits, or `_`.
#[test]
fn requirement_llg_lex_02_accepts_identifier_shape() {
    let lexed = lex("requirement.llg", "_name9");

    assert!(matches!(
        &lexed.tokens[0].kind,
        TokenKind::Identifier(name) if name == "_name9"
    ));
}

//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Integer literals MUST be parsed as unsigned base-10 integers.
#[test]
fn requirement_llg_lex_02_parses_base_10_u64_integer_literals() {
    let lexed = lex("requirement.llg", "12345");

    assert!(matches!(lexed.tokens[0].kind, TokenKind::IntLiteral(12345)));
}

//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Boolean literals MUST include `true` and `false`.
#[test]
fn requirement_llg_lex_02_recognizes_boolean_literals() {
    let lexed = lex("requirement.llg", "true false");
    let tags: Vec<_> = lexed.tokens.iter().map(|token| token.tag()).collect();

    assert_eq!(tags, vec![TokenTag::True, TokenTag::False, TokenTag::Eof]);
}

//= SPEC.md#llg-lex-03-reserved-keywords
//= type=test
//# The keyword set MUST reserve `fn`, `task`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`, `forever`, `return`, `exit`, `delegate`, `observe`, `or`, `true`, and `false`.
#[test]
fn requirement_llg_lex_03_reserves_keywords() {
    let lexed = lex(
        "requirement.llg",
        "fn task let mut if else match for in forever return exit delegate observe or true false",
    );
    let tags: Vec<_> = lexed.tokens.iter().map(|token| token.tag()).collect();

    assert_eq!(
        tags,
        vec![
            TokenTag::Fn,
            TokenTag::Task,
            TokenTag::Let,
            TokenTag::Mut,
            TokenTag::If,
            TokenTag::Else,
            TokenTag::Match,
            TokenTag::For,
            TokenTag::In,
            TokenTag::Forever,
            TokenTag::Return,
            TokenTag::Exit,
            TokenTag::Delegate,
            TokenTag::Observe,
            TokenTag::Or,
            TokenTag::True,
            TokenTag::False,
            TokenTag::Eof,
        ]
    );
}

//= SPEC.md#llg-lex-04-lexical-error-diagnostics
//= type=test
//# Lexical diagnostics for invalid characters MUST include a primary span covering the offending character.
#[test]
fn requirement_llg_lex_04_marks_invalid_character_spans() {
    let lexed = lex("requirement.llg", "@");

    assert_eq!(lexed.diagnostics.len(), 1);
    let label = &lexed.diagnostics[0].labels[0];
    assert_eq!(label.style, LabelStyle::Primary);
    assert_eq!(lexed.source.span_text(label.span), Some("@"));
}

//= SPEC.md#llg-syn-01-top-level-items
//= type=test
//# A phase 1 source file MUST contain only function items and task items at the top level.
#[test]
fn requirement_llg_syn_01_accepts_function_and_task_top_level_items() {
    let parsed = parse_ok(
        r#"
fn main() {}
fn helper() {}
task worker() -> u32 { exit 0; }
"#,
    );

    assert_eq!(parsed.module.items.len(), 3);
    assert!(matches!(parsed.module.items[0], Item::Function(_)));
    assert!(matches!(parsed.module.items[1], Item::Function(_)));
    assert!(matches!(parsed.module.items[2], Item::Task(_)));
}

//= SPEC.md#llg-syn-01-top-level-items
//= type=test
//# A non-function, non-task top-level item MUST be rejected with a syntax diagnostic.
#[test]
fn requirement_llg_syn_01_rejects_non_item_top_level_forms_with_a_syntax_diagnostic() {
    let parsed = parse_err("let value = 1;");

    assert_diagnostic_contains(&parsed, "expected a top-level item");
}

//= SPEC.md#llg-syn-01-top-level-items
//= type=test
//# A function item MUST use Rust-like syntax with `fn`, a name, a parameter list, and a block body.
#[test]
fn requirement_llg_syn_01_parses_function_item_syntax() {
    let parsed = parse_ok("fn main(value: u32) -> u32 { value }");
    let function = first_function(&parsed);

    assert_eq!(function.name.value, "main");
    assert_eq!(function.params.len(), 1);
    assert!(function.return_type.is_some());
    assert!(function.body.trailing_expr.is_some());
}

//= SPEC.md#llg-syn-01-top-level-items
//= type=test
//# The current parser allows the return type to be omitted in phase 1.
#[test]
fn requirement_llg_syn_01_allows_omitted_return_types() {
    let parsed = parse_ok("fn helper() {}");
    let function = first_function(&parsed);

    assert!(function.return_type.is_none());
}

//= SPEC.md#llg-syn-01-top-level-items
//= type=test
//# A task item MUST use the form `task name(param: Type, ...) -> Type { ... }`.
#[test]
fn requirement_llg_syn_01_parses_task_item_syntax() {
    let parsed = parse_ok("task main(value: u32) -> u32 { exit value; }");
    let task = first_task(&parsed);

    assert_eq!(task.name.value, "main");
    assert_eq!(task.params.len(), 1);
    assert!(matches!(task.return_type.kind, TypeKind::Named(_)));
    assert!(matches!(task.body.statements.as_slice(), [Stmt::Exit(_)]));
}

//= SPEC.md#llg-syn-01-top-level-items
//= type=test
//# A task item MUST include an explicit return type.
#[test]
fn requirement_llg_syn_01_rejects_task_items_without_return_types() {
    let parsed = parse_err("task main() { exit 0; }");

    assert_diagnostic_contains(&parsed, "expected `->` before task return type");
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# The parser MUST accept `let`, assignment, expression, `if`, `match`, `for`, `return`, and `observe` statements.
#[test]
fn requirement_llg_syn_02_parses_statement_forms() {
    let parsed = parse_ok(
        r#"
fn main() {
    let mut total: u32 = 0;
    total = total + 1;
    total;
    if true {
        observe total > 0 else {
            return;
        }
    } else {
        observe total == 0 else {
            return;
        }
    }
    match true {
        true => { total = 1; },
        false => { total = 2; }
    }
    for value in [1, 2] {
        total = total + value;
    }
    observe total > 0 else {
        return;
    }
    return;
}
"#,
    );

    let statements = &first_function(&parsed).body.statements;
    assert!(matches!(statements[0], Stmt::Let(_)));
    assert!(matches!(statements[1], Stmt::Assign(_)));
    assert!(matches!(statements[2], Stmt::Expr(_)));
    assert!(matches!(statements[3], Stmt::If(_)));
    assert!(matches!(statements[4], Stmt::Match(_)));
    assert!(matches!(statements[5], Stmt::For(_)));
    assert!(matches!(statements[6], Stmt::Observe(_)));
    assert!(matches!(statements[7], Stmt::Return(_)));
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# The task-orchestration parser MUST additionally accept `forever`, `exit`, and `delegate` statements.
#[test]
fn requirement_llg_syn_02_parses_task_statement_forms() {
    let parsed = parse_ok(
        r#"
task main() -> u32 {
    forever {
        tick();
    }
    delegate worker(1);
    exit 0;
}
"#,
    );

    let statements = &first_task(&parsed).body.statements;
    assert!(matches!(statements[0], Stmt::Forever(_)));
    assert!(matches!(statements[1], Stmt::Delegate(_)));
    assert!(matches!(statements[2], Stmt::Exit(_)));
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# The current parser allows a `let` statement to include `mut`, a type annotation, and an initializer.
#[test]
fn requirement_llg_syn_02_allows_mut_type_and_initializer_on_let() {
    let parsed = parse_ok("fn main() { let mut total: u32 = 0; }");
    let statements = &first_function(&parsed).body.statements;
    let Stmt::Let(let_stmt) = &statements[0] else {
        panic!("expected let statement, got {:?}", statements[0]);
    };

    assert!(let_stmt.mutable);
    assert!(let_stmt.ty.is_some());
    assert!(let_stmt.value.is_some());
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# A statement form that requires a semicolon MUST reject the form if the semicolon is absent.
#[test]
fn requirement_llg_syn_02_rejects_missing_semicolons() {
    let missing_let_semi = parse_err("fn main() { let value = 1 }");
    let missing_expr_semi = parse_err(
        r#"
fn main() {
    1
    observe value == 1 else {
        return;
    }
}
"#,
    );

    assert_diagnostic_contains(&missing_let_semi, "expected `;` after `let` statement");
    assert_diagnostic_contains(&missing_expr_semi, "expected `;` or `}` after expression");
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# The parser MUST accept integer literals, boolean literals, names, tuples, arrays, blocks, grouped expressions, unary operators, binary operators, calls, and indexing expressions.
#[test]
fn requirement_llg_syn_03_parses_expression_forms() {
    let parsed = parse_ok(
        r#"
fn main() {
    let name = 1;
    let arr = [1, 2];
    1;
    true;
    name;
    (1, true);
    [1, 2];
    { 1 };
    (name);
    !true;
    1 + 2;
    call(1);
    arr[0];
}
"#,
    );
    let statements = &first_function(&parsed).body.statements;

    assert_eq!(statements.len(), 13);
    assert!(matches!(expr_stmt(&statements[2]).kind, ExprKind::Int(1)));
    assert!(matches!(
        expr_stmt(&statements[3]).kind,
        ExprKind::Bool(true)
    ));
    assert!(matches!(expr_stmt(&statements[4]).kind, ExprKind::Name(_)));
    assert!(matches!(
        &expr_stmt(&statements[5]).kind,
        ExprKind::Tuple(elements) if elements.len() == 2
    ));
    assert!(matches!(
        &expr_stmt(&statements[6]).kind,
        ExprKind::Array(elements) if elements.len() == 2
    ));
    assert!(matches!(
        &expr_stmt(&statements[7]).kind,
        ExprKind::Block(block)
            if block.statements.is_empty()
                && matches!(
                    block.trailing_expr.as_deref().map(|expr| &expr.kind),
                    Some(ExprKind::Int(1))
                )
    ));
    assert!(matches!(
        expr_stmt(&statements[8]).kind,
        ExprKind::Grouped(_)
    ));
    assert!(matches!(
        expr_stmt(&statements[9]).kind,
        ExprKind::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[10]).kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
    assert!(matches!(
        &expr_stmt(&statements[11]).kind,
        ExprKind::Call { args, .. } if args.len() == 1
    ));
    assert!(matches!(
        expr_stmt(&statements[12]).kind,
        ExprKind::Index { .. }
    ));
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# The supported binary operators MUST include `..`, `||`, `&&`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/`, and `%`.
#[test]
fn requirement_llg_syn_03_supports_the_full_binary_operator_set() {
    let parsed = parse_ok(
        r#"
fn main() {
    a .. b;
    a || b;
    a && b;
    a == b;
    a != b;
    a < b;
    a <= b;
    a > b;
    a >= b;
    a + b;
    a - b;
    a * b;
    a / b;
    a % b;
}
"#,
    );
    let statements = &first_function(&parsed).body.statements;

    assert!(matches!(
        expr_stmt(&statements[0]).kind,
        ExprKind::Binary {
            op: BinaryOp::Range,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[1]).kind,
        ExprKind::Binary {
            op: BinaryOp::Or,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[2]).kind,
        ExprKind::Binary {
            op: BinaryOp::And,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[3]).kind,
        ExprKind::Binary {
            op: BinaryOp::EqEq,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[4]).kind,
        ExprKind::Binary {
            op: BinaryOp::NotEq,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[5]).kind,
        ExprKind::Binary {
            op: BinaryOp::Lt,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[6]).kind,
        ExprKind::Binary {
            op: BinaryOp::LtEq,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[7]).kind,
        ExprKind::Binary {
            op: BinaryOp::Gt,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[8]).kind,
        ExprKind::Binary {
            op: BinaryOp::GtEq,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[9]).kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[10]).kind,
        ExprKind::Binary {
            op: BinaryOp::Sub,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[11]).kind,
        ExprKind::Binary {
            op: BinaryOp::Mul,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[12]).kind,
        ExprKind::Binary {
            op: BinaryOp::Div,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[13]).kind,
        ExprKind::Binary {
            op: BinaryOp::Rem,
            ..
        }
    ));
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Binary operators with the same precedence MUST associate to the left.
#[test]
fn requirement_llg_syn_03_left_associates_same_precedence_binary_operators() {
    let additive = parse_trailing_expr("8 - 4 + 2");
    let rem = parse_trailing_expr("20 / 5 % 2");

    match additive.kind {
        ExprKind::Binary {
            op: BinaryOp::Add,
            left,
            right,
        } => {
            assert!(matches!(right.kind, ExprKind::Int(2)));
            assert!(matches!(
                left.kind,
                ExprKind::Binary {
                    op: BinaryOp::Sub,
                    ..
                }
            ));
        }
        other => panic!("expected left-associated additive expression, got {other:?}"),
    }

    match rem.kind {
        ExprKind::Binary {
            op: BinaryOp::Rem,
            left,
            right,
        } => {
            assert!(matches!(right.kind, ExprKind::Int(2)));
            assert!(matches!(
                left.kind,
                ExprKind::Binary {
                    op: BinaryOp::Div,
                    ..
                }
            ));
        }
        other => panic!("expected remainder expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Postfix call and indexing MUST bind tighter than unary operators.
#[test]
fn requirement_llg_syn_03_postfix_binds_tighter_than_unary() {
    let expr = parse_trailing_expr("-f(1)[0]");

    match expr.kind {
        ExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } => match expr.kind {
            ExprKind::Index { target, .. } => match target.kind {
                ExprKind::Call { .. } => {}
                other => panic!("expected call inside index, got {other:?}"),
            },
            other => panic!("expected index expression, got {other:?}"),
        },
        other => panic!("expected unary expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Unary operators MUST bind tighter than multiplicative operators.
#[test]
fn requirement_llg_syn_03_unary_binds_tighter_than_multiplicative() {
    let expr = parse_trailing_expr("-a * b");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Mul,
            left,
            ..
        } => match left.kind {
            ExprKind::Unary {
                op: UnaryOp::Neg, ..
            } => {}
            other => panic!("expected unary left operand, got {other:?}"),
        },
        other => panic!("expected multiplicative expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Multiplicative operators MUST bind tighter than additive operators.
#[test]
fn requirement_llg_syn_03_multiplicative_binds_tighter_than_additive() {
    let expr = parse_trailing_expr("1 + 2 * 3");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Add,
            right,
            ..
        } => match right.kind {
            ExprKind::Binary {
                op: BinaryOp::Mul, ..
            } => {}
            other => panic!("expected multiplicative right operand, got {other:?}"),
        },
        other => panic!("expected additive expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Additive operators MUST bind tighter than comparison operators.
#[test]
fn requirement_llg_syn_03_additive_binds_tighter_than_comparison() {
    let expr = parse_trailing_expr("1 + 2 < 4");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Lt,
            left,
            ..
        } => match left.kind {
            ExprKind::Binary {
                op: BinaryOp::Add, ..
            } => {}
            other => panic!("expected additive left operand, got {other:?}"),
        },
        other => panic!("expected comparison expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Comparison operators MUST bind tighter than equality operators.
#[test]
fn requirement_llg_syn_03_comparison_binds_tighter_than_equality() {
    let expr = parse_trailing_expr("1 < 2 == 3 < 4");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::EqEq,
            left,
            right,
        } => {
            assert!(matches!(
                left.kind,
                ExprKind::Binary {
                    op: BinaryOp::Lt,
                    ..
                }
            ));
            assert!(matches!(
                right.kind,
                ExprKind::Binary {
                    op: BinaryOp::Lt,
                    ..
                }
            ));
        }
        other => panic!("expected equality expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Equality operators MUST bind tighter than logical and.
#[test]
fn requirement_llg_syn_03_equality_binds_tighter_than_logical_and() {
    let expr = parse_trailing_expr("1 == 2 && 3 == 4");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::And,
            left,
            right,
        } => {
            assert!(matches!(
                left.kind,
                ExprKind::Binary {
                    op: BinaryOp::EqEq,
                    ..
                }
            ));
            assert!(matches!(
                right.kind,
                ExprKind::Binary {
                    op: BinaryOp::EqEq,
                    ..
                }
            ));
        }
        other => panic!("expected logical and expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Logical and MUST bind tighter than logical or.
#[test]
fn requirement_llg_syn_03_logical_and_binds_tighter_than_logical_or() {
    let expr = parse_trailing_expr("a && b || c && d");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Or,
            left,
            right,
        } => {
            assert!(matches!(
                left.kind,
                ExprKind::Binary {
                    op: BinaryOp::And,
                    ..
                }
            ));
            assert!(matches!(
                right.kind,
                ExprKind::Binary {
                    op: BinaryOp::And,
                    ..
                }
            ));
        }
        other => panic!("expected logical or expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Logical or MUST bind tighter than range construction.
#[test]
fn requirement_llg_syn_03_logical_or_binds_tighter_than_range() {
    let expr = parse_trailing_expr("a || b .. c || d");

    match expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Range,
            left,
            right,
        } => {
            assert!(matches!(
                left.kind,
                ExprKind::Binary {
                    op: BinaryOp::Or,
                    ..
                }
            ));
            assert!(matches!(
                right.kind,
                ExprKind::Binary {
                    op: BinaryOp::Or,
                    ..
                }
            ));
        }
        other => panic!("expected range expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Recovery expressions MUST parse `expr or fallback` and `expr or(err) fallback`, and recovery MUST bind looser than range construction.
#[test]
fn requirement_llg_syn_03_parses_recovery_expressions_at_lowest_precedence() {
    let option_recovery = parse_trailing_expr("1 .. 2 or 3");
    match option_recovery.kind {
        ExprKind::Recover {
            expr,
            error_binding,
            fallback,
        } => {
            assert!(error_binding.is_none());
            assert!(matches!(
                expr.kind,
                ExprKind::Binary {
                    op: BinaryOp::Range,
                    ..
                }
            ));
            assert!(matches!(fallback.kind, ExprKind::Int(3)));
        }
        other => panic!("expected recovery expression, got {other:?}"),
    }

    let result_recovery = parse_trailing_expr("1 + 2 or(err) 0");
    match result_recovery.kind {
        ExprKind::Recover {
            error_binding,
            fallback,
            ..
        } => {
            let binding = error_binding.expect("expected recovery error binding");
            assert_eq!(binding.value, "err");
            assert!(matches!(fallback.kind, ExprKind::Int(0)));
        }
        other => panic!("expected recovery expression, got {other:?}"),
    }
}

//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `()` MUST parse as an empty tuple expression.
#[test]
fn requirement_llg_syn_04_parses_empty_tuple_expressions() {
    let parsed = parse_ok("fn main() { (); }");
    let statements = &first_function(&parsed).body.statements;

    assert!(matches!(
        &expr_stmt(&statements[0]).kind,
        ExprKind::Tuple(elements) if elements.is_empty()
    ));
}

//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `(expr)` MUST parse as a grouped expression.
#[test]
fn requirement_llg_syn_04_parses_grouped_expressions() {
    let parsed = parse_ok("fn main() { (1); }");
    let statements = &first_function(&parsed).body.statements;

    assert!(matches!(
        expr_stmt(&statements[0]).kind,
        ExprKind::Grouped(_)
    ));
}

//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `(expr,)` MUST parse as a single-element tuple expression.
#[test]
fn requirement_llg_syn_04_parses_singleton_tuple_expressions() {
    let parsed = parse_ok("fn main() { (1,); }");
    let statements = &first_function(&parsed).body.statements;

    assert!(matches!(
        &expr_stmt(&statements[0]).kind,
        ExprKind::Tuple(elements) if elements.len() == 1
    ));
}

//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `(a, b, ...)` MUST parse as a tuple expression.
#[test]
fn requirement_llg_syn_04_parses_tuple_expressions() {
    let parsed = parse_ok("fn main() { (1, 2, 3); }");
    let statements = &first_function(&parsed).body.statements;

    assert!(matches!(
        &expr_stmt(&statements[0]).kind,
        ExprKind::Tuple(elements) if elements.len() == 3
    ));
}

//= SPEC.md#llg-syn-05-patterns-and-match-arms
//= type=test
//# The parser MUST accept wildcard, binding, integer literal, and boolean patterns.
#[test]
fn requirement_llg_syn_05_accepts_core_pattern_forms() {
    let parsed = parse_ok(
        r#"
fn main() {
    match 1 {
        _ => 0,
        value => value,
        7 => 1,
        false => 2
    }
}
"#,
    );
    let stmt = &first_function(&parsed).body.statements[0];
    let Stmt::Match(match_stmt) = stmt else {
        panic!("expected match statement, got {stmt:?}");
    };

    assert_eq!(match_stmt.arms.len(), 4);
    assert!(matches!(
        match_stmt.arms[0].pattern.kind,
        PatternKind::Wildcard
    ));
    assert!(matches!(
        match_stmt.arms[1].pattern.kind,
        PatternKind::Binding(_)
    ));
    assert!(matches!(
        match_stmt.arms[2].pattern.kind,
        PatternKind::Int(7)
    ));
    assert!(matches!(
        match_stmt.arms[3].pattern.kind,
        PatternKind::Bool(false)
    ));
}

//= SPEC.md#llg-syn-05-patterns-and-match-arms
//= type=test
//# `match` arms MUST use `pattern => body`.
#[test]
fn requirement_llg_syn_05_requires_match_arms_to_use_fat_arrow() {
    let parsed = parse_err(
        r#"
fn main() {
    match 1 {
        _ -> 0
    }
}
"#,
    );

    assert_diagnostic_contains(&parsed, "expected `=>` after match pattern");
}

//= SPEC.md#llg-syn-05-patterns-and-match-arms
//= type=test
//# `match` arms MUST be comma-separated and MAY end with a trailing comma.
#[test]
fn requirement_llg_syn_05_supports_comma_separated_match_arms_with_trailing_commas() {
    let without_trailing_comma = parse_ok(
        r#"
fn main() {
    match 1 {
        _ => 0,
        7 => 1,
        false => 2
    }
}
"#,
    );
    let with_trailing_comma = parse_ok(
        r#"
fn main() {
    match 1 {
        _ => 0,
        7 => 1,
        false => 2,
    }
}
"#,
    );

    for parsed in [&without_trailing_comma, &with_trailing_comma] {
        let stmt = &first_function(parsed).body.statements[0];
        let Stmt::Match(match_stmt) = stmt else {
            panic!("expected match statement, got {stmt:?}");
        };

        assert_eq!(match_stmt.arms.len(), 3);
    }
}

//= SPEC.md#llg-syn-06-observe-statements
//= type=test
//# `observe` statements MUST use the form `observe <expr> <op> <expr> else <block>`.
#[test]
fn requirement_llg_syn_06_parses_relational_observe_statements() {
    let parsed = parse_ok(
        "fn main(value: u32, limit: u32) { observe value + 1 <= limit * 2 else { return; } }",
    );
    let observe = observe_stmt(&first_function(&parsed).body.statements[0]);

    assert!(matches!(
        observe.left.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
    assert_eq!(observe.op, ObserveOp::LtEq);
    assert!(matches!(
        observe.right.kind,
        ExprKind::Binary {
            op: BinaryOp::Mul,
            ..
        }
    ));
    assert!(matches!(
        observe.else_block.statements.as_slice(),
        [Stmt::Return(_)]
    ));
}

//= SPEC.md#llg-syn-06-observe-statements
//= type=test
//# An `observe` statement without an `else` block MUST be rejected with a syntax diagnostic.
#[test]
fn requirement_llg_syn_06_rejects_missing_else_blocks() {
    let parsed = parse_err("fn main(limit: u32) { observe limit <= 10 }");

    assert_diagnostic_contains(&parsed, "expected `else` after `observe`");
}

//= SPEC.md#llg-syn-06-observe-statements
//= type=test
//# The left-hand side of `observe` MUST accept the same phase 1 proof expression forms as the right-hand side.
#[test]
fn requirement_llg_syn_06_accepts_proof_expressions_on_the_left_hand_side() {
    let parsed = parse_ok(
        "fn main(values: [u32; 4], index: u32, limit: u32) { observe values[index] + 1 < limit else { return; } }",
    );
    let observe = observe_stmt(&first_function(&parsed).body.statements[0]);

    assert!(matches!(
        observe.left.kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
    assert!(matches!(observe.right.kind, ExprKind::Name(_)));
}

//= SPEC.md#llg-syn-06-observe-statements
//= type=test
//# The phase 1 `observe` operator set MUST include `==`, `!=`, `<`, `<=`, `>`, and `>=`.
#[test]
fn requirement_llg_syn_06_supports_the_phase_1_observe_operator_set() {
    let parsed = parse_ok(
        r#"
fn main(value: u32, limit: u32) {
    observe value == limit else { return; }
    observe value != limit else { return; }
    observe value < limit else { return; }
    observe value <= limit else { return; }
    observe value > limit else { return; }
    observe value >= limit else { return; }
}
"#,
    );
    let statements = &first_function(&parsed).body.statements;

    assert_eq!(observe_stmt(&statements[0]).op, ObserveOp::Eq);
    assert_eq!(observe_stmt(&statements[1]).op, ObserveOp::NotEq);
    assert_eq!(observe_stmt(&statements[2]).op, ObserveOp::Lt);
    assert_eq!(observe_stmt(&statements[3]).op, ObserveOp::LtEq);
    assert_eq!(observe_stmt(&statements[4]).op, ObserveOp::Gt);
    assert_eq!(observe_stmt(&statements[5]).op, ObserveOp::GtEq);
}

//= SPEC.md#llg-syn-06-observe-statements
//= type=test
//# In phase 1, `observe` proof expressions MUST reject tuple, array, block, range, logical, equality, and comparison subexpressions.
#[test]
fn requirement_llg_syn_06_rejects_non_proof_expression_operands() {
    let tuple_left = parse_err("fn main(value: u32) { observe (1, 2) == value else { return; } }");
    let array_right = parse_err("fn main(value: u32) { observe value == [1, 2] else { return; } }");
    let block_right = parse_err("fn main(value: u32) { observe value == { 1 } else { return; } }");
    let range_right =
        parse_err("fn main(value: u32) { observe value == (0 .. 4) else { return; } }");
    let equality_left = parse_err(
        "fn main(a: u32, b: u32, c: u32) { observe ((a == b)) + c < 10 else { return; } }",
    );
    let comparison_left = parse_err(
        "fn main(a: u32, b: u32, c: u32) { observe ((a < b)) + c < 10 else { return; } }",
    );
    let logical_right =
        parse_err("fn main(a: u32, b: bool, c: bool) { observe a < (b && c) else { return; } }");

    for parsed in [
        &tuple_left,
        &array_right,
        &block_right,
        &range_right,
        &equality_left,
        &comparison_left,
        &logical_right,
    ] {
        assert_diagnostic_contains(
            parsed,
            "phase 1 `observe` operands must be proof expressions",
        );
    }
}

//= SPEC.md#llg-syn-06-observe-statements
//= type=test
//# In phase 1, `observe` proof expressions MUST reject non-proof call callees, call arguments, index targets, and index values.
#[test]
fn requirement_llg_syn_06_rejects_non_proof_call_and_index_subexpressions() {
    let invalid_call_callee = parse_err("fn main() { observe ({ 1 })(2) == 2 else { return; } }");
    let invalid_call_arg = parse_err("fn main(f: u32) { observe f([1]) == 1 else { return; } }");
    let invalid_index_target = parse_err("fn main() { observe [1][0] == 1 else { return; } }");
    let invalid_index_value =
        parse_err("fn main(values: [u32; 4]) { observe values[[0]] == 1 else { return; } }");

    for parsed in [
        &invalid_call_callee,
        &invalid_call_arg,
        &invalid_index_target,
        &invalid_index_value,
    ] {
        assert_diagnostic_contains(
            parsed,
            "phase 1 `observe` operands must be proof expressions",
        );
    }
}

//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# The parser MUST accept unit, named, tuple, fixed-array, and generic application type forms.
#[test]
fn requirement_llg_type_01_parses_core_type_forms() {
    let parsed = parse_ok(
        "fn main(a: (), b: u32, c: (u32, bool), d: [u32; 4], e: Option<u32>, f: Result<u32, Error>) -> () { () }",
    );
    let function = first_function(&parsed);

    assert_eq!(function.params.len(), 6);
    assert!(matches!(
        function.return_type.as_ref().map(|ty| &ty.kind),
        Some(TypeKind::Unit)
    ));
    assert!(matches!(function.params[1].ty.kind, TypeKind::Named(_)));
    assert!(matches!(function.params[2].ty.kind, TypeKind::Tuple(_)));
    assert!(matches!(function.params[3].ty.kind, TypeKind::Array { .. }));
    assert!(matches!(
        function.params[4].ty.kind,
        TypeKind::Applied { .. }
    ));
    assert!(matches!(
        function.params[5].ty.kind,
        TypeKind::Applied { .. }
    ));
}

//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# A fixed-array type MUST use the form `[T; N]`.
#[test]
fn requirement_llg_type_01_uses_semicolon_syntax_for_fixed_array_types() {
    let parsed = parse_ok("fn main(values: [u32; 4]) {}");
    let invalid = parse_err("fn main(values: [u32, 4]) {}");

    match &first_function(&parsed).params[0].ty.kind {
        TypeKind::Array { element, length } => {
            assert!(matches!(element.kind, TypeKind::Named(_)));
            assert_eq!(length.value, 4);
        }
        other => panic!("expected array type, got {other:?}"),
    }

    assert_diagnostic_contains(&invalid, "expected `;` in array type");
}

//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `()` MUST parse as the unit type.
#[test]
fn requirement_llg_type_02_parses_the_unit_type() {
    let parsed = parse_ok("fn main(value: ()) {}");

    assert!(matches!(
        &first_function(&parsed).params[0].ty.kind,
        TypeKind::Unit
    ));
}

//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `(T)` MUST parse as a grouped type and MUST NOT create a tuple type.
#[test]
fn requirement_llg_type_02_parses_grouped_types() {
    let parsed = parse_ok("fn main(value: (u32)) {}");

    assert!(matches!(
        &first_function(&parsed).params[0].ty.kind,
        TypeKind::Named(name) if name.value == "u32"
    ));
}

//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `(T,)` MUST parse as a single-element tuple type.
#[test]
fn requirement_llg_type_02_parses_singleton_tuple_types() {
    let parsed = parse_ok("fn main(value: (u32,)) {}");

    assert!(matches!(
        &first_function(&parsed).params[0].ty.kind,
        TypeKind::Tuple(elements) if elements.len() == 1
    ));
}

//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `(A, B, ...)` MUST parse as a tuple type.
#[test]
fn requirement_llg_type_02_parses_tuple_types() {
    let parsed = parse_ok("fn main(value: (u32, bool, u8)) {}");

    assert!(matches!(
        &first_function(&parsed).params[0].ty.kind,
        TypeKind::Tuple(elements) if elements.len() == 3
    ));
}

//= SPEC.md#llg-type-03-bounded-collection-type-arity
//= type=test
//# `Set<T, N>` MUST require exactly one element type and one explicit capacity.
#[test]
fn requirement_llg_type_03_requires_set_element_type_and_explicit_capacity() {
    let valid = parse_ok("fn main(value: Set<u32, 16>) {}");
    let invalid = parse_err("fn main(value: Set<u32, 16, 32>) {}");

    match &first_function(&valid).params[0].ty.kind {
        TypeKind::Applied { args, .. } => {
            assert!(matches!(
                args.as_slice(),
                [GenericArg::Type(_), GenericArg::Const(_)]
            ));
        }
        other => panic!("expected applied type for Set, got {other:?}"),
    }

    assert_diagnostic_contains(
        &invalid,
        "`Set` requires a value type and an explicit capacity",
    );
}

//= SPEC.md#llg-type-03-bounded-collection-type-arity
//= type=test
//# `Map<K, V, N>` MUST require exactly one key type, one value type, and one explicit capacity.
#[test]
fn requirement_llg_type_03_requires_map_key_value_and_explicit_capacity() {
    let valid = parse_ok("fn main(value: Map<u32, bool, 32>) {}");
    let invalid = parse_err("fn main(value: Map<u32, bool>) {}");

    match &first_function(&valid).params[0].ty.kind {
        TypeKind::Applied { args, .. } => {
            assert!(matches!(
                args.as_slice(),
                [
                    GenericArg::Type(_),
                    GenericArg::Type(_),
                    GenericArg::Const(_)
                ]
            ));
        }
        other => panic!("expected applied type for Map, got {other:?}"),
    }

    assert_diagnostic_contains(
        &invalid,
        "`Map` requires key type, value type, and explicit capacity",
    );
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# The front end MUST preserve byte spans for tokens and syntax nodes.
#[test]
fn requirement_llg_diag_01_preserves_token_and_syntax_node_spans() {
    let lexed = lex("requirement.llg", "fn main() {}");
    let token_texts: Vec<_> = lexed
        .tokens
        .iter()
        .map(|token| lexed.source.span_text(token.span))
        .collect();
    assert_eq!(
        token_texts,
        vec![
            Some("fn"),
            Some("main"),
            Some("("),
            Some(")"),
            Some("{"),
            Some("}"),
            Some(""),
        ]
    );
    let token_tags: Vec<_> = lexed.tokens.iter().map(|token| token.tag()).collect();
    assert_eq!(
        token_tags,
        vec![
            TokenTag::Fn,
            TokenTag::Identifier,
            TokenTag::LParen,
            TokenTag::RParen,
            TokenTag::LBrace,
            TokenTag::RBrace,
            TokenTag::Eof,
        ]
    );

    let source = "fn main() { let value = 1; value }";
    let parsed = parse_ok(source);
    let function = first_function(&parsed);
    let let_stmt = match &function.body.statements[0] {
        Stmt::Let(stmt) => stmt,
        other => panic!("expected let statement, got {other:?}"),
    };
    let trailing_expr = function.body.trailing_expr.as_deref().unwrap();

    assert_eq!(parsed.source.span_text(function.span), Some(source));
    assert_eq!(parsed.source.span_text(function.name.span), Some("main"));
    assert_eq!(
        parsed.source.span_text(function.body.span),
        Some("{ let value = 1; value }")
    );
    assert_eq!(
        parsed.source.span_text(let_stmt.span),
        Some("let value = 1;")
    );
    assert_eq!(parsed.source.span_text(let_stmt.name.span), Some("value"));
    assert_eq!(
        parsed
            .source
            .span_text(let_stmt.value.as_ref().unwrap().span),
        Some("1")
    );
    assert_eq!(parsed.source.span_text(trailing_expr.span), Some("value"));
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Syntax diagnostics MUST include a primary source span.
#[test]
fn requirement_llg_diag_01_syntax_diagnostics_include_a_primary_span() {
    let parsed = parse_err("fn main( {");

    assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic
        .labels
        .iter()
        .any(|label| label.style == LabelStyle::Primary)));
}

//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# Parser recovery MUST preserve following valid top-level items after malformed top-level input.
#[test]
fn requirement_llg_diag_03_preserves_following_top_level_items() {
    let parsed = parse_err(
        r#"
let value = 1;
fn main() {}
"#,
    );

    assert_eq!(parsed.module.items.len(), 1);
    assert_eq!(first_function(&parsed).name.value, "main");
}

//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# Parser recovery MUST preserve following valid statements after a malformed statement.
#[test]
fn requirement_llg_diag_03_preserves_following_valid_statements() {
    let broken_before_keyword = parse_err(
        r#"
fn main() {
    1
    let value = 2;
}
"#,
    );
    let broken_before_statement = parse_err(
        r#"
fn main() {
    let value = ;
    observe value == 1 else {
        return;
    }
}
"#,
    );
    let broken_before_expression = parse_err(
        r#"
fn main() {
    let value = ;
    7;
}
"#,
    );
    let broken_bare_expression = parse_err(
        r#"
fn main() {
    *;
    let value = 1;
}
"#,
    );
    let broken_assignment_value = parse_err(
        r#"
fn main() {
    value = ;
    let next = 1;
}
"#,
    );

    let statements = &first_function(&broken_before_keyword).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Let(_)));

    let statements = &first_function(&broken_before_statement).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Observe(_)));

    let statements = &first_function(&broken_before_expression).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Expr(_)));

    let statements = &first_function(&broken_bare_expression).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Let(_)));

    let statements = &first_function(&broken_assignment_value).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Let(_)));
}

//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# Parser recovery MUST preserve following valid statements after a malformed nested expression.
#[test]
fn requirement_llg_diag_03_preserves_statements_after_malformed_nested_expressions() {
    let broken_call_argument = parse_err(
        r#"
fn main(f: u32) {
    let value = f(* 0, 1);
    return;
}
"#,
    );
    let broken_index = parse_err(
        r#"
fn main(values: [u32; 4]) {
    let value = values[;
    return;
}
"#,
    );
    let broken_tuple_element = parse_err(
        r#"
fn main() {
    let value = (1, ;
    return;
}
"#,
    );
    let broken_array_element = parse_err(
        r#"
fn main() {
    let value = [1, ;
    return;
}
"#,
    );
    let broken_before_statement_keyword = parse_err(
        r#"
fn main(f: u32) {
    let value = f(* 0
    return;
}
"#,
    );
    let broken_before_block_end = parse_err(
        r#"
fn main(f: u32) {
    let value = f(* 0
}
"#,
    );

    let statements = &first_function(&broken_call_argument).body.statements;
    assert_eq!(statements.len(), 2);
    let Stmt::Let(let_stmt) = &statements[0] else {
        panic!("expected recovered let statement, got {:?}", statements[0]);
    };
    assert!(matches!(
        let_stmt.value.as_ref().map(|expr| &expr.kind),
        Some(ExprKind::Call { args, .. }) if args.len() == 1
    ));
    assert!(matches!(statements[1], Stmt::Return(_)));

    for parsed in [&broken_index, &broken_tuple_element, &broken_array_element] {
        let statements = &first_function(parsed).body.statements;
        assert_eq!(statements.len(), 1);
        assert!(matches!(statements[0], Stmt::Return(_)));
    }

    let statements = &first_function(&broken_before_statement_keyword)
        .body
        .statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Return(_)));

    assert_eq!(broken_before_block_end.module.items.len(), 1);
    assert!(first_function(&broken_before_block_end)
        .body
        .statements
        .is_empty());
}

//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# Parser recovery MUST preserve following match arms after a malformed match arm.
#[test]
fn requirement_llg_diag_03_preserves_match_arms_after_malformed_match_arms() {
    let parsed = parse_err(
        r#"
fn main(value: u32) {
    match value {
        0 => * 0,
        1 => 10,
        2 => 20
    }
}
"#,
    );
    let statements = &first_function(&parsed).body.statements;
    let Stmt::Match(match_stmt) = &statements[0] else {
        panic!("expected match statement, got {:?}", statements[0]);
    };

    assert_eq!(match_stmt.arms.len(), 2);
    assert!(matches!(
        match_stmt.arms[0].pattern.kind,
        PatternKind::Int(1)
    ));
    assert!(matches!(
        match_stmt.arms[1].pattern.kind,
        PatternKind::Int(2)
    ));
}

//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# A missing semicolon before `}` MUST not cascade into additional syntax errors for the same statement.
#[test]
fn requirement_llg_diag_03_missing_semicolons_before_rbrace_do_not_cascade() {
    let parsed = parse_err("fn main() { let value = 1 }");

    assert_eq!(parsed.module.items.len(), 1);
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_diagnostic_contains(&parsed, "expected `;` after `let` statement");
}
