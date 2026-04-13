use std::collections::HashMap;

use langlog_syntax::ast::{
    Block, ElseBranch, Expr, ExprKind, Function, Item, MatchBody, Pattern, PatternKind, Stmt,
};
use langlog_syntax::{Diagnostic, Label, ParsedModule, Severity, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Item,
    Param,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    kind: BindingKind,
    span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedName {
    pub use_span: Span,
    pub declaration_span: Span,
    pub kind: BindingKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub parsed: ParsedModule,
    pub diagnostics: Vec<Diagnostic>,
    pub resolutions: Vec<ResolvedName>,
}

impl CheckedProgram {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.severity, Severity::Error))
    }

    pub fn resolution(&self, use_span: Span) -> Option<&ResolvedName> {
        self.resolutions
            .iter()
            .find(|resolution| resolution.use_span == use_span)
    }
}

pub fn analyze(parsed: ParsedModule) -> CheckedProgram {
    let (diagnostics, resolutions) = {
        let mut analyzer = Analyzer::new(&parsed);
        analyzer.collect_items();
        analyzer.analyze_module();
        (analyzer.diagnostics, analyzer.resolutions)
    };

    CheckedProgram {
        parsed,
        diagnostics,
        resolutions,
    }
}

struct Analyzer<'a> {
    parsed: &'a ParsedModule,
    items: HashMap<String, Binding>,
    diagnostics: Vec<Diagnostic>,
    resolutions: Vec<ResolvedName>,
}

impl<'a> Analyzer<'a> {
    fn new(parsed: &'a ParsedModule) -> Self {
        Self {
            parsed,
            items: HashMap::new(),
            diagnostics: Vec::new(),
            resolutions: Vec::new(),
        }
    }

    fn collect_items(&mut self) {
        for item in &self.parsed.module.items {
            let Item::Function(function) = item;
            self.items
                .entry(function.name.value.clone())
                .or_insert(Binding {
                    kind: BindingKind::Item,
                    span: function.name.span,
                });
        }
    }

    fn analyze_module(&mut self) {
        for item in &self.parsed.module.items {
            let Item::Function(function) = item;
            self.analyze_function(function);
        }
    }

    fn analyze_function(&mut self, function: &Function) {
        let mut scopes = ScopeStack::default();
        scopes.push();
        for param in &function.params {
            scopes.insert(
                param.name.value.clone(),
                Binding {
                    kind: BindingKind::Param,
                    span: param.name.span,
                },
            );
        }

        self.analyze_block(&function.body, &mut scopes);
    }

    fn analyze_block(&mut self, block: &Block, scopes: &mut ScopeStack) {
        scopes.push();
        for statement in &block.statements {
            self.analyze_statement(statement, scopes);
        }
        if let Some(expr) = &block.trailing_expr {
            self.analyze_expr(expr, scopes);
        }
        scopes.pop();
    }

    fn analyze_statement(&mut self, statement: &Stmt, scopes: &mut ScopeStack) {
        match statement {
            Stmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.analyze_expr(value, scopes);
                }
                scopes.insert(
                    stmt.name.value.clone(),
                    Binding {
                        kind: BindingKind::Local,
                        span: stmt.name.span,
                    },
                );
            }
            Stmt::Assign(stmt) => {
                self.analyze_expr(&stmt.target, scopes);
                self.analyze_expr(&stmt.value, scopes);
            }
            Stmt::Expr(stmt) => self.analyze_expr(&stmt.expr, scopes),
            Stmt::If(stmt) => {
                self.analyze_expr(&stmt.condition, scopes);
                self.analyze_block(&stmt.then_block, scopes);
                if let Some(else_branch) = &stmt.else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.analyze_block(block, scopes),
                        ElseBranch::If(stmt) => {
                            self.analyze_statement(&Stmt::If(*stmt.clone()), scopes)
                        }
                    }
                }
            }
            Stmt::Match(stmt) => {
                self.analyze_expr(&stmt.expr, scopes);
                for arm in &stmt.arms {
                    scopes.push();
                    self.bind_pattern(&arm.pattern, scopes);
                    match &arm.body {
                        MatchBody::Block(block) => self.analyze_block(block, scopes),
                        MatchBody::Expr(expr) => self.analyze_expr(expr, scopes),
                    }
                    scopes.pop();
                }
            }
            Stmt::For(stmt) => {
                self.analyze_expr(&stmt.iterable, scopes);
                scopes.push();
                self.bind_pattern(&stmt.binding, scopes);
                self.analyze_block(&stmt.body, scopes);
                scopes.pop();
            }
            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.analyze_expr(value, scopes);
                }
            }
            Stmt::Observe(stmt) => self.analyze_expr(&stmt.predicate, scopes),
        }
    }

    fn bind_pattern(&mut self, pattern: &Pattern, scopes: &mut ScopeStack) {
        if let PatternKind::Binding(name) = &pattern.kind {
            scopes.insert(
                name.value.clone(),
                Binding {
                    kind: BindingKind::Local,
                    span: name.span,
                },
            );
        }
    }

    fn analyze_expr(&mut self, expr: &Expr, scopes: &mut ScopeStack) {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Bool(_) => {}
            ExprKind::Name(name) => self.resolve_name(name.value.as_str(), name.span, scopes),
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.analyze_expr(element, scopes);
                }
            }
            ExprKind::Block(block) => self.analyze_block(block, scopes),
            ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
                self.analyze_expr(expr, scopes)
            }
            ExprKind::Binary { left, right, .. } => {
                self.analyze_expr(left, scopes);
                self.analyze_expr(right, scopes);
            }
            ExprKind::Call { callee, args } => {
                self.analyze_expr(callee, scopes);
                for arg in args {
                    self.analyze_expr(arg, scopes);
                }
            }
            ExprKind::Index { target, index } => {
                self.analyze_expr(target, scopes);
                self.analyze_expr(index, scopes);
            }
        }
    }

    fn resolve_name(&mut self, name: &str, use_span: Span, scopes: &ScopeStack) {
        if let Some(binding) = scopes
            .lookup(name)
            .or_else(|| self.items.get(name).copied())
        {
            self.resolutions.push(ResolvedName {
                use_span,
                declaration_span: binding.span,
                kind: binding.kind,
                name: name.to_owned(),
            });
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(format!("undefined binding `{name}`"))
                .with_label(Label::primary(use_span, "not found in this scope")),
        );
    }
}

#[derive(Debug, Default)]
struct ScopeStack {
    scopes: Vec<HashMap<String, Binding>>,
}

impl ScopeStack {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn insert(&mut self, name: String, binding: Binding) {
        self.scopes
            .last_mut()
            .expect("scope stack must not be empty")
            .insert(name, binding);
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}
