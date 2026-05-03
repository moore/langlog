use std::collections::{HashMap, HashSet};

use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, GenericArg, Item, MatchBody, ObserveOp,
    Pattern, PatternKind, Stmt, Type, TypeKind, UnaryOp,
};
use langlog_syntax::{Diagnostic, Label, ParsedModule, Severity, Span, Spanned};

mod hir;

pub use hir::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Item,
    HostBuiltin,
    Param,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Binding {
    kind: BindingKind,
    span: Span,
    loop_bound: bool,
    mutable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostBuiltin {
    ReadU32,
    PrintU32,
    PrintBool,
    PrintNewline,
    Some,
    None,
    Ok,
    Err,
    ArithmeticOverflow,
    ArithmeticUnderflow,
    DivideByZero,
    RemainderByZero,
}

impl HostBuiltin {
    pub const ALL: [Self; 12] = [
        Self::ReadU32,
        Self::PrintU32,
        Self::PrintBool,
        Self::PrintNewline,
        Self::Some,
        Self::None,
        Self::Ok,
        Self::Err,
        Self::ArithmeticOverflow,
        Self::ArithmeticUnderflow,
        Self::DivideByZero,
        Self::RemainderByZero,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::ReadU32 => "read_u32",
            Self::PrintU32 => "print_u32",
            Self::PrintBool => "print_bool",
            Self::PrintNewline => "print_newline",
            Self::Some => "some",
            Self::None => "none",
            Self::Ok => "ok",
            Self::Err => "err",
            Self::ArithmeticOverflow => "arithmetic_overflow",
            Self::ArithmeticUnderflow => "arithmetic_underflow",
            Self::DivideByZero => "divide_by_zero",
            Self::RemainderByZero => "remainder_by_zero",
        }
    }

    pub const fn is_host_import(self) -> bool {
        matches!(
            self,
            Self::ReadU32 | Self::PrintU32 | Self::PrintBool | Self::PrintNewline
        )
    }

    const fn has_generic_signature(self) -> bool {
        matches!(self, Self::Some | Self::None | Self::Ok | Self::Err)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|builtin| builtin.name() == name)
    }
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
    pub mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub parsed: ParsedModule,
    pub diagnostics: Vec<Diagnostic>,
    pub resolutions: Vec<ResolvedName>,
    pub hir: Option<HirProgram>,
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
    let (mut diagnostics, resolutions) = {
        let mut analyzer = Analyzer::new(&parsed);
        analyzer.collect_items();
        analyzer.analyze_module();
        (analyzer.diagnostics, analyzer.resolutions)
    };

    let mut type_checker = TypeChecker::new(&parsed);
    type_checker.check_module();
    diagnostics.extend(type_checker.diagnostics);
    let hir = if diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.severity, Severity::Error))
    {
        None
    } else {
        Some(hir::lower_program(
            &parsed,
            &resolutions,
            &type_checker.facts,
        ))
    };

    CheckedProgram {
        parsed,
        diagnostics,
        resolutions,
        hir,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SemanticType {
    Unit,
    Bool,
    U32,
    ArithmeticError,
    Tuple(Vec<SemanticType>),
    Array {
        element: Box<SemanticType>,
        length: u64,
    },
    Option(Box<SemanticType>),
    Result {
        ok: Box<SemanticType>,
        err: Box<SemanticType>,
    },
    Set {
        element: Box<SemanticType>,
        capacity: u64,
    },
    Map {
        key: Box<SemanticType>,
        value: Box<SemanticType>,
        capacity: u64,
    },
    Range(Box<SemanticType>),
    Named(String),
    Function(FunctionType),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionType {
    params: Vec<SemanticType>,
    return_type: Box<SemanticType>,
}

impl SemanticType {
    fn describe(&self) -> String {
        match self {
            Self::Unit => "()".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::U32 => "u32".to_owned(),
            Self::ArithmeticError => "ArithmeticError".to_owned(),
            Self::Tuple(elements) => format!(
                "({})",
                elements
                    .iter()
                    .map(SemanticType::describe)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Array { element, length } => format!("[{}; {length}]", element.describe()),
            Self::Option(inner) => format!("Option<{}>", inner.describe()),
            Self::Result { ok, err } => format!("Result<{}, {}>", ok.describe(), err.describe()),
            Self::Set { element, capacity } => format!("Set<{}, {capacity}>", element.describe()),
            Self::Map {
                key,
                value,
                capacity,
            } => format!("Map<{}, {}, {capacity}>", key.describe(), value.describe()),
            Self::Range(inner) => format!("range<{}>", inner.describe()),
            Self::Named(name) => name.clone(),
            Self::Function(_) => "function".to_owned(),
            Self::Unknown => "<unknown>".to_owned(),
        }
    }

    fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }

    fn is_u32(&self) -> bool {
        matches!(self, Self::U32)
    }

    fn contains_unknown(&self) -> bool {
        match self {
            Self::Unit
            | Self::Bool
            | Self::U32
            | Self::ArithmeticError
            | Self::Named(_)
            | Self::Unknown => matches!(self, Self::Unknown),
            Self::Function(signature) => {
                signature.params.iter().any(SemanticType::contains_unknown)
                    || signature.return_type.contains_unknown()
            }
            Self::Tuple(elements) => elements.iter().any(SemanticType::contains_unknown),
            Self::Array { element, .. } | Self::Option(element) | Self::Range(element) => {
                element.contains_unknown()
            }
            Self::Result { ok, err } => ok.contains_unknown() || err.contains_unknown(),
            Self::Set { element, .. } => element.contains_unknown(),
            Self::Map { key, value, .. } => key.contains_unknown() || value.contains_unknown(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TypeFacts {
    expr_types: HashMap<Span, SemanticType>,
    binding_types: HashMap<Span, SemanticType>,
}

impl TypeFacts {
    fn record_expr(&mut self, span: Span, ty: SemanticType) -> SemanticType {
        self.expr_types.insert(span, ty.clone());
        ty
    }

    fn record_binding(&mut self, span: Span, ty: SemanticType) {
        self.binding_types.insert(span, ty);
    }

    fn expr_type(&self, span: Span) -> Option<&SemanticType> {
        self.expr_types.get(&span)
    }

    fn binding_type(&self, span: Span) -> Option<&SemanticType> {
        self.binding_types.get(&span)
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
            if HostBuiltin::from_name(function.name.value.as_str()).is_some() {
                self.report_reserved_host_builtin_name(function);
                continue;
            }
            self.items
                .entry(function.name.value.clone())
                .or_insert(Binding {
                    kind: BindingKind::Item,
                    span: function.name.span,
                    loop_bound: false,
                    mutable: false,
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
                    loop_bound: is_bounded_iterable_type(&param.ty),
                    mutable: false,
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
                        loop_bound: stmt.ty.as_ref().is_some_and(is_bounded_iterable_type)
                            || stmt
                                .value
                                .as_ref()
                                .and_then(|expr| self.iterable_bound(expr, scopes))
                                .unwrap_or(false),
                        mutable: stmt.mutable,
                    },
                );
            }
            Stmt::Assign(stmt) => {
                let target_binding = self.analyze_expr(&stmt.target, scopes, function);
                self.analyze_expr(&stmt.value, scopes, function);
                if matches!(target_binding, Some(Binding { mutable: false, .. })) {
                    self.report_immutable_assignment(stmt.target.span);
                }
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
                if matches!(self.iterable_bound(&stmt.iterable, scopes), Some(false)) {
                    self.report_unbounded_iteration(stmt.iterable.span);
                }
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
                self.analyze_expr(&stmt.left, scopes, function);
                self.analyze_expr(&stmt.right, scopes, function);
                self.check_observe_expr_stability(&stmt.left, scopes);
                self.check_observe_expr_stability(&stmt.right, scopes);
                self.analyze_block(&stmt.else_block, scopes, function);
                if !is_terminal_block(&stmt.else_block) {
                    self.report_non_terminal_observe_else(stmt.else_block.span);
                }
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
                    loop_bound: false,
                    mutable: false,
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
            ExprKind::Recover {
                expr,
                error_binding,
                fallback,
            } => {
                self.analyze_expr(expr, scopes, function);
                scopes.push();
                if let Some(binding) = error_binding {
                    scopes.insert(
                        binding.value.clone(),
                        Binding {
                            kind: BindingKind::Local,
                            span: binding.span,
                            loop_bound: false,
                            mutable: false,
                        },
                    );
                }
                self.analyze_expr(fallback, scopes, function);
                scopes.pop();
                None
            }
            ExprKind::Call { callee, args } => {
                let callee_binding = self.analyze_expr(callee, scopes, function);
                if matches!(
                    callee_binding,
                    Some(Binding {
                        kind: BindingKind::Item,
                        span,
                        ..
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
            .or_else(|| {
                HostBuiltin::from_name(name).map(|_| Binding {
                    kind: BindingKind::HostBuiltin,
                    span: use_span,
                    loop_bound: false,
                    mutable: false,
                })
            })
        {
            self.resolutions.push(ResolvedName {
                use_span,
                declaration_span: binding.span,
                kind: binding.kind,
                name: name.to_owned(),
                mutable: binding.mutable,
            });
            return Some(binding);
        }

        self.diagnostics.push(
            Diagnostic::error(format!("undefined binding `{name}`"))
                .with_label(Label::primary(use_span, "not found in this scope")),
        );
        None
    }

    fn report_reserved_host_builtin_name(&mut self, function: &Function) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "`{}` is reserved for a host builtin",
                function.name.value
            ))
            .with_label(Label::primary(
                function.name.span,
                "host builtin names cannot be redefined",
            )),
        );
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

    fn iterable_bound(&self, expr: &Expr, scopes: &ScopeStack) -> Option<bool> {
        match &expr.kind {
            ExprKind::Array(_) => Some(true),
            ExprKind::Binary { op, .. } if *op == langlog_syntax::ast::BinaryOp::Range => {
                Some(true)
            }
            ExprKind::Grouped(expr) => self.iterable_bound(expr, scopes),
            ExprKind::Name(name) => scopes
                .lookup(name.value.as_str())
                .or_else(|| self.items.get(name.value.as_str()).copied())
                .or_else(|| {
                    HostBuiltin::from_name(name.value.as_str()).map(|_| Binding {
                        kind: BindingKind::HostBuiltin,
                        span: name.span,
                        loop_bound: false,
                        mutable: false,
                    })
                })
                .map(|binding| binding.loop_bound),
            ExprKind::Int(_)
            | ExprKind::Bool(_)
            | ExprKind::Tuple(_)
            | ExprKind::Block(_)
            | ExprKind::Unary { .. }
            | ExprKind::Call { .. }
            | ExprKind::Index { .. } => Some(false),
            ExprKind::Binary { .. } => Some(false),
            ExprKind::Recover { .. } => Some(false),
        }
    }

    fn report_unbounded_iteration(&mut self, iterable_span: Span) {
        self.diagnostics.push(
            Diagnostic::error("unbounded iteration is not allowed in phase 1").with_label(
                Label::primary(
                    iterable_span,
                    "iterable is outside the bounded phase 1 loop model",
                ),
            ),
        );
    }

    fn report_non_terminal_observe_else(&mut self, else_span: Span) {
        self.diagnostics.push(
            Diagnostic::error("`observe` `else` blocks must be terminal in phase 1").with_label(
                Label::primary(else_span, "false observations must not fall through"),
            ),
        );
    }

    fn report_immutable_assignment(&mut self, target_span: Span) {
        self.diagnostics.push(
            Diagnostic::error("assignment to an immutable binding is not allowed").with_label(
                Label::primary(target_span, "mark this binding `mut` to assign to it"),
            ),
        );
    }

    fn report_mutable_observe_binding(&mut self, binding_span: Span) {
        self.diagnostics.push(
            Diagnostic::error("mutable bindings are not allowed in `observe` proof expressions")
                .with_label(Label::primary(
                    binding_span,
                    "proof expressions must not reference `mut` bindings",
                )),
        );
    }

    fn check_observe_expr_stability(&mut self, expr: &Expr, scopes: &ScopeStack) {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Bool(_) => {}
            ExprKind::Name(name) => {
                if scopes
                    .lookup(name.value.as_str())
                    .or_else(|| self.items.get(name.value.as_str()).copied())
                    .is_some_and(|binding| binding.mutable)
                {
                    self.report_mutable_observe_binding(name.span);
                }
            }
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.check_observe_expr_stability(element, scopes);
                }
            }
            ExprKind::Block(block) => {
                for statement in &block.statements {
                    if let Stmt::Expr(stmt) = statement {
                        self.check_observe_expr_stability(&stmt.expr, scopes);
                    }
                }
                if let Some(expr) = &block.trailing_expr {
                    self.check_observe_expr_stability(expr, scopes);
                }
            }
            ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
                self.check_observe_expr_stability(expr, scopes);
            }
            ExprKind::Binary { left, right, .. } => {
                self.check_observe_expr_stability(left, scopes);
                self.check_observe_expr_stability(right, scopes);
            }
            ExprKind::Recover { expr, fallback, .. } => {
                self.check_observe_expr_stability(expr, scopes);
                self.check_observe_expr_stability(fallback, scopes);
            }
            ExprKind::Call { callee, args } => {
                self.check_observe_expr_stability(callee, scopes);
                for arg in args {
                    self.check_observe_expr_stability(arg, scopes);
                }
            }
            ExprKind::Index { target, index } => {
                self.check_observe_expr_stability(target, scopes);
                self.check_observe_expr_stability(index, scopes);
            }
        }
    }
}

struct TypeChecker<'a> {
    parsed: &'a ParsedModule,
    diagnostics: Vec<Diagnostic>,
    item_signatures: HashMap<String, FunctionType>,
    facts: TypeFacts,
}

impl<'a> TypeChecker<'a> {
    fn new(parsed: &'a ParsedModule) -> Self {
        let mut item_signatures: HashMap<String, FunctionType> = HostBuiltin::ALL
            .iter()
            .copied()
            .map(|builtin| (builtin.name().to_owned(), host_builtin_signature(builtin)))
            .collect();
        item_signatures.extend(parsed.module.items.iter().map(|item| {
            let Item::Function(function) = item;
            (function.name.value.clone(), function_signature(function))
        }));

        Self {
            parsed,
            diagnostics: Vec::new(),
            item_signatures,
            facts: TypeFacts::default(),
        }
    }

    fn check_module(&mut self) {
        for item in &self.parsed.module.items {
            let Item::Function(function) = item;
            self.check_function(function);
        }
    }

    fn check_function(&mut self, function: &Function) {
        let mut scopes = TypeScopeStack::default();
        scopes.push();
        for param in &function.params {
            let param_type = lower_type(&param.ty);
            self.facts
                .record_binding(param.name.span, param_type.clone());
            scopes.insert(param.name.value.clone(), param_type);
        }

        let expected_return = function
            .return_type
            .as_ref()
            .map(lower_type)
            .unwrap_or(SemanticType::Unit);
        let body_type = self.check_block(&function.body, &mut scopes, &expected_return);

        if let Some(expr) = &function.body.trailing_expr {
            self.require_same_type(expr.span, &expected_return, &body_type);
        } else if !is_terminal_block(&function.body) {
            self.require_same_type(function.body.span, &expected_return, &SemanticType::Unit);
        }

        scopes.pop();
    }

    fn check_block(
        &mut self,
        block: &Block,
        scopes: &mut TypeScopeStack,
        expected_return: &SemanticType,
    ) -> SemanticType {
        scopes.push();
        for statement in &block.statements {
            self.check_statement(statement, scopes, expected_return);
        }
        let result = block
            .trailing_expr
            .as_deref()
            .map(|expr| self.check_expr_with_expected(expr, scopes, Some(expected_return)))
            .unwrap_or(SemanticType::Unit);
        scopes.pop();
        result
    }

    fn check_statement(
        &mut self,
        statement: &Stmt,
        scopes: &mut TypeScopeStack,
        expected_return: &SemanticType,
    ) {
        match statement {
            Stmt::Let(stmt) => {
                let annotation_type = stmt.ty.as_ref().map(lower_type);
                let value_type = stmt.value.as_ref().map(|expr| {
                    self.check_expr_with_expected(expr, scopes, annotation_type.as_ref())
                });
                if stmt.ty.is_none() && stmt.value.is_none() {
                    self.report_let_requires_type_or_initializer(stmt.name.span);
                }
                if let (Some(annotation_type), Some(value_type)) = (&annotation_type, &value_type) {
                    self.require_same_type(stmt.span, annotation_type, value_type);
                }
                let binding_type = stmt
                    .ty
                    .as_ref()
                    .map(lower_type)
                    .or(value_type)
                    .unwrap_or(SemanticType::Unknown);
                self.facts
                    .record_binding(stmt.name.span, binding_type.clone());
                scopes.insert(stmt.name.value.clone(), binding_type);
            }
            Stmt::Assign(stmt) => {
                let target = self.check_expr(&stmt.target, scopes);
                let value = self.check_expr_with_expected(&stmt.value, scopes, Some(&target));
                self.require_same_type(stmt.value.span, &target, &value);
            }
            Stmt::Expr(stmt) => {
                self.check_expr(&stmt.expr, scopes);
            }
            Stmt::If(stmt) => {
                let condition_type = self.check_expr(&stmt.condition, scopes);
                self.require_bool(stmt.condition.span, &condition_type, "if conditions");
                self.check_block(&stmt.then_block, scopes, expected_return);
                if let Some(else_branch) = &stmt.else_branch {
                    self.check_else_branch(else_branch, scopes, expected_return);
                }
            }
            Stmt::Match(stmt) => {
                let scrutinee_type = self.check_expr(&stmt.expr, scopes);
                for arm in &stmt.arms {
                    scopes.push();
                    self.bind_pattern_type(&arm.pattern, scopes, &scrutinee_type);
                    match &arm.body {
                        MatchBody::Block(block) => {
                            self.check_block(block, scopes, expected_return);
                        }
                        MatchBody::Expr(expr) => {
                            self.check_expr(expr, scopes);
                        }
                    }
                    scopes.pop();
                }
            }
            Stmt::For(stmt) => {
                let iterable_type = self.check_expr(&stmt.iterable, scopes);
                scopes.push();
                self.bind_pattern_type(
                    &stmt.binding,
                    scopes,
                    &iterable_item_type(&iterable_type).unwrap_or(SemanticType::Unknown),
                );
                self.check_block(&stmt.body, scopes, expected_return);
                scopes.pop();
            }
            Stmt::Return(stmt) => {
                let value_type = stmt
                    .value
                    .as_ref()
                    .map(|value| {
                        self.check_expr_with_expected(value, scopes, Some(expected_return))
                    })
                    .unwrap_or(SemanticType::Unit);
                self.require_same_type(stmt.span, expected_return, &value_type);
            }
            Stmt::Observe(stmt) => {
                let left = self.check_expr(&stmt.left, scopes);
                let right = self.check_expr(&stmt.right, scopes);
                self.require_observe_types(stmt.span, stmt.op, &left, &right);
                self.check_block(&stmt.else_block, scopes, expected_return);
            }
        }
    }

    fn check_else_branch(
        &mut self,
        branch: &ElseBranch,
        scopes: &mut TypeScopeStack,
        expected_return: &SemanticType,
    ) {
        match branch {
            ElseBranch::Block(block) => {
                self.check_block(block, scopes, expected_return);
            }
            ElseBranch::If(stmt) => {
                self.check_statement(&Stmt::If(*stmt.clone()), scopes, expected_return);
            }
        }
    }

    fn bind_pattern_type(
        &mut self,
        pattern: &Pattern,
        scopes: &mut TypeScopeStack,
        value_type: &SemanticType,
    ) {
        match &pattern.kind {
            PatternKind::Binding(name) => {
                self.facts.record_binding(name.span, value_type.clone());
                scopes.insert(name.value.clone(), value_type.clone());
            }
            PatternKind::Wildcard | PatternKind::Int(_) | PatternKind::Bool(_) => {}
        }
    }

    fn check_expr(&mut self, expr: &Expr, scopes: &mut TypeScopeStack) -> SemanticType {
        self.check_expr_with_expected(expr, scopes, None)
    }

    fn check_expr_with_expected(
        &mut self,
        expr: &Expr,
        scopes: &mut TypeScopeStack,
        expected: Option<&SemanticType>,
    ) -> SemanticType {
        let ty = match &expr.kind {
            ExprKind::Int(_) => SemanticType::U32,
            ExprKind::Bool(_) => SemanticType::Bool,
            ExprKind::Name(name) => scopes
                .lookup(name.value.as_str())
                .or_else(|| {
                    self.item_signatures
                        .get(name.value.as_str())
                        .cloned()
                        .map(SemanticType::Function)
                })
                .unwrap_or(SemanticType::Unknown),
            ExprKind::Tuple(elements) => SemanticType::Tuple(
                elements
                    .iter()
                    .map(|element| self.check_expr(element, scopes))
                    .collect(),
            ),
            ExprKind::Array(elements) => self.check_array_expr(expr.span, elements, scopes),
            ExprKind::Block(block) => self.check_block(block, scopes, &SemanticType::Unknown),
            ExprKind::Unary { op, expr } => {
                let operand = self.check_expr(expr, scopes);
                match op {
                    UnaryOp::Neg => {
                        self.require_u32(expr.span, &operand, "arithmetic operators");
                        SemanticType::U32
                    }
                    UnaryOp::Not => {
                        self.require_bool(expr.span, &operand, "logical operators");
                        SemanticType::Bool
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let left_type = self.check_expr(left, scopes);
                let right_type = self.check_expr(right, scopes);
                self.check_binary_expr(expr.span, *op, &left_type, &right_type)
            }
            ExprKind::Recover {
                expr: target,
                error_binding,
                fallback,
            } => self.check_recovery_expr(
                expr.span,
                target,
                error_binding.as_ref(),
                fallback,
                scopes,
            ),
            ExprKind::Call { callee, args } => {
                if let Some(builtin) = callee_builtin(callee) {
                    let ty = self.check_builtin_call_expr(
                        expr.span, callee, builtin, args, scopes, expected,
                    );
                    return self.facts.record_expr(expr.span, ty);
                }
                let callee_type = self.check_expr(callee, scopes);
                self.check_call_expr(callee.span, &callee_type, args, scopes)
            }
            ExprKind::Index { target, index } => {
                let target_type = self.check_expr(target, scopes);
                let index_type = self.check_expr(index, scopes);
                self.check_index_expr(expr.span, &target_type, index.span, &index_type)
            }
            ExprKind::Grouped(expr) => self.check_expr(expr, scopes),
        };

        self.facts.record_expr(expr.span, ty)
    }

    fn check_array_expr(
        &mut self,
        span: Span,
        elements: &[Expr],
        scopes: &mut TypeScopeStack,
    ) -> SemanticType {
        if elements.is_empty() {
            self.report_empty_array_literal_requires_context(span);
            return SemanticType::Unknown;
        }

        let mut element_type = None;
        for element in elements {
            let current = self.check_expr(element, scopes);
            if let Some(expected) = &element_type {
                self.require_same_type(element.span, expected, &current);
            } else {
                element_type = Some(current);
            }
        }

        SemanticType::Array {
            element: Box::new(element_type.unwrap_or(SemanticType::Unknown)),
            length: elements.len() as u64,
        }
    }

    fn check_binary_expr(
        &mut self,
        span: Span,
        op: BinaryOp,
        left: &SemanticType,
        right: &SemanticType,
    ) -> SemanticType {
        match op {
            BinaryOp::Range => {
                self.require_u32(span, left, "range expressions");
                self.require_u32(span, right, "range expressions");
                SemanticType::Range(Box::new(SemanticType::U32))
            }
            BinaryOp::Or | BinaryOp::And => {
                self.require_bool(span, left, "logical operators");
                self.require_bool(span, right, "logical operators");
                SemanticType::Bool
            }
            BinaryOp::EqEq | BinaryOp::NotEq => {
                self.require_same_type(span, left, right);
                SemanticType::Bool
            }
            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
                self.require_u32(span, left, "ordering comparisons");
                self.require_u32(span, right, "ordering comparisons");
                SemanticType::Bool
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                let left_inner = self.check_arithmetic_operand(span, left);
                let right_inner = self.check_arithmetic_operand(span, right);
                if let (Some(left_inner), Some(right_inner)) = (&left_inner, &right_inner) {
                    self.require_same_type(span, left_inner, right_inner);
                }
                SemanticType::Result {
                    ok: Box::new(SemanticType::U32),
                    err: Box::new(SemanticType::ArithmeticError),
                }
            }
        }
    }

    fn check_arithmetic_operand(&mut self, span: Span, ty: &SemanticType) -> Option<SemanticType> {
        match ty {
            SemanticType::Unknown => None,
            SemanticType::U32 => Some(SemanticType::U32),
            SemanticType::Result { ok, err }
                if **ok == SemanticType::U32 && **err == SemanticType::ArithmeticError =>
            {
                Some(SemanticType::U32)
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "arithmetic operators must have type u32 or Result<u32, ArithmeticError>",
                    )
                    .with_label(Label::primary(
                        span,
                        "expected `u32` or `Result<u32, ArithmeticError>` here",
                    )),
                );
                None
            }
        }
    }

    fn check_builtin_call_expr(
        &mut self,
        call_span: Span,
        callee: &Expr,
        builtin: HostBuiltin,
        args: &[Expr],
        scopes: &mut TypeScopeStack,
        expected: Option<&SemanticType>,
    ) -> SemanticType {
        match builtin {
            HostBuiltin::Some => {
                if !self.require_builtin_arity(callee.span, builtin, 1, args.len()) {
                    return SemanticType::Option(Box::new(SemanticType::Unknown));
                }
                let expected_inner = match expected {
                    Some(SemanticType::Option(inner)) => Some(inner.as_ref()),
                    _ => None,
                };
                let value = self.check_expr_with_expected(&args[0], scopes, expected_inner);
                let return_type = SemanticType::Option(Box::new(value.clone()));
                self.record_builtin_callee_type(callee.span, vec![value], return_type.clone());
                return_type
            }
            HostBuiltin::None => {
                if !self.require_builtin_arity(callee.span, builtin, 0, args.len()) {
                    return SemanticType::Option(Box::new(SemanticType::Unknown));
                }
                let return_type = match expected {
                    Some(SemanticType::Option(inner)) => {
                        SemanticType::Option(Box::new((**inner).clone()))
                    }
                    _ => {
                        self.report_cannot_infer_builtin(call_span, "none");
                        SemanticType::Option(Box::new(SemanticType::Unknown))
                    }
                };
                self.record_builtin_callee_type(callee.span, Vec::new(), return_type.clone());
                return_type
            }
            HostBuiltin::Ok => {
                if !self.require_builtin_arity(callee.span, builtin, 1, args.len()) {
                    return arithmetic_result(SemanticType::Unknown);
                }
                let expected_ok = match expected {
                    Some(SemanticType::Result { ok, err })
                        if **err == SemanticType::ArithmeticError =>
                    {
                        Some(ok.as_ref())
                    }
                    _ => None,
                };
                let value = self.check_expr_with_expected(&args[0], scopes, expected_ok);
                let return_type = arithmetic_result(value.clone());
                self.record_builtin_callee_type(callee.span, vec![value], return_type.clone());
                return_type
            }
            HostBuiltin::Err => {
                if !self.require_builtin_arity(callee.span, builtin, 1, args.len()) {
                    return arithmetic_result(SemanticType::Unknown);
                }
                let err_type = self.check_expr_with_expected(
                    &args[0],
                    scopes,
                    Some(&SemanticType::ArithmeticError),
                );
                self.require_same_type(args[0].span, &SemanticType::ArithmeticError, &err_type);
                let return_type = match expected {
                    Some(SemanticType::Result { ok, err })
                        if **err == SemanticType::ArithmeticError =>
                    {
                        SemanticType::Result {
                            ok: Box::new((**ok).clone()),
                            err: Box::new(SemanticType::ArithmeticError),
                        }
                    }
                    _ => {
                        self.report_cannot_infer_builtin(call_span, "err");
                        arithmetic_result(SemanticType::Unknown)
                    }
                };
                self.record_builtin_callee_type(
                    callee.span,
                    vec![SemanticType::ArithmeticError],
                    return_type.clone(),
                );
                return_type
            }
            HostBuiltin::ArithmeticOverflow
            | HostBuiltin::ArithmeticUnderflow
            | HostBuiltin::DivideByZero
            | HostBuiltin::RemainderByZero => {
                if !self.require_builtin_arity(callee.span, builtin, 0, args.len()) {
                    return SemanticType::ArithmeticError;
                }
                let return_type = SemanticType::ArithmeticError;
                self.record_builtin_callee_type(callee.span, Vec::new(), return_type.clone());
                return_type
            }
            HostBuiltin::ReadU32
            | HostBuiltin::PrintU32
            | HostBuiltin::PrintBool
            | HostBuiltin::PrintNewline => {
                let signature = host_builtin_signature(builtin);
                self.record_builtin_callee_type(
                    callee.span,
                    signature.params.clone(),
                    (*signature.return_type).clone(),
                );
                self.check_call_expr(
                    callee.span,
                    &SemanticType::Function(signature),
                    args,
                    scopes,
                )
            }
        }
    }

    fn record_builtin_callee_type(
        &mut self,
        span: Span,
        params: Vec<SemanticType>,
        return_type: SemanticType,
    ) {
        self.facts.record_expr(
            span,
            SemanticType::Function(FunctionType {
                params,
                return_type: Box::new(return_type),
            }),
        );
    }

    fn require_builtin_arity(
        &mut self,
        callee_span: Span,
        builtin: HostBuiltin,
        expected: usize,
        found: usize,
    ) -> bool {
        if expected == found {
            return true;
        }
        self.report_call_arity_mismatch(callee_span, expected, found);
        if builtin.has_generic_signature() {
            return false;
        }
        false
    }

    fn check_recovery_expr(
        &mut self,
        span: Span,
        target: &Expr,
        error_binding: Option<&Spanned<String>>,
        fallback: &Expr,
        scopes: &mut TypeScopeStack,
    ) -> SemanticType {
        let target_type = self.check_expr(target, scopes);
        match (error_binding, target_type) {
            (None, SemanticType::Option(inner)) => {
                let fallback_type =
                    self.check_expr_with_expected(fallback, scopes, Some(inner.as_ref()));
                self.require_same_type(fallback.span, inner.as_ref(), &fallback_type);
                *inner
            }
            (Some(binding), SemanticType::Result { ok, err }) => {
                scopes.push();
                self.facts.record_binding(binding.span, (*err).clone());
                scopes.insert(binding.value.clone(), (*err).clone());
                let fallback_type =
                    self.check_expr_with_expected(fallback, scopes, Some(ok.as_ref()));
                scopes.pop();
                self.require_same_type(fallback.span, ok.as_ref(), &fallback_type);
                *ok
            }
            (None, SemanticType::Unknown) | (Some(_), SemanticType::Unknown) => {
                self.check_expr(fallback, scopes);
                SemanticType::Unknown
            }
            (None, found) => {
                self.report_type_mismatch(
                    span,
                    &SemanticType::Option(Box::new(SemanticType::Unknown)),
                    &found,
                );
                self.check_expr(fallback, scopes);
                SemanticType::Unknown
            }
            (Some(binding), found) => {
                self.facts
                    .record_binding(binding.span, SemanticType::Unknown);
                self.report_type_mismatch(
                    span,
                    &SemanticType::Result {
                        ok: Box::new(SemanticType::Unknown),
                        err: Box::new(SemanticType::Unknown),
                    },
                    &found,
                );
                self.check_expr(fallback, scopes);
                SemanticType::Unknown
            }
        }
    }

    fn check_call_expr(
        &mut self,
        callee_span: Span,
        callee_type: &SemanticType,
        args: &[Expr],
        scopes: &mut TypeScopeStack,
    ) -> SemanticType {
        let SemanticType::Function(signature) = callee_type else {
            if !matches!(callee_type, SemanticType::Unknown) {
                self.report_called_non_function(callee_span);
            }
            return SemanticType::Unknown;
        };

        if args.len() != signature.params.len() {
            self.report_call_arity_mismatch(callee_span, signature.params.len(), args.len());
            return (*signature.return_type).clone();
        }

        for (arg, expected) in args.iter().zip(signature.params.iter()) {
            let found = self.check_expr_with_expected(arg, scopes, Some(expected));
            self.require_same_type(arg.span, expected, &found);
        }

        (*signature.return_type).clone()
    }

    fn check_index_expr(
        &mut self,
        expr_span: Span,
        target_type: &SemanticType,
        index_span: Span,
        index_type: &SemanticType,
    ) -> SemanticType {
        match target_type {
            SemanticType::Array { element, .. } => {
                self.require_u32(index_span, index_type, "array indices");
                (**element).clone()
            }
            SemanticType::Map { key, value, .. } => {
                self.require_same_type(index_span, key, index_type);
                (**value).clone()
            }
            SemanticType::Unknown => SemanticType::Unknown,
            _ => {
                self.report_non_indexable_target(expr_span);
                SemanticType::Unknown
            }
        }
    }

    fn require_same_type(&mut self, span: Span, expected: &SemanticType, found: &SemanticType) {
        if matches!(expected, SemanticType::Unknown) || matches!(found, SemanticType::Unknown) {
            return;
        }
        if expected == found {
            return;
        }

        self.report_type_mismatch(span, expected, found);
    }

    fn require_bool(&mut self, span: Span, found: &SemanticType, context: &str) {
        if matches!(found, SemanticType::Unknown) || found.is_bool() {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(format!("{context} must have type bool"))
                .with_label(Label::primary(span, "expected `bool` here")),
        );
    }

    fn require_u32(&mut self, span: Span, found: &SemanticType, context: &str) {
        if matches!(found, SemanticType::Unknown) || found.is_u32() {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(format!("{context} must have type u32"))
                .with_label(Label::primary(span, "expected `u32` here")),
        );
    }

    fn require_observe_types(
        &mut self,
        span: Span,
        op: ObserveOp,
        left: &SemanticType,
        right: &SemanticType,
    ) {
        match op {
            ObserveOp::Eq | ObserveOp::NotEq => self.require_same_type(span, left, right),
            ObserveOp::Lt | ObserveOp::LtEq | ObserveOp::Gt | ObserveOp::GtEq => {
                self.require_u32(span, left, "observe ordering comparisons");
                self.require_u32(span, right, "observe ordering comparisons");
            }
        }
    }

    fn report_type_mismatch(&mut self, span: Span, expected: &SemanticType, found: &SemanticType) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "type mismatch: expected {}, found {}",
                expected.describe(),
                found.describe()
            ))
            .with_label(Label::primary(span, "types do not match here")),
        );
    }

    fn report_called_non_function(&mut self, callee_span: Span) {
        self.diagnostics.push(
            Diagnostic::error("calls require a function-valued callee").with_label(Label::primary(
                callee_span,
                "this expression is not callable",
            )),
        );
    }

    fn report_call_arity_mismatch(&mut self, callee_span: Span, expected: usize, found: usize) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "call arity mismatch: expected {expected} argument(s), found {found}"
            ))
            .with_label(Label::primary(callee_span, "adjust this call signature")),
        );
    }

    fn report_non_indexable_target(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("indexing requires an array or map target").with_label(
                Label::primary(span, "this expression is not an array or map"),
            ),
        );
    }

    fn report_let_requires_type_or_initializer(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("let bindings require a type annotation or initializer").with_label(
                Label::primary(span, "add a type annotation or initializer here"),
            ),
        );
    }

    fn report_empty_array_literal_requires_context(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("empty array literals require an explicit element type").with_label(
                Label::primary(
                    span,
                    "add a type annotation or a non-empty array literal here",
                ),
            ),
        );
    }

    fn report_cannot_infer_builtin(&mut self, span: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("cannot infer type for builtin `{name}`"))
                .with_label(Label::primary(span, "add a type annotation here")),
        );
    }
}

