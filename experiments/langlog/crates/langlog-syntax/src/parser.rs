use crate::ast::{
    AssignStmt, BinaryOp, Block, DelegateStmt, ElseBranch, ExitStmt, Expr, ExprKind, ExprStmt,
    ForStmt, ForeverStmt, Function, GenericArg, IfStmt, Item, LetStmt, MarkerAnnotation, MarkerArg,
    MarkerArgKind, MarkerImplicationStmt, MarkerRefinement, MarkerRule, MarkerRuleBlock,
    MarkerRuleIfStmt, MarkerRuleParam, MarkerRuleStmt, MatchArm, MatchBody, MatchStmt, Module,
    ObserveOp, ObserveStmt, Param, Pattern, PatternKind, ReturnStmt, Stmt, Task, Type, TypeKind,
    UnaryOp, UnsafeMarkerConstruction, UnsafeMarkerStmt,
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
            let start_cursor = self.cursor;
            match self.parse_item() {
                Some(item) => items.push(item),
                None => self.synchronize_item(),
            }
            if !self.ensure_progress(start_cursor, "parser made no progress while parsing items") {
                break;
            }
        }

        Module { items }
    }

    fn parse_item(&mut self) -> Option<Item> {
        match self.current_tag() {
            TokenTag::Fn => self.parse_function().map(Item::Function),
            TokenTag::Task => self.parse_task().map(Item::Task),
            TokenTag::Identifier if self.at_contextual_keyword("mark") => {
                self.parse_marker_rule().map(Item::MarkerRule)
            }
            _ => {
                self.error_current(
                    "expected a top-level item",
                    "expected `fn`, `task`, or `mark`",
                );
                None
            }
        }
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

    fn parse_task(&mut self) -> Option<Task> {
        let start = self.expect_tag(TokenTag::Task, "expected `task` to start a task")?;
        let name = self.expect_identifier("expected a task name")?;
        self.expect_tag(TokenTag::LParen, "expected `(` after task name")?;
        let params = self.parse_params()?;
        self.expect_tag(TokenTag::Arrow, "expected `->` before task return type")?;
        let return_type = self.parse_type()?;
        let body = self.parse_block()?;
        let span = start.span.cover(body.span).unwrap_or(body.span);

        Some(Task {
            span,
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_marker_rule(&mut self) -> Option<MarkerRule> {
        let start =
            self.expect_contextual_keyword("mark", "expected `mark` to start a marker rule")?;
        let name = self.expect_identifier("expected a marker rule name")?;
        self.expect_tag(TokenTag::LParen, "expected `(` after marker rule name")?;
        let params = self.parse_marker_rule_params()?;
        let body = self.parse_marker_rule_block()?;
        let span = start.span.cover(body.span).unwrap_or(start.span);

        Some(MarkerRule {
            span,
            name,
            params,
            body,
        })
    }

    fn parse_marker_rule_params(&mut self) -> Option<Vec<MarkerRuleParam>> {
        let mut params = Vec::new();

        if self.bump_if(TokenTag::RParen) {
            return Some(params);
        }

        loop {
            let name = self.expect_identifier("expected a marker rule parameter name")?;
            self.expect_tag(
                TokenTag::Colon,
                "expected `:` after marker rule parameter name",
            )?;
            let place = self.expect_contextual_keyword(
                "place",
                "expected `place` as the marker rule parameter type",
            )?;
            let span = name.span.cover(place.span).unwrap_or(name.span);
            params.push(MarkerRuleParam { span, name });

            if self.bump_if(TokenTag::Comma) {
                if self.bump_if(TokenTag::RParen) {
                    break;
                }
            } else {
                self.expect_tag(
                    TokenTag::RParen,
                    "expected `)` after marker rule parameters",
                )?;
                break;
            }
        }

        Some(params)
    }

    fn parse_marker_rule_block(&mut self) -> Option<MarkerRuleBlock> {
        let start = self.expect_tag(TokenTag::LBrace, "expected `{` to start marker rule body")?;
        let mut statements = Vec::new();

        while !self.at(TokenTag::RBrace) && !self.at(TokenTag::Eof) {
            let start_cursor = self.cursor;
            match self.parse_marker_rule_statement() {
                Some(statement) => statements.push(statement),
                None => self.synchronize_marker_rule_statement(),
            }
            if !self.ensure_progress(
                start_cursor,
                "parser made no progress while parsing a marker rule statement",
            ) {
                break;
            }
        }

        let end = self.expect_tag(TokenTag::RBrace, "expected `}` to close marker rule body")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);
        Some(MarkerRuleBlock { span, statements })
    }

    fn parse_marker_rule_statement(&mut self) -> Option<MarkerRuleStmt> {
        match self.current_tag() {
            TokenTag::If => self
                .parse_marker_rule_if_statement()
                .map(MarkerRuleStmt::If),
            TokenTag::Identifier if self.at_contextual_keyword("implies") => self
                .parse_marker_implication_statement()
                .map(MarkerRuleStmt::Implies),
            _ => {
                self.error_current(
                    "expected a marker rule statement",
                    "expected `if` or `implies`",
                );
                None
            }
        }
    }

    fn parse_marker_rule_if_statement(&mut self) -> Option<MarkerRuleIfStmt> {
        let start = self.expect_tag(TokenTag::If, "expected `if`")?;
        let subject = self.expect_identifier("expected a marker refinement place")?;
        self.expect_tag(TokenTag::With, "expected `with` after refinement place")?;
        let marker = self.parse_marker_annotation()?;
        let body = self.parse_marker_rule_block()?;
        let span = start.span.cover(body.span).unwrap_or(start.span);
        let refinement_span = subject.span.cover(marker.span).unwrap_or(subject.span);

        Some(MarkerRuleIfStmt {
            span,
            refinement: MarkerRefinement {
                span: refinement_span,
                subject,
                marker,
            },
            body,
        })
    }

    fn parse_marker_implication_statement(&mut self) -> Option<MarkerImplicationStmt> {
        let start = self.expect_contextual_keyword("implies", "expected `implies`")?;
        let marker = self.parse_marker_annotation()?;
        self.expect_tag(TokenTag::For, "expected `for` after implied marker")?;
        let target = self.expect_identifier("expected target place after `for`")?;
        let end = self.expect_tag(TokenTag::Semi, "expected `;` after marker implication")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(MarkerImplicationStmt {
            span,
            marker,
            target,
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
        let base = self.parse_type_atom()?;
        if !self.bump_if(TokenTag::With) {
            return Some(base);
        }

        let markers = self.parse_marker_annotations()?;
        let end = markers
            .last()
            .map(|marker| marker.span)
            .unwrap_or(base.span);
        let span = base.span.cover(end).unwrap_or(base.span);
        Some(Type::new(
            span,
            TypeKind::With {
                base: Box::new(base),
                markers,
            },
        ))
    }

    fn parse_type_atom(&mut self) -> Option<Type> {
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

    fn parse_marker_annotations(&mut self) -> Option<Vec<MarkerAnnotation>> {
        if self.bump_if(TokenTag::LParen) {
            let mut markers = Vec::new();
            if !self.at(TokenTag::RParen) {
                loop {
                    markers.push(self.parse_marker_annotation()?);
                    if self.bump_if(TokenTag::Comma) {
                        if self.at(TokenTag::RParen) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            self.expect_tag(TokenTag::RParen, "expected `)` after marker list")?;
            return Some(markers);
        }

        Some(vec![self.parse_marker_annotation()?])
    }

    fn parse_marker_annotation(&mut self) -> Option<MarkerAnnotation> {
        let name = self.expect_identifier("expected a marker name")?;
        let mut args = Vec::new();
        let mut end = name.span;

        if self.bump_if(TokenTag::LParen) {
            if !self.at(TokenTag::RParen) {
                loop {
                    args.push(self.parse_marker_arg()?);
                    if self.bump_if(TokenTag::Comma) {
                        if self.at(TokenTag::RParen) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            let close = self.expect_tag(TokenTag::RParen, "expected `)` after marker arguments")?;
            end = close.span;
        }

        let span = name.span.cover(end).unwrap_or(name.span);
        Some(MarkerAnnotation { span, name, args })
    }

    fn parse_marker_arg(&mut self) -> Option<MarkerArg> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Identifier(name) => {
                self.bump();
                let base = Spanned::new(token.span, name);
                if self.bump_if(TokenTag::Dot) {
                    let field = self.expect_identifier("expected a field name after `.`")?;
                    let span = base.span.cover(field.span).unwrap_or(base.span);
                    return Some(MarkerArg {
                        span,
                        kind: MarkerArgKind::Field { base, field },
                    });
                }
                Some(MarkerArg {
                    span: base.span,
                    kind: MarkerArgKind::Name(base),
                })
            }
            TokenKind::Question => {
                self.bump();
                let name = self.expect_identifier("expected a marker-pattern binding name")?;
                let span = token.span.cover(name.span).unwrap_or(token.span);
                Some(MarkerArg {
                    span,
                    kind: MarkerArgKind::PatternBinding(name),
                })
            }
            TokenKind::IntLiteral(value) => {
                self.bump();
                Some(MarkerArg {
                    span: token.span,
                    kind: MarkerArgKind::Int(value),
                })
            }
            TokenKind::True => {
                self.bump();
                Some(MarkerArg {
                    span: token.span,
                    kind: MarkerArgKind::Bool(true),
                })
            }
            TokenKind::False => {
                self.bump();
                Some(MarkerArg {
                    span: token.span,
                    kind: MarkerArgKind::Bool(false),
                })
            }
            _ => {
                self.error_current(
                    "expected a marker argument",
                    "expected a place name, field place, marker-pattern binding, or literal here",
                );
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
            let start_cursor = self.cursor;
            if self.starts_statement() {
                match self.parse_statement() {
                    Some(stmt) => statements.push(stmt),
                    None => self.synchronize_statement(),
                }
                if !self.ensure_progress(
                    start_cursor,
                    "parser made no progress while parsing a statement",
                ) {
                    break;
                }
                continue;
            }

            let expr = match self.parse_expression(0) {
                Some(expr) => expr,
                None => {
                    self.synchronize_statement();
                    if !self.ensure_progress(
                        start_cursor,
                        "parser made no progress while recovering from an expression",
                    ) {
                        break;
                    }
                    continue;
                }
            };

            if self.bump_if(TokenTag::Eq) {
                let value = match self.parse_expression(0) {
                    Some(value) => value,
                    None => {
                        self.synchronize_statement();
                        if !self.ensure_progress(
                            start_cursor,
                            "parser made no progress while recovering from an assignment",
                        ) {
                            break;
                        }
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
            if !self.ensure_progress(
                start_cursor,
                "parser made no progress while synchronizing a block",
            ) {
                break;
            }
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
                | TokenTag::Forever
                | TokenTag::Exit
                | TokenTag::Delegate
                | TokenTag::Observe
                | TokenTag::Unsafe
        )
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        match self.current_tag() {
            TokenTag::Let => self.parse_let_statement().map(Stmt::Let),
            TokenTag::If => self.parse_if_statement().map(Stmt::If),
            TokenTag::Match => self.parse_match_statement().map(Stmt::Match),
            TokenTag::For => self.parse_for_statement().map(Stmt::For),
            TokenTag::Return => self.parse_return_statement().map(Stmt::Return),
            TokenTag::Forever => self.parse_forever_statement().map(Stmt::Forever),
            TokenTag::Exit => self.parse_exit_statement().map(Stmt::Exit),
            TokenTag::Delegate => self.parse_delegate_statement().map(Stmt::Delegate),
            TokenTag::Observe => self.parse_observe_statement().map(Stmt::Observe),
            TokenTag::Unsafe => self.parse_unsafe_marker_statement().map(Stmt::UnsafeMarker),
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

    fn parse_forever_statement(&mut self) -> Option<ForeverStmt> {
        let start = self.expect_tag(TokenTag::Forever, "expected `forever`")?;
        let body = self.parse_block()?;
        let span = start.span.cover(body.span).unwrap_or(start.span);

        Some(ForeverStmt { span, body })
    }

    fn parse_exit_statement(&mut self) -> Option<ExitStmt> {
        let start = self.expect_tag(TokenTag::Exit, "expected `exit`")?;
        let value = self.parse_expression(0)?;
        let end = self.expect_tag(TokenTag::Semi, "expected `;` after `exit`")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);

        Some(ExitStmt { span, value })
    }

    fn parse_delegate_statement(&mut self) -> Option<DelegateStmt> {
        let start = self.expect_tag(TokenTag::Delegate, "expected `delegate`")?;
        let target = self.expect_identifier("expected a task name after `delegate`")?;
        self.expect_tag(TokenTag::LParen, "expected `(` after delegated task name")?;
        let args = self.parse_call_arguments(TokenTag::RParen)?;
        let args_end =
            self.expect_tag(TokenTag::RParen, "expected `)` after delegate arguments")?;
        let end = self.expect_tag(TokenTag::Semi, "expected `;` after `delegate`")?;
        let span = start
            .span
            .cover(end.span)
            .or_else(|| start.span.cover(args_end.span))
            .unwrap_or(start.span);

        Some(DelegateStmt { span, target, args })
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

    fn parse_unsafe_marker_statement(&mut self) -> Option<UnsafeMarkerStmt> {
        let construction = self.parse_unsafe_marker_construction(true)?;
        Some(UnsafeMarkerStmt {
            span: construction.span,
            construction,
        })
    }

    fn parse_unsafe_marker_construction(
        &mut self,
        require_inner_semicolon: bool,
    ) -> Option<UnsafeMarkerConstruction> {
        let start = self.expect_tag(TokenTag::Unsafe, "expected `unsafe`")?;
        self.expect_tag(TokenTag::LBrace, "expected `{` after `unsafe`")?;
        let marker = self.expect_identifier("expected a marker name")?;
        self.expect_tag(TokenTag::PathSep, "expected `::` after marker name")?;
        let method = self.expect_identifier("expected marker constructor name")?;
        if method.value != "mark" {
            self.diagnostics.push(
                Diagnostic::error("unsafe marker construction must call `mark`")
                    .with_label(Label::primary(method.span, "expected `mark` here")),
            );
        }
        self.expect_tag(TokenTag::LParen, "expected `(` after `mark`")?;
        let args = self.parse_call_arguments(TokenTag::RParen)?;
        self.expect_tag(TokenTag::RParen, "expected `)` after marker arguments")?;
        if require_inner_semicolon {
            self.expect_tag(TokenTag::Semi, "expected `;` after unsafe marker statement")?;
        } else if self.at(TokenTag::Semi) {
            self.error_current(
                "unsafe marker expressions must not end with an inner `;`",
                "remove this semicolon or use the statement form",
            );
            self.bump();
        }
        let end = self.expect_tag(TokenTag::RBrace, "expected `}` after unsafe marker")?;
        let span = start.span.cover(end.span).unwrap_or(start.span);
        Some(UnsafeMarkerConstruction { span, marker, args })
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
            if self.at(TokenTag::With) {
                let left_bp = 7;
                if left_bp < min_binding_power {
                    break;
                }

                self.bump();
                let marker = self.parse_marker_annotation()?;
                let span = lhs.span.cover(marker.span).unwrap_or(lhs.span);
                lhs = Expr::new(
                    span,
                    ExprKind::MarkerRefinement {
                        subject: Box::new(lhs),
                        marker,
                    },
                );
                continue;
            }

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
                    let args = self.parse_call_arguments(TokenTag::RParen)?;
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

    fn parse_call_arguments(&mut self, close: TokenTag) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if self.at(close) {
            return Some(args);
        }

        loop {
            match self.parse_expression(0) {
                Some(arg) => args.push(arg),
                None => {
                    self.synchronize_expression_list_item(close);
                    if self.bump_if(TokenTag::Comma) {
                        if self.at(close) {
                            break;
                        }
                        continue;
                    }
                    if self.at(close) {
                        break;
                    }
                    return None;
                }
            }
            if self.bump_if(TokenTag::Comma) {
                if self.at(close) {
                    break;
                }
            } else {
                break;
            }
        }

        Some(args)
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
                if self.at(TokenTag::PathSep) {
                    let path_sep = self.bump();
                    let span = name.span.cover(path_sep.span).unwrap_or(name.span);
                    self.diagnostics.push(
                        Diagnostic::error("marker constructors must be inside `unsafe`")
                            .with_label(Label::primary(
                                span,
                                "wrap this marker construction in `unsafe { ... }`",
                            )),
                    );
                    if self.at(TokenTag::Identifier) {
                        self.bump();
                    }
                    if self.bump_if(TokenTag::LParen) {
                        let _ = self.parse_call_arguments(TokenTag::RParen);
                        let _ = self
                            .expect_tag(TokenTag::RParen, "expected `)` after marker arguments");
                    }
                }
                Some(Expr::new(token.span, ExprKind::Name(name)))
            }
            TokenKind::Unsafe => {
                let construction = self.parse_unsafe_marker_construction(false)?;
                Some(Expr::new(
                    construction.span,
                    ExprKind::UnsafeMarker(construction),
                ))
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
            if self.at(TokenTag::Fn)
                || self.at(TokenTag::Task)
                || self.at_contextual_keyword("mark")
            {
                break;
            }
            self.bump();
        }
    }

    fn synchronize_marker_rule_statement(&mut self) {
        while !self.at(TokenTag::Eof) {
            match self.current_tag() {
                TokenTag::Semi => {
                    self.bump();
                    break;
                }
                TokenTag::RBrace => break,
                TokenTag::If => break,
                TokenTag::Identifier if self.at_contextual_keyword("implies") => break,
                _ => {
                    self.bump();
                }
            }
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
                | TokenTag::Forever
                | TokenTag::Exit
                | TokenTag::Delegate
                | TokenTag::Observe
                | TokenTag::Unsafe => break,
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

    fn ensure_progress(&mut self, start_cursor: usize, message: &str) -> bool {
        if self.cursor != start_cursor {
            return true;
        }

        self.error_current(message, "parser recovery advanced here");
        if self.at(TokenTag::Eof) {
            return false;
        }
        self.cursor += 1;
        true
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

    fn at_contextual_keyword(&self, keyword: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Identifier(name) if name == keyword)
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

    fn expect_contextual_keyword(&mut self, keyword: &str, message: &str) -> Option<Token> {
        if self.at_contextual_keyword(keyword) {
            return Some(self.bump());
        }

        self.error_current(message, &format!("expected `{keyword}` here"));
        None
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
        ExprKind::MarkerRefinement { .. } => false,
        ExprKind::UnsafeMarker(_) => false,
    }
}

#[cfg(test)]
mod tests;
