use crate::ast::{BinaryOp, Expr, ExprKind, Function, GenericArg, Item, TypeKind, UnaryOp};
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

//= SPEC.md#llg-lex-01-comments-and-token-spans
//= type=test
//# The lexer MUST ignore line comments beginning with `//`.
#[test]
fn requirement_llg_lex_01_line_comments_are_ignored() {
    let lexed = lex("requirement.llg", "// comment\nfn main() {}");

    assert!(lexed.diagnostics.is_empty());
    assert_eq!(lexed.tokens[0].tag(), TokenTag::Fn);
}

//= SPEC.md#llg-lex-01-comments-and-token-spans
//= type=test
//# The lexer MUST ignore block comments delimited by `/*` and `*/`.
#[test]
fn requirement_llg_lex_01_block_comments_are_ignored() {
    let lexed = lex("requirement.llg", "/* comment */ fn main() {}");

    assert!(lexed.diagnostics.is_empty());
    assert_eq!(lexed.tokens[0].tag(), TokenTag::Fn);
}

//= SPEC.md#llg-lex-01-comments-and-token-spans
//= type=test
//# The lexer MUST support nested block comments.
#[test]
fn requirement_llg_lex_01_nested_block_comments_are_supported() {
    let lexed = lex(
        "requirement.llg",
        "/* outer /* inner */ still outer */ fn main() {}",
    );

    assert!(lexed.diagnostics.is_empty());
    assert_eq!(lexed.tokens[0].tag(), TokenTag::Fn);
}

//= SPEC.md#llg-lex-01-comments-and-token-spans
//= type=test
//# The lexer MUST report an error for an unterminated block comment.
#[test]
fn requirement_llg_lex_01_unterminated_block_comment_reports_error() {
    let lexed = lex("requirement.llg", "/* unterminated");

    assert_eq!(lexed.diagnostics.len(), 1);
    assert!(lexed.diagnostics[0]
        .message
        .contains("unterminated block comment"));
}

//= SPEC.md#llg-lex-01-comments-and-token-spans
//= type=test
//# The lexer MUST attach a byte span to every emitted token.
#[test]
fn requirement_llg_lex_01_every_token_has_a_span() {
    let lexed = lex("requirement.llg", "fn main() {}");

    for token in &lexed.tokens {
        let text = lexed.source.span_text(token.span);
        if token.tag() == TokenTag::Eof {
            assert_eq!(text, Some(""));
        } else {
            assert!(text.is_some_and(|slice| !slice.is_empty()));
        }
    }
}