#[derive(Debug, Default)]
struct TypeScopeStack {
    scopes: Vec<HashMap<String, SemanticType>>,
}

impl TypeScopeStack {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn insert(&mut self, name: String, ty: SemanticType) {
        self.scopes
            .last_mut()
            .expect("type scope stack must not be empty")
            .insert(name, ty);
    }

    fn lookup(&self, name: &str) -> Option<SemanticType> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }
}

fn name_span(expr: &Expr) -> Span {
    match &expr.kind {
        ExprKind::Name(name) => name.span,
        ExprKind::Grouped(expr) => name_span(expr),
        other => panic!("expected name-like callee expression, got {other:?}"),
    }
}

fn callee_builtin(expr: &Expr) -> Option<HostBuiltin> {
    match &expr.kind {
        ExprKind::Name(name) => HostBuiltin::from_name(name.value.as_str()),
        ExprKind::Grouped(expr) => callee_builtin(expr),
        _ => None,
    }
}

fn arithmetic_result(ok: SemanticType) -> SemanticType {
    SemanticType::Result {
        ok: Box::new(ok),
        err: Box::new(SemanticType::ArithmeticError),
    }
}

fn function_signature(function: &Function) -> FunctionType {
    FunctionType {
        params: function
            .params
            .iter()
            .map(|param| lower_type(&param.ty))
            .collect(),
        return_type: Box::new(
            function
                .return_type
                .as_ref()
                .map(lower_type)
                .unwrap_or(SemanticType::Unit),
        ),
    }
}

