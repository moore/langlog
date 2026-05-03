use crate::ast::{
    AssignStmt, BinaryOp, Block, ElseBranch, Expr, ExprKind, ExprStmt, ForStmt, Function,
    GenericArg, IfStmt, Item, LetStmt, MatchArm, MatchBody, MatchStmt, Module, ObserveOp,
    ObserveStmt, Param, Pattern, PatternKind, ReturnStmt, Stmt, Type, TypeKind, UnaryOp,
};
use crate::diagnostic::{Diagnostic, Label};
use crate::lexer::LexedSource;
use crate::span::{SourceFile, Span, Spanned};
use crate::token::{Token, TokenKind, TokenTag};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedModule {
    pub source: SourceFile,
    pub tokens: Vec<Token>,
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

impl ParsedModule {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.severity, crate::diagnostic::Severity::Error))
    }
}

pub fn parse_lexed(lexed: LexedSource) -> ParsedModule {
    let LexedSource {
        source,
        tokens,
        diagnostics,
    } = lexed;

    let (module, diagnostics) = {
        let mut parser = Parser::new(&source, &tokens, diagnostics);
        let module = parser.parse_module();
        let diagnostics = parser.finish();
        (module, diagnostics)
    };

    ParsedModule {
        source,
        tokens,
        module,
        diagnostics,
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(_source: &'a SourceFile, tokens: &'a [Token], diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            tokens,
            cursor: 0,
            diagnostics,
        }
    }

    fn finish(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    fn parse_module(&mut self) -> Module {
        let mut items = Vec::new();

        while !self.at(TokenTag::Eof) {
            match self.parse_item() {
                Some(item) => items.push(item),
                None => self.synchronize_item(),
            }
        }

        Module { items }
    }

    fn parse_item(&mut self) -> Option<Item> {
        if !self.at(TokenTag::Fn) {
            self.error_current("expected a top-level item", "expected `fn`");
            return None;
        }

        self.parse_function().map(Item::Function)
    }

    fn parse_function(&mut self) -> Option<Function> {
        let start = self.expect_tag(TokenTag::Fn, "expected `fn` to start a function")?;
        let name = self.expect_identifier("expected a function name")?;
        self.expect_tag(TokenTag::LParen, "expected `(` after function name")?;
        let params = self.parse_params()?;
        let return_type = if self.bump_if(TokenTag::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let span = start.span.cover(body.span).unwrap_or(body.span);

        Some(Function {
            span,
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();

        if self.bump_if(TokenTag::RParen) {
            return Some(params);
        }

        loop {
            let name = self.expect_identifier("expected a parameter name")?;
            self.expect_tag(TokenTag::Colon, "expected `:` after parameter name")?;
            let ty = self.parse_type()?;
            let span = name.span.cover(ty.span).unwrap_or(name.span);
            params.push(Param { span, name, ty });

            if self.bump_if(TokenTag::Comma) {
                if self.bump_if(TokenTag::RParen) {
                    break;
                }
            } else {
                self.expect_tag(TokenTag::RParen, "expected `)` after parameters")?;
                break;
            }
        }

        Some(params)
    }

    fn parse_type(&mut self) -> Option<Type> {
        match self.current_tag() {
            TokenTag::LParen => self.parse_tuple_or_grouped_type(),
            TokenTag::LBracket => self.parse_array_type(),
            TokenTag::Identifier => self.parse_named_or_applied_type(),
            _ => {
                self.error_current("expected a type", "type expected here");
                None
            }
        }
    }

    fn parse_tuple_or_grouped_type(&mut self) -> Option<Type> {
        let start = self.expect_tag(TokenTag::LParen, "expected `(`")?;
        if self.bump_if(TokenTag::RParen) {
            return Some(Type::new(
                start.span.cover(self.previous_span())?,
                TypeKind::Unit,
            ));
        }

        let first = self.parse_type()?;
        let mut elements = vec![first];

        if self.bump_if(TokenTag::Comma) {
            while !self.at(TokenTag::RParen) && !self.at(TokenTag::Eof) {
                elements.push(self.parse_type()?);
                if !self.bump_if(TokenTag::Comma) {
                    break;
                }
            }
            let end = self.expect_tag(TokenTag::RParen, "expected `)` to close tuple type")?;
            let span = start.span.cover(end.span).unwrap_or(start.span);
            Some(Type::new(span, TypeKind::Tuple(elements)))
        } else {
            self.expect_tag(TokenTag::RParen, "expected `)` after type")?;
            elements.into_iter().next()
        }
    }

    fn parse_array_type(&mut self) -> Option<Type> {
        let start = self.expect_tag(TokenTag::LBracket, "expected `[`")?;
        let element = self.parse_type()?;
        self.expect_tag(TokenTag::Semi, "expected `;` in array type")?;
        let length = self.expect_int_literal("expected an array length")?;
        let end = self.expect_tag(TokenTag::RBracket, "expected `]` to close array type")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(Type::new(
            span,
            TypeKind::Array {
                element: Box::new(element),
                length,
            },
        ))
    }

    fn parse_named_or_applied_type(&mut self) -> Option<Type> {
        let base = self.expect_identifier("expected a type name")?;
        if !self.bump_if(TokenTag::Lt) {
            return Some(Type::new(base.span, TypeKind::Named(base)));
        }

        let mut args = Vec::new();
        loop {
            if self.at(TokenTag::IntLiteral) {
                args.push(GenericArg::Const(
                    self.expect_int_literal("expected a const generic")?,
                ));
            } else {
                args.push(GenericArg::Type(self.parse_type()?));
            }

            if self.bump_if(TokenTag::Comma) {
                if self.at(TokenTag::Gt) {
                    break;
                }
            } else {
                break;
            }
        }

        let end = self.expect_tag(TokenTag::Gt, "expected `>` to close generic arguments")?;
        let span = base.span.cover(end.span).unwrap_or(base.span);
        self.validate_builtin_type_application(&base, &args, span);
        Some(Type::new(span, TypeKind::Applied { base, args }))
    }

    fn validate_builtin_type_application(
        &mut self,
        base: &Spanned<String>,
        args: &[GenericArg],
        span: Span,
    ) {
        let valid = match base.value.as_str() {
            "Set" => matches!(args, [GenericArg::Type(_), GenericArg::Const(_)]),
            "Map" => matches!(
                args,
                [
                    GenericArg::Type(_),
                    GenericArg::Type(_),
                    GenericArg::Const(_)
                ]
            ),
            _ => true,
        };

        if valid {
            return;
        }

        let message = match base.value.as_str() {
            "Set" => "`Set` requires a value type and an explicit capacity, as in `Set<T, N>`",
            "Map" => {
                "`Map` requires key type, value type, and explicit capacity, as in `Map<K, V, N>`"
            }
            _ => return,
        };

        self.diagnostics.push(
            Diagnostic::error(message)
                .with_label(Label::primary(span, "invalid built-in type application")),
        );
    }

    fn parse_block(&mut self) -> Option<Block> {
        let start = self.expect_tag(TokenTag::LBrace, "expected `{` to start a block")?;
        let mut statements = Vec::new();
        let mut trailing_expr = None;

        while !self.at(TokenTag::RBrace) && !self.at(TokenTag::Eof) {
            if self.starts_statement() {
                match self.parse_statement() {
                    Some(stmt) => statements.push(stmt),
                    None => self.synchronize_statement(),
                }
                continue;
            }

            let expr = match self.parse_expression(0) {
                Some(expr) => expr,
                None => {
                    self.synchronize_statement();
                    continue;
                }
            };

            if self.bump_if(TokenTag::Eq) {
                let value = match self.parse_expression(0) {
                    Some(value) => value,
                    None => {
                        self.synchronize_statement();
                        continue;
                    }
                };
                let end = self.expect_tag(TokenTag::Semi, "expected `;` after assignment")?;
                let span = expr.span.cover(end.span).unwrap_or(expr.span);
                statements.push(Stmt::Assign(AssignStmt {
                    span,
                    target: expr,
                    value,
                }));
                continue;
            }

            if self.bump_if(TokenTag::Semi) {
                let span = expr.span.cover(self.previous_span()).unwrap_or(expr.span);
                statements.push(Stmt::Expr(ExprStmt { span, expr }));
                continue;
            }

            if self.at(TokenTag::RBrace) {
                trailing_expr = Some(Box::new(expr));
                break;
            }

            self.error_current(
                "expected `;` or `}` after expression",
                "expression ends here",
            );
            self.synchronize_statement();
        }

        let end = self.expect_tag(TokenTag::RBrace, "expected `}` to close block")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(Block {
            span,
            statements,
            trailing_expr,
        })
    }

    fn starts_statement(&self) -> bool {
        matches!(
            self.current_tag(),
            TokenTag::Let
                | TokenTag::If
                | TokenTag::Match
                | TokenTag::For
                | TokenTag::Return
                | TokenTag::Observe
        )
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        match self.current_tag() {
            TokenTag::Let => self.parse_let_statement().map(Stmt::Let),
            TokenTag::If => self.parse_if_statement().map(Stmt::If),
            TokenTag::Match => self.parse_match_statement().map(Stmt::Match),
            TokenTag::For => self.parse_for_statement().map(Stmt::For),
            TokenTag::Return => self.parse_return_statement().map(Stmt::Return),
            TokenTag::Observe => self.parse_observe_statement().map(Stmt::Observe),
            _ => {
                self.error_current("expected a statement", "statement expected here");
                None
            }
        }
    }

    fn parse_let_statement(&mut self) -> Option<LetStmt> {
        let start = self.expect_tag(TokenTag::Let, "expected `let`")?;
        let mutable = self.bump_if(TokenTag::Mut);
        let name = self.expect_identifier("expected a binding name")?;
        let ty = if self.bump_if(TokenTag::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let value = if self.bump_if(TokenTag::Eq) {
            Some(self.parse_expression(0)?)
        } else {
            None
        };
        let end = self.expect_tag(TokenTag::Semi, "expected `;` after `let` statement")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(LetStmt {
            span,
            mutable,
            name,
            ty,
            value,
        })
    }

    fn parse_if_statement(&mut self) -> Option<IfStmt> {
        let start = self.expect_tag(TokenTag::If, "expected `if`")?;
        let condition = self.parse_expression(0)?;
        let then_block = self.parse_block()?;

        let else_branch = if self.bump_if(TokenTag::Else) {
            if self.at(TokenTag::If) {
                Some(ElseBranch::If(Box::new(self.parse_if_statement()?)))
            } else {
                Some(ElseBranch::Block(self.parse_block()?))
            }
        } else {
            None
        };

        let end_span = match else_branch.as_ref() {
            Some(ElseBranch::Block(block)) => block.span,
            Some(ElseBranch::If(stmt)) => stmt.span,
            None => then_block.span,
        };
        let span = start.span.cover(end_span).unwrap_or(start.span);

        Some(IfStmt {
            span,
            condition,
            then_block,
            else_branch,
        })
    }

    fn parse_match_statement(&mut self) -> Option<MatchStmt> {
        let start = self.expect_tag(TokenTag::Match, "expected `match`")?;
        let expr = self.parse_expression(0)?;
        self.expect_tag(TokenTag::LBrace, "expected `{` after match expression")?;
        let mut arms = Vec::new();

        while !self.at(TokenTag::RBrace) && !self.at(TokenTag::Eof) {
            let pattern = match self.parse_pattern() {
                Some(pattern) => pattern,
                None => {
                    self.synchronize_match_arm();
                    if self.bump_if(TokenTag::Comma) {
                        continue;
                    }
                    break;
                }
            };
            if self
                .expect_tag(TokenTag::FatArrow, "expected `=>` after match pattern")
                .is_none()
            {
                self.synchronize_match_arm();
                if self.bump_if(TokenTag::Comma) {
                    continue;
                }
                break;
            }
            let body = if self.at(TokenTag::LBrace) {
                match self.parse_block() {
                    Some(block) => MatchBody::Block(block),
                    None => {
                        self.synchronize_match_arm();
                        if self.bump_if(TokenTag::Comma) {
                            continue;
                        }
                        break;
                    }
                }
            } else {
                match self.parse_expression(0) {
                    Some(expr) => MatchBody::Expr(expr),
                    None => {
                        self.synchronize_match_arm();
                        if self.bump_if(TokenTag::Comma) {
                            continue;
                        }
                        break;
                    }
                }
            };

            let end_span = match &body {
                MatchBody::Block(block) => block.span,
                MatchBody::Expr(expr) => expr.span,
            };
            let span = pattern.span.cover(end_span).unwrap_or(pattern.span);
            arms.push(MatchArm {
                span,
                pattern,
                body,
            });

            if !self.bump_if(TokenTag::Comma) {
                break;
            }
        }

        let end = self.expect_tag(TokenTag::RBrace, "expected `}` after match arms")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(MatchStmt { span, expr, arms })
    }

    fn parse_for_statement(&mut self) -> Option<ForStmt> {
        let start = self.expect_tag(TokenTag::For, "expected `for`")?;
        let binding = self.parse_pattern()?;
        self.expect_tag(TokenTag::In, "expected `in` after loop binding")?;
        let iterable = self.parse_expression(0)?;
        let body = self.parse_block()?;
        let span = start.span.cover(body.span).unwrap_or(start.span);

        Some(ForStmt {
            span,
            binding,
            iterable,
            body,
        })
    }

    fn parse_return_statement(&mut self) -> Option<ReturnStmt> {
        let start = self.expect_tag(TokenTag::Return, "expected `return`")?;
        let value = if self.at(TokenTag::Semi) {
            None
        } else {
            Some(self.parse_expression(0)?)
        };
        let end = self.expect_tag(TokenTag::Semi, "expected `;` after `return`")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(ReturnStmt { span, value })
    }

    fn parse_observe_statement(&mut self) -> Option<ObserveStmt> {
        let start = self.expect_tag(TokenTag::Observe, "expected `observe`")?;
        let left = self.parse_expression(10)?;
        let op = self.parse_observe_operator()?;
        let right = self.parse_expression(10)?;
        self.validate_observe_operand(&left);
        self.validate_observe_operand(&right);
        self.expect_tag(TokenTag::Else, "expected `else` after `observe`")?;
        let else_block = self.parse_block()?;
        let span = start.span.cover(else_block.span).unwrap_or(start.span);

        Some(ObserveStmt {
            span,
            left,
            op,
            right,
            else_block,
        })
    }

    fn parse_observe_operator(&mut self) -> Option<ObserveOp> {
        let op = match self.current_tag() {
            TokenTag::EqEq => ObserveOp::Eq,
            TokenTag::BangEq => ObserveOp::NotEq,
            TokenTag::Lt => ObserveOp::Lt,
            TokenTag::LtEq => ObserveOp::LtEq,
            TokenTag::Gt => ObserveOp::Gt,
            TokenTag::GtEq => ObserveOp::GtEq,
            _ => {
                self.error_current(
                    "expected an `observe` comparison operator",
                    "expected `==`, `!=`, `<`, `<=`, `>`, or `>=` here",
                );
                return None;
            }
        };
        self.bump();
        Some(op)
    }

    fn validate_observe_operand(&mut self, expr: &Expr) {
        if observe_expr_is_phase1_proof_expr(expr) {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error("phase 1 `observe` operands must be proof expressions").with_label(
                Label::primary(
                    expr.span,
                    "tuple, array, block, range, logical, equality, and comparison expressions are not allowed here",
                ),
            ),
        );
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Underscore => {
                self.bump();
                Some(Pattern::new(token.span, PatternKind::Wildcard))
            }
            TokenKind::Identifier(name) => {
                self.bump();
                let name = Spanned::new(token.span, name);
                Some(Pattern::new(token.span, PatternKind::Binding(name)))
            }
            TokenKind::IntLiteral(value) => {
                self.bump();
                Some(Pattern::new(token.span, PatternKind::Int(value)))
            }
            TokenKind::True => {
                self.bump();
                Some(Pattern::new(token.span, PatternKind::Bool(true)))
            }
            TokenKind::False => {
                self.bump();
                Some(Pattern::new(token.span, PatternKind::Bool(false)))
            }
            _ => {
                self.error_current("expected a pattern", "pattern expected here");
                None
            }
        }
    }

    fn parse_expression(&mut self, min_binding_power: u8) -> Option<Expr> {
        let mut lhs = self.parse_prefix()?;
        lhs = self.parse_postfix(lhs)?;

        loop {
            if self.at(TokenTag::Or) {
                let left_bp = 0;
                if left_bp < min_binding_power {
                    break;
                }

                self.bump();
                let error_binding = if self.bump_if(TokenTag::LParen) {
                    let binding = self.expect_identifier("expected error binding after `or(`")?;
                    self.expect_tag(TokenTag::RParen, "expected `)` after error binding")?;
                    Some(binding)
                } else {
                    None
                };
                let fallback = self.parse_expression(left_bp)?;
                let span = lhs.span.cover(fallback.span).unwrap_or(lhs.span);
                lhs = Expr::new(
                    span,
                    ExprKind::Recover {
                        expr: Box::new(lhs),
                        error_binding,
                        fallback: Box::new(fallback),
                    },
                );
                continue;
            }

            let (op, left_bp, right_bp) = match self.current_tag() {
                TokenTag::DotDot => (BinaryOp::Range, 1, 2),
                TokenTag::OrOr => (BinaryOp::Or, 3, 4),
                TokenTag::AndAnd => (BinaryOp::And, 5, 6),
                TokenTag::EqEq => (BinaryOp::EqEq, 7, 8),
                TokenTag::BangEq => (BinaryOp::NotEq, 7, 8),
                TokenTag::Lt => (BinaryOp::Lt, 9, 10),
                TokenTag::LtEq => (BinaryOp::LtEq, 9, 10),
                TokenTag::Gt => (BinaryOp::Gt, 9, 10),
                TokenTag::GtEq => (BinaryOp::GtEq, 9, 10),
                TokenTag::Plus => (BinaryOp::Add, 11, 12),
                TokenTag::Minus => (BinaryOp::Sub, 11, 12),
                TokenTag::Star => (BinaryOp::Mul, 13, 14),
                TokenTag::Slash => (BinaryOp::Div, 13, 14),
                TokenTag::Percent => (BinaryOp::Rem, 13, 14),
                _ => break,
            };

            if left_bp < min_binding_power {
                break;
            }

            self.bump();
            let rhs = self.parse_expression(right_bp)?;
            let span = lhs.span.cover(rhs.span).unwrap_or(lhs.span);
            lhs = Expr::new(
                span,
                ExprKind::Binary {
                    op,
                    left: Box::new(lhs),
                    right: Box::new(rhs),
                },
            );
        }

        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        match self.current_tag() {
            TokenTag::Minus => {
                let start = self.bump();
                let expr = self.parse_expression(15)?;
                let span = start.span.cover(expr.span).unwrap_or(start.span);
                Some(Expr::new(
                    span,
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                ))
            }
            TokenTag::Bang => {
                let start = self.bump();
                let expr = self.parse_expression(15)?;
                let span = start.span.cover(expr.span).unwrap_or(start.span);
                Some(Expr::new(
                    span,
                    ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    },
                ))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_postfix(&mut self, mut expr: Expr) -> Option<Expr> {
        loop {
            match self.current_tag() {
                TokenTag::LParen => {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(TokenTag::RParen) {
                        loop {
                            match self.parse_expression(0) {
                                Some(arg) => args.push(arg),
                                None => {
                                    self.synchronize_expression_list_item(TokenTag::RParen);
                                    if self.bump_if(TokenTag::Comma) {
                                        if self.at(TokenTag::RParen) {
                                            break;
                                        }
                                        continue;
                                    }
                                    if self.at(TokenTag::RParen) {
                                        break;
                                    }
                                    return None;
                                }
                            }
                            if self.bump_if(TokenTag::Comma) {
                                if self.at(TokenTag::RParen) {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    let end = self.expect_tag(TokenTag::RParen, "expected `)` after arguments")?;
                    let span = expr.span.cover(end.span).unwrap_or(expr.span);
                    expr = Expr::new(
                        span,
                        ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                    );
                }
                TokenTag::LBracket => {
                    self.bump();
                    let index = match self.parse_expression(0) {
                        Some(index) => index,
                        None => {
                            self.synchronize_expression_list_item(TokenTag::RBracket);
                            if self.at(TokenTag::RBracket) {
                                self.bump();
                            }
                            return None;
                        }
                    };
                    let end = self.expect_tag(TokenTag::RBracket, "expected `]` after index")?;
                    let span = expr.span.cover(end.span).unwrap_or(expr.span);
                    expr = Expr::new(
                        span,
                        ExprKind::Index {
                            target: Box::new(expr),
                            index: Box::new(index),
                        },
                    );
                }
                _ => break,
            }
        }

        Some(expr)
    }

    fn parse_atom(&mut self) -> Option<Expr> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::IntLiteral(value) => {
                self.bump();
                Some(Expr::new(token.span, ExprKind::Int(value)))
            }
            TokenKind::True => {
                self.bump();
                Some(Expr::new(token.span, ExprKind::Bool(true)))
            }
            TokenKind::False => {
                self.bump();
                Some(Expr::new(token.span, ExprKind::Bool(false)))
            }
            TokenKind::Identifier(name) => {
                self.bump();
                let name = Spanned::new(token.span, name);
                Some(Expr::new(token.span, ExprKind::Name(name)))
            }
            TokenKind::LParen => self.parse_grouped_or_tuple_expression(),
            TokenKind::LBracket => self.parse_array_expression(),
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Some(Expr::new(block.span, ExprKind::Block(block)))
            }
            _ => {
                self.error_current("expected an expression", "expression expected here");
                None
            }
        }
    }

    fn parse_grouped_or_tuple_expression(&mut self) -> Option<Expr> {
        let start = self.expect_tag(TokenTag::LParen, "expected `(`")?;
        if self.bump_if(TokenTag::RParen) {
            return Some(Expr::new(
                start.span.cover(self.previous_span()).unwrap_or(start.span),
                ExprKind::Tuple(Vec::new()),
            ));
        }

        let first = match self.parse_expression(0) {
            Some(first) => first,
            None => {
                self.synchronize_expression_list_item(TokenTag::RParen);
                self.expect_tag(TokenTag::RParen, "expected `)` after expression")?;
                return None;
            }
        };
        if self.bump_if(TokenTag::Comma) {
            let mut elements = vec![first];
            while !self.at(TokenTag::RParen) && !self.at(TokenTag::Eof) {
                match self.parse_expression(0) {
                    Some(element) => elements.push(element),
                    None => {
                        self.synchronize_expression_list_item(TokenTag::RParen);
                        if self.bump_if(TokenTag::Comma) {
                            continue;
                        }
                        break;
                    }
                }
                if !self.bump_if(TokenTag::Comma) {
                    break;
                }
            }
            let end = self.expect_tag(TokenTag::RParen, "expected `)` after tuple")?;
            let span = start.span.cover(end.span).unwrap_or(start.span);
            return Some(Expr::new(span, ExprKind::Tuple(elements)));
        }

        let end = self.expect_tag(TokenTag::RParen, "expected `)` after expression")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);
        Some(Expr::new(span, ExprKind::Grouped(Box::new(first))))
    }

    fn parse_array_expression(&mut self) -> Option<Expr> {
        let start = self.expect_tag(TokenTag::LBracket, "expected `[`")?;
        let mut elements = Vec::new();
        if !self.at(TokenTag::RBracket) {
            loop {
                match self.parse_expression(0) {
                    Some(element) => elements.push(element),
                    None => {
                        self.synchronize_expression_list_item(TokenTag::RBracket);
                        if self.bump_if(TokenTag::Comma) {
                            if self.at(TokenTag::RBracket) {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                if self.bump_if(TokenTag::Comma) {
                    if self.at(TokenTag::RBracket) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        let end = self.expect_tag(TokenTag::RBracket, "expected `]` after array")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);
        Some(Expr::new(span, ExprKind::Array(elements)))
    }

    fn expect_identifier(&mut self, message: &str) -> Option<Spanned<String>> {
        match self.current().kind.clone() {
            TokenKind::Identifier(name) => {
                let span = self.current().span;
                self.bump();
                Some(Spanned::new(span, name))
            }
            _ => {
                self.error_current(message, "identifier expected here");
                None
            }
        }
    }

    fn expect_int_literal(&mut self, message: &str) -> Option<Spanned<u64>> {
        match self.current().kind.clone() {
            TokenKind::IntLiteral(value) => {
                let span = self.current().span;
                self.bump();
                Some(Spanned::new(span, value))
            }
            _ => {
                self.error_current(message, "integer literal expected here");
                None
            }
        }
    }

    fn expect_tag(&mut self, tag: TokenTag, message: &str) -> Option<Token> {
        if self.at(tag) {
            Some(self.bump())
        } else {
            self.error_expected(tag, message);
            None
        }
    }

    fn error_expected(&mut self, tag: TokenTag, message: &str) {
        let current = self.current();
        self.diagnostics
            .push(Diagnostic::error(message).with_label(Label::primary(
                current.span,
                format!("expected {}", tag.describe()),
            )));
    }

    fn error_current(&mut self, message: &str, label: &str) {
        let current = self.current();
        self.diagnostics
            .push(Diagnostic::error(message).with_label(Label::primary(current.span, label)));
    }

    fn synchronize_item(&mut self) {
        while !self.at(TokenTag::Eof) {
            if self.at(TokenTag::Fn) {
                break;
            }
            self.bump();
        }
    }

    fn synchronize_statement(&mut self) {
        while !self.at(TokenTag::Eof) {
            match self.current_tag() {
                TokenTag::Semi => {
                    self.bump();
                    break;
                }
                TokenTag::RBrace => break,
                TokenTag::Let
                | TokenTag::If
                | TokenTag::Match
                | TokenTag::For
                | TokenTag::Return
                | TokenTag::Observe => break,
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn synchronize_expression_list_item(&mut self, close: TokenTag) {
        while !self.at(TokenTag::Eof) {
            if self.at(close)
                || self.at(TokenTag::Comma)
                || self.at(TokenTag::Semi)
                || self.at(TokenTag::RBrace)
                || self.starts_statement()
            {
                break;
            }
            self.bump();
        }
    }

    fn synchronize_match_arm(&mut self) {
        while !self.at(TokenTag::Eof) && !self.at(TokenTag::Comma) && !self.at(TokenTag::RBrace) {
            self.bump();
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn current_tag(&self) -> TokenTag {
        self.current().tag()
    }

    fn previous_span(&self) -> Span {
        self.tokens[self.cursor.saturating_sub(1)].span
    }

    fn at(&self, tag: TokenTag) -> bool {
        self.current_tag() == tag
    }

    fn bump_if(&mut self, tag: TokenTag) -> bool {
        if self.at(tag) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.cursor].clone();
        if !self.at(TokenTag::Eof) {
            self.cursor += 1;
        }
        token
    }
}

fn observe_expr_is_phase1_proof_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Name(_) => true,
        ExprKind::Tuple(_) | ExprKind::Array(_) | ExprKind::Block(_) => false,
        ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
            observe_expr_is_phase1_proof_expr(expr)
        }
        ExprKind::Binary { op, left, right } => {
            matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
            ) && observe_expr_is_phase1_proof_expr(left)
                && observe_expr_is_phase1_proof_expr(right)
        }
        ExprKind::Recover { .. } => false,
        ExprKind::Call { callee, args } => {
            observe_expr_is_phase1_proof_expr(callee)
                && args.iter().all(observe_expr_is_phase1_proof_expr)
        }
        ExprKind::Index { target, index } => {
            observe_expr_is_phase1_proof_expr(target) && observe_expr_is_phase1_proof_expr(index)
        }
    }
}

#[cfg(test)]
mod tests {
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
}