//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII letters, digits, or `_`.
#[test]
fn requirement_llg_lex_02_identifier_shape_is_accepted() {
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
fn requirement_llg_lex_02_integer_literals_are_base_10_u64_values() {
    let lexed = lex("requirement.llg", "12345");

    assert!(matches!(lexed.tokens[0].kind, TokenKind::IntLiteral(12345)));
}

//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Boolean literals MUST include `true` and `false`.
#[test]
fn requirement_llg_lex_02_boolean_literals_are_tokens() {
    let lexed = lex("requirement.llg", "true false");
    let tags: Vec<_> = lexed.tokens.iter().map(|token| token.tag()).collect();

    assert_eq!(tags, vec![TokenTag::True, TokenTag::False, TokenTag::Eof]);
}

//= SPEC.md#llg-lex-03-reserved-keywords
//= type=test
//# The phase 1 keyword set MUST reserve `fn`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`, `return`, `observe`, `true`, and `false`.
#[test]
fn requirement_llg_lex_03_keywords_are_reserved() {
    let lexed = lex(
        "requirement.llg",
        "fn let mut if else match for in return observe true false",
    );
    let tags: Vec<_> = lexed.tokens.iter().map(|token| token.tag()).collect();

    assert_eq!(
        tags,
        vec![
            TokenTag::Fn,
            TokenTag::Let,
            TokenTag::Mut,
            TokenTag::If,
            TokenTag::Else,
            TokenTag::Match,
            TokenTag::For,
            TokenTag::In,
            TokenTag::Return,
            TokenTag::Observe,
            TokenTag::True,
            TokenTag::False,
            TokenTag::Eof,
        ]
    );
}

//= SPEC.md#llg-syn-01-top-level-functions
//= type=test
//# A phase 1 source file MUST contain only function items at the top level.
#[test]
fn requirement_llg_syn_01_only_functions_are_allowed_at_top_level() {
    let parsed = parse_err("let value = 1;");

    assert!(parsed.module.items.is_empty());
}

//= SPEC.md#llg-syn-01-top-level-functions
//= type=test
//# A function item MUST use Rust-like syntax with `fn`, a name, a parameter list, and a block body.
#[test]
fn requirement_llg_syn_01_function_item_syntax_is_parsed() {
    let parsed = parse_ok("fn main(value: u32) -> u32 { value }");
    let function = first_function(&parsed);

    assert_eq!(function.name.value, "main");
    assert_eq!(function.params.len(), 1);
    assert!(function.return_type.is_some());
    assert!(function.body.trailing_expr.is_some());
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# The parser MUST accept `let`, assignment, expression, `if`, `match`, `for`, `return`, and `observe` statements.
#[test]
fn requirement_llg_syn_02_statement_forms_are_parsed() {
    let parsed = parse_ok(
        r#"
fn main() {
    let mut total: u32 = 0;
    total = total + 1;
    total;
    if true {
        observe true;
    } else {
        observe false;
    }
    match true {
        true => { total = 1; },
        false => { total = 2; }
    }
    for value in [1, 2] {
        total = total + value;
    }
    observe total > 0;
    return;
}
"#,
    );

    assert!(!parsed.has_errors());
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# A statement form that requires a semicolon MUST reject the form if the semicolon is absent.
#[test]
fn requirement_llg_syn_02_missing_semicolon_is_rejected() {
    let parsed = parse_err("fn main() { let value = 1 }");

    assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("expected `;` after `let` statement")
        || diagnostic
            .message
            .contains("expected `;` or `}` after expression")));
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# The parser MUST accept integer literals, boolean literals, names, tuples, arrays, blocks, grouped expressions, unary operators, binary operators, calls, and indexing expressions.
#[test]
fn requirement_llg_syn_03_expression_forms_are_parsed() {
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

    assert!(!parsed.has_errors());
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

//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# The parser MUST accept unit, named, tuple, fixed-array, and generic application type forms.
#[test]
fn requirement_llg_type_01_type_forms_are_parsed() {
    let parsed = parse_ok(
        "fn main(a: (), b: u32, c: (u32, bool), d: [u32; 4], e: Option<u32>, f: Result<u32, Error>) -> () { () }",
    );

    assert_eq!(first_function(&parsed).params.len(), 6);
}

//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# A fixed-array type MUST use the form `[T; N]`.
#[test]
fn requirement_llg_type_01_fixed_array_type_uses_semicolon_syntax() {
    let parsed = parse_ok("fn main(values: [u32; 4]) {}");

    match &first_function(&parsed).params[0].ty.kind {
        TypeKind::Array { .. } => {}
        other => panic!("expected array type, got {other:?}"),
    }
}

//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# `Set<T, N>` and `Map<K, V, N>` MUST carry explicit capacity arguments in the source type.
#[test]
fn requirement_llg_type_01_set_and_map_require_explicit_capacity() {
    let parsed = parse_err("fn main(a: Set<u32>, b: Map<u32, bool>) {}");

    assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("requires a value type and an explicit capacity")
        || diagnostic
            .message
            .contains("requires key type, value type, and explicit capacity")));
}

//= SPEC.md#llg-diag-01-source-spans-and-syntax-diagnostics
//= type=test
//# The front end MUST preserve byte spans for tokens and syntax nodes or derive them from spanned children without reparsing source text.
#[test]
fn requirement_llg_diag_01_syntax_nodes_preserve_source_spans() {
    let source = "fn main() { let value = 1; value }";
    let parsed = parse_ok(source);
    let function = first_function(&parsed);
    let trailing_expr = function.body.trailing_expr.as_deref().unwrap();

    assert_eq!(parsed.source.span_text(function.span), Some(source));
    assert_eq!(parsed.source.span_text(function.name.span), Some("main"));
    assert_eq!(parsed.source.span_text(trailing_expr.span), Some("value"));
}

//= SPEC.md#llg-diag-01-source-spans-and-syntax-diagnostics
//= type=test
//# Syntax diagnostics MUST include a primary source span.
#[test]
fn requirement_llg_diag_01_syntax_errors_have_primary_labels() {
    let parsed = parse_err("fn main( {");

    assert!(parsed.diagnostics.iter().any(|diagnostic| diagnostic
        .labels
        .iter()
        .any(|label| label.style == LabelStyle::Primary)));
}

//= SPEC.md#llg-rel-01-collections-and-relations
//= type=test
//# The language MUST parse capacity-bounded `Set<T, N>` and `Map<K, V, N>` types.
#[test]
fn requirement_llg_rel_01_capacity_bounded_collection_types_parse() {
    let parsed = parse_ok("fn main(a: Set<u32, 16>, b: Map<u32, bool, 32>) {}");

    let params = &first_function(&parsed).params;
    match &params[0].ty.kind {
        TypeKind::Applied { args, .. } => {
            assert!(matches!(
                args.as_slice(),
                [GenericArg::Type(_), GenericArg::Const(_)]
            ));
        }
        other => panic!("expected applied type for Set, got {other:?}"),
    }
    match &params[1].ty.kind {
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
}