fn host_builtin_signature(builtin: HostBuiltin) -> FunctionType {
    match builtin {
        HostBuiltin::ReadU32 => FunctionType {
            params: Vec::new(),
            return_type: Box::new(SemanticType::U32),
        },
        HostBuiltin::PrintU32 => FunctionType {
            params: vec![SemanticType::U32],
            return_type: Box::new(SemanticType::Unit),
        },
        HostBuiltin::PrintBool => FunctionType {
            params: vec![SemanticType::Bool],
            return_type: Box::new(SemanticType::Unit),
        },
        HostBuiltin::PrintNewline => FunctionType {
            params: Vec::new(),
            return_type: Box::new(SemanticType::Unit),
        },
        HostBuiltin::Some => FunctionType {
            params: vec![SemanticType::Unknown],
            return_type: Box::new(SemanticType::Option(Box::new(SemanticType::Unknown))),
        },
        HostBuiltin::None => FunctionType {
            params: Vec::new(),
            return_type: Box::new(SemanticType::Option(Box::new(SemanticType::Unknown))),
        },
        HostBuiltin::Ok => FunctionType {
            params: vec![SemanticType::Unknown],
            return_type: Box::new(arithmetic_result(SemanticType::Unknown)),
        },
        HostBuiltin::Err => FunctionType {
            params: vec![SemanticType::ArithmeticError],
            return_type: Box::new(arithmetic_result(SemanticType::Unknown)),
        },
        HostBuiltin::ArithmeticOverflow
        | HostBuiltin::ArithmeticUnderflow
        | HostBuiltin::DivideByZero
        | HostBuiltin::RemainderByZero => FunctionType {
            params: Vec::new(),
            return_type: Box::new(SemanticType::ArithmeticError),
        },
    }
}

