use std::collections::{HashMap, HashSet};

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
struct CallEdge {
    caller_name: String,
    callee_name: String,
    callee_span: Span,
    call_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Visited,
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
    call_edges: Vec<CallEdge>,
}

impl<'a> Analyzer<'a> {
    fn new(parsed: &'a ParsedModule) -> Self {
        Self {
            parsed,
            items: HashMap::new(),
            diagnostics: Vec::new(),
            resolutions: Vec::new(),
            call_edges: Vec::new(),
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

        self.detect_indirect_recursion();
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

        self.analyze_block(&function.body, &mut scopes, function);
    }

    fn analyze_block(&mut self, block: &Block, scopes: &mut ScopeStack, function: &Function) {
        scopes.push();
        for statement in &block.statements {
            self.analyze_statement(statement, scopes, function);
        }
        if let Some(expr) = &block.trailing_expr {
            self.analyze_expr(expr, scopes, function);
        }
        scopes.pop();
    }

    fn analyze_statement(
        &mut self,
        statement: &Stmt,
        scopes: &mut ScopeStack,
        function: &Function,
    ) {
        match statement {
            Stmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.analyze_expr(value, scopes, function);
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
                self.analyze_expr(&stmt.target, scopes, function);
                self.analyze_expr(&stmt.value, scopes, function);
            }
            Stmt::Expr(stmt) => {
                self.analyze_expr(&stmt.expr, scopes, function);
            }
            Stmt::If(stmt) => {
                self.analyze_expr(&stmt.condition, scopes, function);
                self.analyze_block(&stmt.then_block, scopes, function);
                if let Some(else_branch) = &stmt.else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.analyze_block(block, scopes, function),
                        ElseBranch::If(stmt) => {
                            self.analyze_statement(&Stmt::If(*stmt.clone()), scopes, function)
                        }
                    }
                }
            }
            Stmt::Match(stmt) => {
                self.analyze_expr(&stmt.expr, scopes, function);
                for arm in &stmt.arms {
                    scopes.push();
                    self.bind_pattern(&arm.pattern, scopes);
                    match &arm.body {
                        MatchBody::Block(block) => self.analyze_block(block, scopes, function),
                        MatchBody::Expr(expr) => {
                            self.analyze_expr(expr, scopes, function);
                        }
                    }
                    scopes.pop();
                }
            }
            Stmt::For(stmt) => {
                self.analyze_expr(&stmt.iterable, scopes, function);
                scopes.push();
                self.bind_pattern(&stmt.binding, scopes);
                self.analyze_block(&stmt.body, scopes, function);
                scopes.pop();
            }
            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.analyze_expr(value, scopes, function);
                }
            }
            Stmt::Observe(stmt) => {
                self.analyze_expr(&stmt.predicate, scopes, function);
            }
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

    fn analyze_expr(
        &mut self,
        expr: &Expr,
        scopes: &mut ScopeStack,
        function: &Function,
    ) -> Option<Binding> {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Bool(_) => None,
            ExprKind::Name(name) => self.resolve_name(name.value.as_str(), name.span, scopes),
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.analyze_expr(element, scopes, function);
                }
                None
            }
            ExprKind::Block(block) => {
                self.analyze_block(block, scopes, function);
                None
            }
            ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
                self.analyze_expr(expr, scopes, function)
            }
            ExprKind::Binary { left, right, .. } => {
                self.analyze_expr(left, scopes, function);
                self.analyze_expr(right, scopes, function);
                None
            }
            ExprKind::Call { callee, args } => {
                let callee_binding = self.analyze_expr(callee, scopes, function);
                if matches!(
                    callee_binding,
                    Some(Binding {
                        kind: BindingKind::Item,
                        span
                    }) if span == function.name.span
                ) {
                    self.report_direct_recursion(function, callee.span);
                }
                if let Some(Binding {
                    kind: BindingKind::Item,
                    ..
                }) = callee_binding
                {
                    let callee_name = self
                        .resolutions
                        .iter()
                        .find(|resolution| resolution.use_span == name_span(callee))
                        .map(|resolution| resolution.name.clone())
                        .expect("resolved item call should have a recorded name resolution");
                    self.call_edges.push(CallEdge {
                        caller_name: function.name.value.clone(),
                        callee_name,
                        callee_span: callee.span,
                        call_span: callee.span,
                    });
                }
                for arg in args {
                    self.analyze_expr(arg, scopes, function);
                }
                None
            }
            ExprKind::Index { target, index } => {
                self.analyze_expr(target, scopes, function);
                self.analyze_expr(index, scopes, function);
                None
            }
        }
    }

    fn resolve_name(&mut self, name: &str, use_span: Span, scopes: &ScopeStack) -> Option<Binding> {
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
            return Some(binding);
        }

        self.diagnostics.push(
            Diagnostic::error(format!("undefined binding `{name}`"))
                .with_label(Label::primary(use_span, "not found in this scope")),
        );
        None
    }

    fn report_direct_recursion(&mut self, function: &Function, call_span: Span) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "direct recursion is not allowed for `{}`",
                function.name.value
            ))
            .with_label(Label::primary(call_span, "recursive call occurs here"))
            .with_label(Label::secondary(
                function.name.span,
                "recursive function declared here",
            )),
        );
    }

    fn detect_indirect_recursion(&mut self) {
        let mut adjacency: HashMap<String, Vec<CallEdge>> = HashMap::new();
        for edge in &self.call_edges {
            if edge.caller_name != edge.callee_name {
                adjacency
                    .entry(edge.caller_name.clone())
                    .or_default()
                    .push(edge.clone());
            }
        }

        let mut states = HashMap::new();
        let mut stack = Vec::new();
        let mut reported_edges = HashSet::new();
        let mut function_names: Vec<_> = self.items.keys().cloned().collect();
        function_names.sort();
        for function_name in function_names {
            self.visit_call_graph(
                &function_name,
                &adjacency,
                &mut states,
                &mut stack,
                &mut reported_edges,
            );
        }
    }

    fn visit_call_graph(
        &mut self,
        function_name: &str,
        adjacency: &HashMap<String, Vec<CallEdge>>,
        states: &mut HashMap<String, VisitState>,
        stack: &mut Vec<String>,
        reported_edges: &mut HashSet<Span>,
    ) {
        match states.get(function_name) {
            Some(VisitState::Visiting | VisitState::Visited) => return,
            None => {}
        }

        states.insert(function_name.to_owned(), VisitState::Visiting);
        stack.push(function_name.to_owned());

        if let Some(edges) = adjacency.get(function_name) {
            for edge in edges {
                if stack.contains(&edge.callee_name) {
                    if reported_edges.insert(edge.call_span) {
                        self.report_indirect_recursion(edge, stack);
                    }
                    continue;
                }

                self.visit_call_graph(&edge.callee_name, adjacency, states, stack, reported_edges);
            }
        }

        stack.pop();
        states.insert(function_name.to_owned(), VisitState::Visited);
    }

    fn report_indirect_recursion(&mut self, edge: &CallEdge, stack: &[String]) {
        let cycle_start = stack
            .iter()
            .position(|function_name| function_name == &edge.callee_name)
            .expect("callee should appear in the active DFS stack");
        let mut cycle: Vec<&str> = stack[cycle_start..].iter().map(String::as_str).collect();
        cycle.push(edge.callee_name.as_str());
        let cycle_text = cycle.join(" -> ");

        self.diagnostics.push(
            Diagnostic::error(format!("indirect recursion is not allowed: {cycle_text}"))
                .with_label(Label::primary(edge.call_span, "cycle closes here"))
                .with_label(Label::secondary(
                    edge.callee_span,
                    "cycle re-enters this function",
                )),
        );
    }
}

fn name_span(expr: &Expr) -> Span {
    match &expr.kind {
        ExprKind::Name(name) => name.span,
        ExprKind::Grouped(expr) => name_span(expr),
        other => panic!("expected name-like callee expression, got {other:?}"),
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
