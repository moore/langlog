use crate::ast::{
    BinaryOp, Expr, ExprKind, Function, GenericArg, Item, PatternKind, Stmt, TypeKind, UnaryOp,
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

//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST ignore line comments beginning with `//`.
//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST ignore block comments delimited by `/*` and `*/`.
//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST support nested block comments.
//= SPEC.md#llg-lex-01-comments
//= type=test
//# The lexer MUST report an error for an unterminated block comment.
#[test]
fn requirement_llg_lex_01_handles_comments() {
    let line = lex("requirement.llg", "// comment\nfn main() {}");
    let block = lex("requirement.llg", "/* comment */ fn main() {}");
    let nested = lex(
        "requirement.llg",
        "/* outer /* inner */ still outer */ fn main() {}",
    );
    let unterminated = lex("requirement.llg", "/* unterminated");

    assert!(line.diagnostics.is_empty());
    assert_eq!(line.tokens[0].tag(), TokenTag::Fn);

    assert!(block.diagnostics.is_empty());
    assert_eq!(block.tokens[0].tag(), TokenTag::Fn);

    assert!(nested.diagnostics.is_empty());
    assert_eq!(nested.tokens[0].tag(), TokenTag::Fn);

    assert_eq!(unterminated.diagnostics.len(), 1);
    assert!(unterminated.diagnostics[0]
        .message
        .contains("unterminated block comment"));
}

//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII letters, digits, or `_`.
//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Integer literals MUST be parsed as unsigned base-10 integers.
//= SPEC.md#llg-lex-02-identifiers-and-literals
//= type=test
//# Boolean literals MUST include `true` and `false`.
#[test]
fn requirement_llg_lex_02_recognizes_identifiers_and_literals() {
    let identifier = lex("requirement.llg", "_name9");
    let integer = lex("requirement.llg", "12345");
    let booleans = lex("requirement.llg", "true false");

    assert!(matches!(
        &identifier.tokens[0].kind,
        TokenKind::Identifier(name) if name == "_name9"
    ));
    assert!(matches!(integer.tokens[0].kind, TokenKind::IntLiteral(12345)));

    let tags: Vec<_> = booleans.tokens.iter().map(|token| token.tag()).collect();
    assert_eq!(tags, vec![TokenTag::True, TokenTag::False, TokenTag::Eof]);
}

//= SPEC.md#llg-lex-03-reserved-keywords
//= type=test
//# The phase 1 keyword set MUST reserve `fn`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`, `return`, `observe`, `true`, and `false`.
#[test]
fn requirement_llg_lex_03_reserves_keywords() {
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

//= SPEC.md#llg-syn-01-top-level-functions
//= type=test
//# A phase 1 source file MUST contain only function items at the top level.
//= SPEC.md#llg-syn-01-top-level-functions
//= type=test
//# A function item MUST use Rust-like syntax with `fn`, a name, a parameter list, and a block body.
//= SPEC.md#llg-syn-01-top-level-functions
//= type=test
//# The current parser allows the return type to be omitted in phase 1.
#[test]
fn requirement_llg_syn_01_parses_top_level_functions() {
    let non_function = parse_err("let value = 1;");
    let parsed = parse_ok(
        r#"
fn main(value: u32) -> u32 { value }
fn helper() {}
"#,
    );
    let function = first_function(&parsed);

    assert!(non_function.module.items.is_empty());
    assert_eq!(function.name.value, "main");
    assert_eq!(function.params.len(), 1);
    assert!(function.return_type.is_some());

    let Item::Function(helper) = &parsed.module.items[1];
    assert!(helper.return_type.is_none());
}

//= SPEC.md#llg-syn-02-statements
//= type=test
//# The parser MUST accept `let`, assignment, expression, `if`, `match`, `for`, `return`, and `observe` statements.
//= SPEC.md#llg-syn-02-statements
//= type=test
//# The current parser allows a `let` statement to include `mut`, a type annotation, and an initializer.
//= SPEC.md#llg-syn-02-statements
//= type=test
//# A statement form that requires a semicolon MUST reject the form if the semicolon is absent.
#[test]
fn requirement_llg_syn_02_parses_statement_forms_and_requires_semicolons() {
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
    let missing_let_semi = parse_err("fn main() { let value = 1 }");
    let missing_expr_semi = parse_err(
        r#"
fn main() {
    1
    observe true;
}
"#,
    );

    let statements = &first_function(&parsed).body.statements;
    let Stmt::Let(let_stmt) = &statements[0] else {
        panic!("expected a let statement, got {:?}", statements[0]);
    };

    assert!(let_stmt.mutable);
    assert!(let_stmt.ty.is_some());
    assert!(let_stmt.value.is_some());
    assert!(matches!(statements[1], Stmt::Assign(_)));
    assert!(matches!(statements[2], Stmt::Expr(_)));
    assert!(matches!(statements[3], Stmt::If(_)));
    assert!(matches!(statements[4], Stmt::Match(_)));
    assert!(matches!(statements[5], Stmt::For(_)));
    assert!(matches!(statements[6], Stmt::Observe(_)));
    assert!(matches!(statements[7], Stmt::Return(_)));

    assert!(missing_let_semi.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("expected `;` after `let` statement")));
    assert!(missing_expr_semi.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("expected `;` or `}` after expression")));
}