fn lower_type(ty: &Type) -> SemanticType {
    match &ty.kind {
        TypeKind::Unit => SemanticType::Unit,
        TypeKind::Named(name) => match name.value.as_str() {
            "u32" => SemanticType::U32,
            "bool" => SemanticType::Bool,
            "ArithmeticError" => SemanticType::ArithmeticError,
            _ => SemanticType::Named(name.value.clone()),
        },
        TypeKind::Tuple(elements) => SemanticType::Tuple(elements.iter().map(lower_type).collect()),
        TypeKind::Array { element, length } => SemanticType::Array {
            element: Box::new(lower_type(element)),
            length: length.value,
        },
        TypeKind::Applied { base, args } => match (base.value.as_str(), args.as_slice()) {
            ("Option", [GenericArg::Type(inner)]) => {
                SemanticType::Option(Box::new(lower_type(inner)))
            }
            ("Result", [GenericArg::Type(ok), GenericArg::Type(err)]) => SemanticType::Result {
                ok: Box::new(lower_type(ok)),
                err: Box::new(lower_type(err)),
            },
            ("Set", [GenericArg::Type(element), GenericArg::Const(capacity)]) => {
                SemanticType::Set {
                    element: Box::new(lower_type(element)),
                    capacity: capacity.value,
                }
            }
            (
                "Map",
                [GenericArg::Type(key), GenericArg::Type(value), GenericArg::Const(capacity)],
            ) => SemanticType::Map {
                key: Box::new(lower_type(key)),
                value: Box::new(lower_type(value)),
                capacity: capacity.value,
            },
            _ => SemanticType::Named(base.value.clone()),
        },
    }
}

fn iterable_item_type(ty: &SemanticType) -> Option<SemanticType> {
    match ty {
        SemanticType::Array { element, .. } => Some((**element).clone()),
        SemanticType::Range(inner) => Some((**inner).clone()),
        SemanticType::Set { element, .. } => Some((**element).clone()),
        SemanticType::Map { key, value, .. } => Some(SemanticType::Tuple(vec![
            (**key).clone(),
            (**value).clone(),
        ])),
        _ => None,
    }
}

fn is_bounded_iterable_type(ty: &langlog_syntax::ast::Type) -> bool {
    match &ty.kind {
        langlog_syntax::ast::TypeKind::Array { .. } => true,
        langlog_syntax::ast::TypeKind::Applied { base, .. } => {
            matches!(base.value.as_str(), "Set" | "Map")
        }
        _ => false,
    }
}

fn is_terminal_block(block: &Block) -> bool {
    block.trailing_expr.is_none() && block.statements.last().is_some_and(is_terminal_statement)
}

fn is_terminal_statement(statement: &Stmt) -> bool {
    match statement {
        Stmt::Return(_) => true,
        Stmt::If(stmt) => {
            is_terminal_block(&stmt.then_block)
                && stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(is_terminal_else_branch)
        }
        Stmt::Match(stmt) => {
            !stmt.arms.is_empty()
                && stmt
                    .arms
                    .iter()
                    .all(|arm| is_terminal_match_body(&arm.body))
        }
        _ => false,
    }
}

fn is_terminal_else_branch(branch: &ElseBranch) -> bool {
    match branch {
        ElseBranch::Block(block) => is_terminal_block(block),
        ElseBranch::If(stmt) => {
            is_terminal_block(&stmt.then_block)
                && stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(is_terminal_else_branch)
        }
    }
}

fn is_terminal_match_body(body: &MatchBody) -> bool {
    match body {
        MatchBody::Block(block) => is_terminal_block(block),
        MatchBody::Expr(_) => false,
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