//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# The parser MUST accept integer literals, boolean literals, names, tuples, arrays, blocks, grouped expressions, unary operators, binary operators, calls, and indexing expressions.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# The supported binary operators MUST include `..`, `||`, `&&`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `+`, `-`, `*`, `/`, and `%`.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Binary operators with the same precedence MUST associate to the left.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Postfix call and indexing MUST bind tighter than unary operators.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Unary operators MUST bind tighter than multiplicative operators.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Multiplicative operators MUST bind tighter than additive operators.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Additive operators MUST bind tighter than comparison operators.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Comparison operators MUST bind tighter than equality operators.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Equality operators MUST bind tighter than logical and.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Logical and MUST bind tighter than logical or.
//= SPEC.md#llg-syn-03-expressions-and-precedence
//= type=test
//# Logical or MUST bind tighter than range construction.
#[test]
fn requirement_llg_syn_03_parses_expression_forms_and_operator_binding() {
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
    1 != 2;
    3 <= 4;
    5 >= 6;
    8 - 4 - 2;
    20 / 5 % 2;
}
"#,
    );
    let postfix = parse_trailing_expr("-f(1)[0]");
    let unary_mul = parse_trailing_expr("-a * b");
    let add_cmp = parse_trailing_expr("1 + 2 < 4");
    let cmp_eq = parse_trailing_expr("1 < 2 == 3 < 4");
    let eq_and = parse_trailing_expr("1 == 2 && 3 == 4");
    let and_or = parse_trailing_expr("a && b || c && d");
    let or_range = parse_trailing_expr("a || b .. c || d");

    let statements = &first_function(&parsed).body.statements;
    assert!(matches!(
        expr_stmt(&statements[10]).kind,
        ExprKind::Binary {
            op: BinaryOp::Add,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[13]).kind,
        ExprKind::Binary {
            op: BinaryOp::NotEq,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[14]).kind,
        ExprKind::Binary {
            op: BinaryOp::LtEq,
            ..
        }
    ));
    assert!(matches!(
        expr_stmt(&statements[15]).kind,
        ExprKind::Binary {
            op: BinaryOp::GtEq,
            ..
        }
    ));

    match &expr_stmt(&statements[16]).kind {
        ExprKind::Binary {
            op: BinaryOp::Sub,
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
        other => panic!("expected left-associated subtraction, got {other:?}"),
    }

    match &expr_stmt(&statements[17]).kind {
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

    match postfix.kind {
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

    match unary_mul.kind {
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

    match add_cmp.kind {
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

    match cmp_eq.kind {
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

    match eq_and.kind {
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

    match and_or.kind {
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

    match or_range.kind {
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

//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `()` MUST parse as an empty tuple expression.
//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `(expr)` MUST parse as a grouped expression.
//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `(expr,)` MUST parse as a single-element tuple expression.
//= SPEC.md#llg-syn-04-grouped-and-tuple-expressions
//= type=test
//# `(a, b, ...)` MUST parse as a tuple expression.
#[test]
fn requirement_llg_syn_04_distinguishes_grouped_and_tuple_expressions() {
    let parsed = parse_ok(
        r#"
fn main() {
    ();
    (1);
    (1,);
    (1, 2, 3);
}
"#,
    );
    let statements = &first_function(&parsed).body.statements;

    assert!(matches!(
        &expr_stmt(&statements[0]).kind,
        ExprKind::Tuple(elements) if elements.is_empty()
    ));
    assert!(matches!(
        expr_stmt(&statements[1]).kind,
        ExprKind::Grouped(_)
    ));
    assert!(matches!(
        &expr_stmt(&statements[2]).kind,
        ExprKind::Tuple(elements) if elements.len() == 1
    ));
    assert!(matches!(
        &expr_stmt(&statements[3]).kind,
        ExprKind::Tuple(elements) if elements.len() == 3
    ));
}

//= SPEC.md#llg-syn-05-patterns-and-match-arms
//= type=test
//# The parser MUST accept wildcard, binding, integer literal, and boolean patterns.
//= SPEC.md#llg-syn-05-patterns-and-match-arms
//= type=test
//# `match` arms MUST use `pattern => body`.
//= SPEC.md#llg-syn-05-patterns-and-match-arms
//= type=test
//# `match` arms MUST be comma-separated and MAY end with a trailing comma.
#[test]
fn requirement_llg_syn_05_parses_patterns_and_match_arms() {
    let parsed = parse_ok(
        r#"
fn main() {
    match 1 {
        _ => 0,
        value => value,
        7 => 1,
        false => 2,
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
    assert!(matches!(match_stmt.arms[2].pattern.kind, PatternKind::Int(7)));
    assert!(matches!(
        match_stmt.arms[3].pattern.kind,
        PatternKind::Bool(false)
    ));
}

//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# The parser MUST accept unit, named, tuple, fixed-array, and generic application type forms.
//= SPEC.md#llg-type-01-phase-1-types
//= type=test
//# A fixed-array type MUST use the form `[T; N]`.
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
    assert!(matches!(function.params[4].ty.kind, TypeKind::Applied { .. }));
    assert!(matches!(function.params[5].ty.kind, TypeKind::Applied { .. }));
}

//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `()` MUST parse as the unit type.
//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `(T)` MUST parse as a grouped type and MUST NOT create a tuple type.
//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `(T,)` MUST parse as a single-element tuple type.
//= SPEC.md#llg-type-02-grouped-and-tuple-types
//= type=test
//# `(A, B, ...)` MUST parse as a tuple type.
#[test]
fn requirement_llg_type_02_distinguishes_grouped_and_tuple_types() {
    let unit = parse_ok("fn main(value: ()) {}");
    let grouped = parse_ok("fn main(value: (u32)) {}");
    let singleton = parse_ok("fn main(value: (u32,)) {}");
    let tuple = parse_ok("fn main(value: (u32, bool, u8)) {}");

    assert!(matches!(
        &first_function(&unit).params[0].ty.kind,
        TypeKind::Unit
    ));
    assert!(matches!(
        &first_function(&grouped).params[0].ty.kind,
        TypeKind::Named(name) if name.value == "u32"
    ));
    assert!(matches!(
        &first_function(&singleton).params[0].ty.kind,
        TypeKind::Tuple(elements) if elements.len() == 1
    ));
    assert!(matches!(
        &first_function(&tuple).params[0].ty.kind,
        TypeKind::Tuple(elements) if elements.len() == 3
    ));
}

//= SPEC.md#llg-type-03-bounded-collection-type-arity
//= type=test
//# `Set<T, N>` MUST require exactly one element type and one explicit capacity.
//= SPEC.md#llg-type-03-bounded-collection-type-arity
//= type=test
//# `Map<K, V, N>` MUST require exactly one key type, one value type, and one explicit capacity.
#[test]
fn requirement_llg_type_03_validates_bounded_collection_type_arity() {
    let valid = parse_ok("fn main(a: Set<u32, 16>, b: Map<u32, bool, 32>) {}");
    let invalid_set = parse_err("fn main(value: Set<u32, 16, 32>) {}");
    let invalid_map = parse_err("fn main(value: Map<u32, bool>) {}");
    let params = &first_function(&valid).params;

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

    assert!(invalid_set.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("`Set` requires a value type and an explicit capacity")));
    assert!(invalid_map.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("`Map` requires key type, value type, and explicit capacity")));
}

//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# The front end MUST preserve byte spans for tokens and syntax nodes.
//= SPEC.md#llg-diag-01-source-span-preservation
//= type=test
//# Syntax diagnostics MUST include a primary source span.
#[test]
fn requirement_llg_diag_01_preserves_source_spans_and_primary_labels() {
    let lexed = lex("requirement.llg", "fn main() {}");
    for token in &lexed.tokens {
        let text = lexed.source.span_text(token.span);
        if token.tag() == TokenTag::Eof {
            assert_eq!(text, Some(""));
        } else {
            assert!(text.is_some_and(|slice| !slice.is_empty()));
        }
    }

    let source = "fn main() { let value = 1; value }";
    let parsed = parse_ok(source);
    let function = first_function(&parsed);
    let trailing_expr = function.body.trailing_expr.as_deref().unwrap();

    assert_eq!(parsed.source.span_text(function.span), Some(source));
    assert_eq!(parsed.source.span_text(function.name.span), Some("main"));
    assert_eq!(parsed.source.span_text(trailing_expr.span), Some("value"));

    let broken = parse_err("fn main( {");
    assert!(broken.diagnostics.iter().any(|diagnostic| diagnostic
        .labels
        .iter()
        .any(|label| label.style == LabelStyle::Primary)));
}

//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# Parser recovery MUST preserve following valid top-level items after malformed top-level input.
//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# Parser recovery MUST preserve following valid statements after a malformed statement.
//= SPEC.md#llg-diag-03-parser-recovery
//= type=test
//# A missing semicolon before `}` MUST not cascade into additional syntax errors for the same statement.
#[test]
fn requirement_llg_diag_03_recovers_after_invalid_input_without_cascading() {
    let malformed_item = parse_err(
        r#"
let value = 1;
fn main() {}
"#,
    );
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
    observe true;
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
    let missing_let_semi = parse_err("fn main() { let value = 1 }");

    assert_eq!(malformed_item.module.items.len(), 1);
    assert_eq!(first_function(&malformed_item).name.value, "main");

    let statements = &first_function(&broken_before_keyword).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Let(_)));

    let statements = &first_function(&broken_before_statement).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Observe(_)));

    let statements = &first_function(&broken_before_expression).body.statements;
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0], Stmt::Expr(_)));

    assert_eq!(missing_let_semi.module.items.len(), 1);
    assert_eq!(missing_let_semi.diagnostics.len(), 1);
    assert!(missing_let_semi.diagnostics[0]
        .message
        .contains("expected `;` after `let` statement"));
}
