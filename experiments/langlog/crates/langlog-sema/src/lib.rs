use std::collections::{HashMap, HashSet};

use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, GenericArg, Item, MarkerAnnotation,
    MarkerArg, MarkerArgKind, MarkerFamily, MarkerImplicationStmt, MarkerRefinement, MarkerRule,
    MarkerRuleBlock, MarkerRuleStmt, MatchBody, ObserveOp, Pattern, PatternKind, Stmt, Task, Type,
    TypeKind, UnaryOp,
};
use langlog_syntax::{Diagnostic, Label, ParsedModule, Severity, Span, Spanned};

mod hir;

pub use hir::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Item,
    TaskItem,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Function,
    Task,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkerFamilySignature {
    arity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ItemSignature {
    kind: ItemKind,
    signature: FunctionType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskStateSignature {
    params: Vec<SemanticType>,
    span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ItemContext<'a> {
    kind: ItemKind,
    name: &'a str,
    name_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ControlContext<'a> {
    item: ItemContext<'a>,
    forever_depth: usize,
    in_task_state: bool,
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
            if let Some((name, kind)) = item_name_and_kind(item) {
                if HostBuiltin::from_name(name.value.as_str()).is_some() {
                    self.report_reserved_host_builtin_name(name);
                    continue;
                }
                self.items.entry(name.value.clone()).or_insert(Binding {
                    kind: match kind {
                        ItemKind::Function => BindingKind::Item,
                        ItemKind::Task => BindingKind::TaskItem,
                    },
                    span: name.span,
                    loop_bound: false,
                    mutable: false,
                });
            }
        }
    }

    fn analyze_module(&mut self) {
        for item in &self.parsed.module.items {
            match item {
                Item::Function(function) => self.analyze_function(function),
                Item::Task(task) => self.analyze_task(task),
                Item::MarkerFamily(_) => {}
                Item::MarkerRule(rule) => self.analyze_marker_rule(rule),
            }
        }

        self.detect_indirect_recursion();
    }

    fn analyze_function(&mut self, function: &Function) {
        let context = ItemContext {
            kind: ItemKind::Function,
            name: function.name.value.as_str(),
            name_span: function.name.span,
        };
        self.analyze_callable(
            &function.params,
            function.return_type.as_ref(),
            &function.body,
            context,
        );
    }

    fn analyze_task(&mut self, task: &Task) {
        let context = ItemContext {
            kind: ItemKind::Task,
            name: task.name.value.as_str(),
            name_span: task.name.span,
        };
        let mut scopes = ScopeStack::default();
        scopes.push();
        for param in &task.params {
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
        for param in &task.params {
            self.analyze_type_marker_args(&param.ty, &scopes);
        }
        self.analyze_type_marker_args(&task.return_type, &scopes);

        let mut field_names = HashMap::new();
        for field in &task.fields {
            if let Some(previous) = field_names.insert(field.name.value.clone(), field.name.span) {
                self.report_duplicate_task_field(field.name.span, previous, &field.name.value);
            }
            self.analyze_type_marker_args(&field.ty, &scopes);
            let mut control = ControlContext {
                item: context,
                forever_depth: 0,
                in_task_state: false,
            };
            self.analyze_expr(&field.value, &mut scopes, &mut control);
            scopes.insert(
                field.name.value.clone(),
                Binding {
                    kind: BindingKind::Local,
                    span: field.name.span,
                    loop_bound: is_bounded_iterable_type(&field.ty),
                    mutable: field.mutable,
                },
            );
        }

        let mut state_names = HashMap::new();
        for state in &task.states {
            if let Some(previous) = state_names.insert(state.name.value.clone(), state.name.span) {
                self.report_duplicate_task_state(state.name.span, previous, &state.name.value);
            }
        }

        for state in &task.states {
            scopes.push();
            for param in &state.params {
                if field_names.contains_key(&param.name.value) {
                    self.report_state_param_collides_with_task_field(param.name.span);
                }
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
            for param in &state.params {
                self.analyze_type_marker_args(&param.ty, &scopes);
            }
            let mut control = ControlContext {
                item: context,
                forever_depth: 0,
                in_task_state: true,
            };
            self.analyze_block(&state.body, &mut scopes, &mut control);
            scopes.pop();
        }
        scopes.pop();
    }

    fn analyze_marker_rule(&mut self, rule: &MarkerRule) {
        let mut scopes = ScopeStack::default();
        scopes.push();
        for param in &rule.params {
            scopes.insert(
                param.name.value.clone(),
                Binding {
                    kind: BindingKind::Local,
                    span: param.name.span,
                    loop_bound: false,
                    mutable: false,
                },
            );
        }
        self.analyze_marker_rule_block(&rule.body, &mut scopes);
    }

    fn analyze_marker_rule_block(&mut self, block: &MarkerRuleBlock, scopes: &mut ScopeStack) {
        scopes.push();
        for statement in &block.statements {
            match statement {
                MarkerRuleStmt::If(stmt) => {
                    self.resolve_name(
                        stmt.refinement.subject.value.as_str(),
                        stmt.refinement.subject.span,
                        scopes,
                    );
                    self.analyze_marker_annotation_args(&stmt.refinement.marker, scopes);
                    let bindings = marker_pattern_bindings(&stmt.refinement.marker);
                    scopes.push();
                    for binding in bindings {
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
                    self.analyze_marker_rule_block(&stmt.body, scopes);
                    scopes.pop();
                }
                MarkerRuleStmt::Implies(stmt) => {
                    self.analyze_marker_annotation_args(&stmt.marker, scopes);
                    self.resolve_name(stmt.target.value.as_str(), stmt.target.span, scopes);
                }
            }
        }
        scopes.pop();
    }

    fn analyze_callable(
        &mut self,
        params: &[langlog_syntax::ast::Param],
        return_type: Option<&Type>,
        body: &Block,
        context: ItemContext<'_>,
    ) {
        let mut scopes = ScopeStack::default();
        scopes.push();
        for param in params {
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
        for param in params {
            self.analyze_type_marker_args(&param.ty, &scopes);
        }
        if let Some(return_type) = return_type {
            self.analyze_type_marker_args(return_type, &scopes);
        }

        let mut control = ControlContext {
            item: context,
            forever_depth: 0,
            in_task_state: false,
        };
        self.analyze_block(body, &mut scopes, &mut control);
    }

    fn analyze_block(
        &mut self,
        block: &Block,
        scopes: &mut ScopeStack,
        context: &mut ControlContext<'_>,
    ) {
        scopes.push();
        for statement in &block.statements {
            self.analyze_statement(statement, scopes, context);
        }
        if let Some(expr) = &block.trailing_expr {
            self.analyze_expr(expr, scopes, context);
        }
        scopes.pop();
    }

    fn analyze_statement(
        &mut self,
        statement: &Stmt,
        scopes: &mut ScopeStack,
        context: &mut ControlContext<'_>,
    ) {
        match statement {
            Stmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.analyze_expr(value, scopes, context);
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
                if let Some(ty) = &stmt.ty {
                    self.analyze_type_marker_args(ty, scopes);
                }
            }
            Stmt::Assign(stmt) => {
                let target_binding = self.analyze_expr(&stmt.target, scopes, context);
                self.analyze_expr(&stmt.value, scopes, context);
                if matches!(target_binding, Some(Binding { mutable: false, .. })) {
                    self.report_immutable_assignment(stmt.target.span);
                }
            }
            Stmt::Expr(stmt) => {
                self.analyze_expr(&stmt.expr, scopes, context);
            }
            Stmt::If(stmt) => {
                self.analyze_expr(&stmt.condition, scopes, context);
                self.analyze_block(&stmt.then_block, scopes, context);
                if let Some(else_branch) = &stmt.else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.analyze_block(block, scopes, context),
                        ElseBranch::If(stmt) => {
                            self.analyze_statement(&Stmt::If(*stmt.clone()), scopes, context)
                        }
                    }
                }
            }
            Stmt::Match(stmt) => {
                self.analyze_expr(&stmt.expr, scopes, context);
                for arm in &stmt.arms {
                    scopes.push();
                    self.bind_pattern(&arm.pattern, scopes);
                    match &arm.body {
                        MatchBody::Block(block) => self.analyze_block(block, scopes, context),
                        MatchBody::Expr(expr) => {
                            self.analyze_expr(expr, scopes, context);
                        }
                    }
                    scopes.pop();
                }
            }
            Stmt::For(stmt) => {
                self.analyze_expr(&stmt.iterable, scopes, context);
                if matches!(self.iterable_bound(&stmt.iterable, scopes), Some(false)) {
                    self.report_unbounded_iteration(stmt.iterable.span);
                }
                scopes.push();
                self.bind_pattern(&stmt.binding, scopes);
                self.analyze_block(&stmt.body, scopes, context);
                scopes.pop();
            }
            Stmt::Return(stmt) => {
                if context.item.kind == ItemKind::Task {
                    self.report_return_in_task(stmt.span);
                }
                if let Some(value) = &stmt.value {
                    self.analyze_expr(value, scopes, context);
                }
            }
            Stmt::Forever(stmt) => {
                self.report_legacy_task_statement(stmt.span, "`forever`", "`go` state cycles");
                if context.forever_depth > 0 {
                    self.report_nested_forever(stmt.span);
                }
                context.forever_depth += 1;
                self.analyze_block(&stmt.body, scopes, context);
                context.forever_depth -= 1;
            }
            Stmt::Exit(stmt) => {
                if !context.in_task_state {
                    self.report_task_statement_outside_task(stmt.span, "`exit`");
                }
                self.analyze_expr(&stmt.value, scopes, context);
            }
            Stmt::Delegate(stmt) => {
                self.report_legacy_task_statement(stmt.span, "`delegate`", "`go`");
                for arg in &stmt.args {
                    self.analyze_expr(arg, scopes, context);
                }
            }
            Stmt::Go(stmt) => {
                if !context.in_task_state {
                    self.report_task_statement_outside_task(stmt.span, "`go`");
                }
                for arg in &stmt.args {
                    self.analyze_expr(arg, scopes, context);
                }
            }
            Stmt::Observe(stmt) => {
                self.analyze_expr(&stmt.left, scopes, context);
                self.analyze_expr(&stmt.right, scopes, context);
                self.analyze_block(&stmt.else_block, scopes, context);
                if !is_terminal_block(&stmt.else_block, terminal_kind(context.item.kind)) {
                    self.report_non_terminal_observe_else(stmt.else_block.span);
                }
            }
            Stmt::UnsafeMarker(stmt) => {
                for arg in &stmt.construction.args {
                    self.analyze_expr(arg, scopes, context);
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
        context: &mut ControlContext<'_>,
    ) -> Option<Binding> {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Bool(_) => None,
            ExprKind::Name(name) => self.resolve_name(name.value.as_str(), name.span, scopes),
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.analyze_expr(element, scopes, context);
                }
                None
            }
            ExprKind::Block(block) => {
                self.analyze_block(block, scopes, context);
                None
            }
            ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
                self.analyze_expr(expr, scopes, context)
            }
            ExprKind::Binary { left, right, .. } => {
                self.analyze_expr(left, scopes, context);
                self.analyze_expr(right, scopes, context);
                None
            }
            ExprKind::Recover {
                expr,
                error_binding,
                fallback,
            } => {
                self.analyze_expr(expr, scopes, context);
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
                self.analyze_expr(fallback, scopes, context);
                scopes.pop();
                None
            }
            ExprKind::Call { callee, args } => {
                let callee_binding = self.analyze_expr(callee, scopes, context);
                if matches!(
                    callee_binding,
                    Some(Binding {
                        kind: BindingKind::Item,
                        span,
                        ..
                    }) if span == context.item.name_span
                ) {
                    self.report_direct_recursion(
                        context.item.name,
                        context.item.name_span,
                        callee.span,
                    );
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
                        caller_name: context.item.name.to_owned(),
                        callee_name,
                        callee_span: callee.span,
                        call_span: callee.span,
                    });
                }
                for arg in args {
                    self.analyze_expr(arg, scopes, context);
                }
                None
            }
            ExprKind::Index { target, index } => {
                self.analyze_expr(target, scopes, context);
                self.analyze_expr(index, scopes, context);
                None
            }
            ExprKind::MarkerRefinement { subject, marker } => {
                self.analyze_expr(subject, scopes, context);
                self.analyze_marker_annotation_args(marker, scopes);
                None
            }
            ExprKind::UnsafeMarker(construction) => {
                for arg in &construction.args {
                    self.analyze_expr(arg, scopes, context);
                }
                None
            }
        }
    }

    fn analyze_type_marker_args(&mut self, ty: &Type, scopes: &ScopeStack) {
        match &ty.kind {
            TypeKind::With { base, markers } => {
                self.analyze_type_marker_args(base, scopes);
                for marker in markers {
                    for arg in &marker.args {
                        self.analyze_marker_arg(arg, scopes);
                    }
                }
            }
            TypeKind::Tuple(elements) => {
                for element in elements {
                    self.analyze_type_marker_args(element, scopes);
                }
            }
            TypeKind::Array { element, .. } => self.analyze_type_marker_args(element, scopes),
            TypeKind::Applied { args, .. } => {
                for arg in args {
                    if let GenericArg::Type(ty) = arg {
                        self.analyze_type_marker_args(ty, scopes);
                    }
                }
            }
            TypeKind::Unit | TypeKind::Named(_) => {}
        }
    }

    fn analyze_marker_annotation_args(&mut self, marker: &MarkerAnnotation, scopes: &ScopeStack) {
        for arg in &marker.args {
            self.analyze_marker_arg(arg, scopes);
        }
    }

    fn analyze_marker_arg(&mut self, arg: &MarkerArg, scopes: &ScopeStack) {
        match &arg.kind {
            MarkerArgKind::Name(name) => {
                if let Some(binding) = self.resolve_name(name.value.as_str(), name.span, scopes) {
                    self.require_marker_place_binding(name.span, binding);
                }
            }
            MarkerArgKind::Field { base, .. } => {
                if let Some(binding) = self.resolve_name(base.value.as_str(), base.span, scopes) {
                    self.require_marker_place_binding(base.span, binding);
                }
            }
            MarkerArgKind::PatternBinding(_) | MarkerArgKind::Int(_) | MarkerArgKind::Bool(_) => {}
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

    fn report_reserved_host_builtin_name(&mut self, name: &Spanned<String>) {
        self.diagnostics.push(
            Diagnostic::error(format!("`{}` is reserved for a host builtin", name.value))
                .with_label(Label::primary(
                    name.span,
                    "host builtin names cannot be redefined",
                )),
        );
    }

    fn report_direct_recursion(
        &mut self,
        function_name: &str,
        function_span: Span,
        call_span: Span,
    ) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "direct recursion is not allowed for `{}`",
                function_name
            ))
            .with_label(Label::primary(call_span, "recursive call occurs here"))
            .with_label(Label::secondary(
                function_span,
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
            | ExprKind::Index { .. }
            | ExprKind::MarkerRefinement { .. }
            | ExprKind::UnsafeMarker(_) => Some(false),
            ExprKind::Binary { .. } => Some(false),
            ExprKind::Recover { .. } => Some(false),
        }
    }

    fn require_marker_place_binding(&mut self, span: Span, binding: Binding) {
        if matches!(binding.kind, BindingKind::Param | BindingKind::Local) {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error("marker arguments must name value places").with_label(
                Label::primary(span, "expected a parameter or local binding here"),
            ),
        );
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

    fn report_return_in_task(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("`return` is not allowed inside a task")
                .with_label(Label::primary(span, "use `exit` or `go` in task states")),
        );
    }

    fn report_task_statement_outside_task(&mut self, span: Span, keyword: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("{keyword} is only valid inside task states"))
                .with_label(Label::primary(span, "task-state orchestration statement")),
        );
    }

    fn report_nested_forever(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("nested `forever` loops are not allowed")
                .with_label(Label::primary(span, "nested `forever` starts here")),
        );
    }

    fn report_legacy_task_statement(&mut self, span: Span, statement: &str, replacement: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("{statement} is not part of target task states"))
                .with_label(Label::primary(span, format!("use {replacement} instead"))),
        );
    }

    fn report_duplicate_task_field(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate task field `{name}`"))
                .with_label(Label::primary(span, "field is declared again here"))
                .with_label(Label::secondary(previous, "first declaration is here")),
        );
    }

    fn report_duplicate_task_state(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate task state `{name}`"))
                .with_label(Label::primary(span, "state is declared again here"))
                .with_label(Label::secondary(previous, "first declaration is here")),
        );
    }

    fn report_state_param_collides_with_task_field(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("state parameters must not shadow task fields").with_label(
                Label::primary(span, "rename this state parameter or task field"),
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
}

struct TypeChecker<'a> {
    parsed: &'a ParsedModule,
    diagnostics: Vec<Diagnostic>,
    item_signatures: HashMap<String, ItemSignature>,
    host_signatures: HashMap<String, FunctionType>,
    marker_families: HashMap<String, MarkerFamilySignature>,
    task_state_stack: Vec<HashMap<String, TaskStateSignature>>,
    facts: TypeFacts,
}

impl<'a> TypeChecker<'a> {
    fn new(parsed: &'a ParsedModule) -> Self {
        let host_signatures: HashMap<String, FunctionType> = HostBuiltin::ALL
            .iter()
            .copied()
            .map(|builtin| (builtin.name().to_owned(), host_builtin_signature(builtin)))
            .collect();
        let item_signatures = parsed
            .module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some((
                    function.name.value.clone(),
                    ItemSignature {
                        kind: ItemKind::Function,
                        signature: function_signature(function),
                    },
                )),
                Item::Task(task) => Some((
                    task.name.value.clone(),
                    ItemSignature {
                        kind: ItemKind::Task,
                        signature: task_signature(task),
                    },
                )),
                Item::MarkerFamily(_) | Item::MarkerRule(_) => None,
            })
            .collect();
        let marker_families = parsed
            .module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::MarkerFamily(family) => Some((
                    family.name.value.clone(),
                    MarkerFamilySignature {
                        arity: family.params.len(),
                    },
                )),
                Item::Function(_) | Item::Task(_) | Item::MarkerRule(_) => None,
            })
            .collect();

        Self {
            parsed,
            diagnostics: Vec::new(),
            item_signatures,
            host_signatures,
            marker_families,
            task_state_stack: Vec::new(),
            facts: TypeFacts::default(),
        }
    }

    fn check_module(&mut self) {
        self.check_marker_family_declarations();
        self.check_marker_rule_declarations();
        for item in &self.parsed.module.items {
            match item {
                Item::Function(function) => self.check_function(function),
                Item::Task(task) => self.check_task(task),
                Item::MarkerFamily(family) => self.check_marker_family(family),
                Item::MarkerRule(rule) => self.check_marker_rule(rule),
            }
        }
    }

    fn check_marker_family_declarations(&mut self) {
        let mut seen = HashMap::new();
        for item in &self.parsed.module.items {
            let Item::MarkerFamily(family) = item else {
                continue;
            };
            if is_builtin_marker_family_name(&family.name.value) {
                self.report_builtin_marker_family_shadow(
                    family.name.span,
                    family.name.value.as_str(),
                );
            }
            if let Some(previous) = seen.insert(family.name.value.clone(), family.name.span) {
                self.report_duplicate_marker_family(family.name.span, previous, &family.name.value);
            }
        }
    }

    fn check_marker_family(&mut self, family: &MarkerFamily) {
        let mut seen_params = HashMap::new();
        for param in &family.params {
            if let Some(previous) = seen_params.insert(param.name.value.clone(), param.name.span) {
                self.report_duplicate_marker_family_param(
                    param.name.span,
                    previous,
                    &param.name.value,
                );
            }
        }
    }

    fn check_marker_rule_declarations(&mut self) {
        let mut seen = HashMap::new();
        for item in &self.parsed.module.items {
            let Item::MarkerRule(rule) = item else {
                continue;
            };
            if let Some(previous) = seen.insert(rule.name.value.clone(), rule.name.span) {
                self.report_duplicate_companion_rule(rule.name.span, previous, &rule.name.value);
            }
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
        for param in &function.params {
            self.check_marker_qualified_type(&param.ty, &scopes, true);
        }

        let expected_return = function
            .return_type
            .as_ref()
            .map(lower_type)
            .unwrap_or(SemanticType::Unit);
        if let Some(return_type) = &function.return_type {
            self.check_marker_qualified_type(return_type, &scopes, true);
        }
        let body_type = self.check_block(&function.body, &mut scopes, &expected_return);

        if let Some(expr) = &function.body.trailing_expr {
            self.require_same_type(expr.span, &expected_return, &body_type);
        } else if !is_terminal_block(&function.body, TerminalKind::Function) {
            self.require_same_type(function.body.span, &expected_return, &SemanticType::Unit);
        }

        scopes.pop();
    }

    fn check_task(&mut self, task: &Task) {
        let mut scopes = TypeScopeStack::default();
        scopes.push();
        for param in &task.params {
            let param_type = lower_type(&param.ty);
            self.facts
                .record_binding(param.name.span, param_type.clone());
            scopes.insert(param.name.value.clone(), param_type);
        }
        for param in &task.params {
            self.check_marker_qualified_type(&param.ty, &scopes, true);
        }

        let expected_return = lower_type(&task.return_type);
        self.check_marker_qualified_type(&task.return_type, &scopes, true);

        let mut field_names = HashMap::new();
        for field in &task.fields {
            if let Some(previous) = field_names.insert(field.name.value.clone(), field.name.span) {
                self.report_duplicate_task_field(field.name.span, previous, &field.name.value);
            }
            let field_type = lower_type(&field.ty);
            self.check_marker_qualified_type(&field.ty, &scopes, true);
            let value_type =
                self.check_expr_with_expected(&field.value, &mut scopes, Some(&field_type));
            self.require_same_type(field.value.span, &field_type, &value_type);
            self.facts
                .record_binding(field.name.span, field_type.clone());
            scopes.insert(field.name.value.clone(), field_type);
        }

        let mut state_signatures = HashMap::new();
        let mut start_count = 0usize;
        for state in &task.states {
            if state.name.value == "start" {
                start_count += 1;
            }
            let signature = TaskStateSignature {
                params: state
                    .params
                    .iter()
                    .map(|param| lower_type(&param.ty))
                    .collect(),
                span: state.name.span,
            };
            if let Some(previous) =
                state_signatures.insert(state.name.value.clone(), signature.clone())
            {
                self.report_duplicate_task_state(state.name.span, previous.span, &state.name.value);
            }
        }
        match start_count {
            0 => self.report_missing_start_state(task.body_span),
            1 => {}
            _ => self.report_duplicate_start_state(task.body_span),
        }

        if let Some(start) = state_signatures.get("start").cloned() {
            let task_params: Vec<_> = task
                .params
                .iter()
                .map(|param| lower_type(&param.ty))
                .collect();
            self.require_state_signature_matches_task(task.name.span, &task_params, &start.params);
        }

        self.task_state_stack.push(state_signatures);
        for state in &task.states {
            scopes.push();
            for param in &state.params {
                if field_names.contains_key(&param.name.value) {
                    self.report_state_param_collides_with_task_field(param.name.span);
                }
                let param_type = lower_type(&param.ty);
                self.facts
                    .record_binding(param.name.span, param_type.clone());
                scopes.insert(param.name.value.clone(), param_type);
            }
            for param in &state.params {
                self.check_marker_qualified_type(&param.ty, &scopes, true);
            }
            self.check_block(&state.body, &mut scopes, &expected_return);
            if !is_terminal_block(&state.body, TerminalKind::TaskState) {
                self.report_task_state_fallthrough(state.body.span);
            }
            scopes.pop();
        }
        self.task_state_stack.pop();

        scopes.pop();
    }

    fn check_marker_rule(&mut self, rule: &MarkerRule) {
        if !is_builtin_companion_rule(rule.name.value.as_str()) {
            self.report_unknown_companion_rule(rule.name.span, rule.name.value.as_str());
        }
        let expected_params = companion_rule_param_count(rule.name.value.as_str());
        if let Some(expected) = expected_params {
            if rule.params.len() != expected {
                self.report_companion_rule_param_count(
                    rule.name.span,
                    rule.name.value.as_str(),
                    expected,
                    rule.params.len(),
                );
            }
        }

        let mut scopes = MarkerRuleScopeStack::default();
        scopes.push();
        let mut seen_params = HashMap::new();
        for param in &rule.params {
            if let Some(previous) = seen_params.insert(param.name.value.clone(), param.name.span) {
                self.report_duplicate_marker_rule_param(
                    param.name.span,
                    previous,
                    &param.name.value,
                );
            }
            scopes.insert_place(param.name.value.clone());
        }
        self.check_marker_rule_block(&rule.body, &mut scopes);
        scopes.pop();
    }

    fn check_marker_rule_block(
        &mut self,
        block: &MarkerRuleBlock,
        scopes: &mut MarkerRuleScopeStack,
    ) {
        scopes.push();
        for statement in &block.statements {
            match statement {
                MarkerRuleStmt::If(stmt) => self.check_marker_rule_if(stmt, scopes),
                MarkerRuleStmt::Implies(stmt) => self.check_marker_implication(stmt, scopes),
            }
        }
        scopes.pop();
    }

    fn check_marker_rule_if(
        &mut self,
        stmt: &langlog_syntax::ast::MarkerRuleIfStmt,
        scopes: &mut MarkerRuleScopeStack,
    ) {
        self.require_marker_rule_place(
            stmt.refinement.subject.span,
            stmt.refinement.subject.value.as_str(),
            scopes,
        );
        let bindings = self.check_marker_refinement(&stmt.refinement, scopes);
        scopes.push();
        for binding in bindings {
            scopes.insert_place(binding.value);
        }
        self.check_marker_rule_block(&stmt.body, scopes);
        scopes.pop();
    }

    fn check_marker_implication(
        &mut self,
        stmt: &MarkerImplicationStmt,
        scopes: &MarkerRuleScopeStack,
    ) {
        self.check_marker_rule_annotation(&stmt.marker, scopes, MarkerRuleMarkerContext::Implied);
        self.require_marker_rule_place(stmt.target.span, stmt.target.value.as_str(), scopes);
    }

    fn check_marker_refinement(
        &mut self,
        refinement: &MarkerRefinement,
        scopes: &MarkerRuleScopeStack,
    ) -> Vec<Spanned<String>> {
        self.check_marker_rule_annotation(
            &refinement.marker,
            scopes,
            MarkerRuleMarkerContext::Refinement,
        );
        marker_pattern_bindings(&refinement.marker)
    }

    fn check_marker_rule_annotation(
        &mut self,
        marker: &MarkerAnnotation,
        scopes: &MarkerRuleScopeStack,
        context: MarkerRuleMarkerContext,
    ) {
        let Some(family) = self.resolve_marker_family(&marker.name.value) else {
            self.report_unknown_marker(marker.name.span, marker.name.value.as_str());
            return;
        };

        if !self.marker_rule_arity_is_valid(&family, marker.args.len()) {
            self.report_marker_arity(marker.name.span, marker.name.value.as_str());
        }

        for arg in &marker.args {
            match &arg.kind {
                MarkerArgKind::Name(name) => {
                    self.require_marker_rule_place(name.span, name.value.as_str(), scopes);
                }
                MarkerArgKind::PatternBinding(name) => {
                    if context != MarkerRuleMarkerContext::Refinement {
                        self.report_marker_pattern_binding_outside_refinement(name.span);
                    }
                }
                MarkerArgKind::Field { field, .. } => {
                    self.report_marker_rule_field_place(field.span);
                }
                MarkerArgKind::Int(_) | MarkerArgKind::Bool(_) => {}
            }
        }
    }

    fn marker_rule_arity_is_valid(&self, family: &HirMarkerFamily, arity: usize) -> bool {
        match family {
            HirMarkerFamily::Event => arity == 0,
            HirMarkerFamily::True | HirMarkerFamily::False => arity == 0,
            HirMarkerFamily::Equal
            | HirMarkerFamily::LessThan
            | HirMarkerFamily::GreaterThan
            | HirMarkerFamily::LessOrEqual
            | HirMarkerFamily::GreaterOrEqual
            | HirMarkerFamily::MemberOf => arity == 2,
            HirMarkerFamily::User(name) => self
                .marker_families
                .get(name)
                .is_some_and(|signature| arity == signature.arity),
        }
    }

    fn require_marker_rule_place(&mut self, span: Span, name: &str, scopes: &MarkerRuleScopeStack) {
        if scopes.contains(name) {
            return;
        }

        self.diagnostics.push(
            Diagnostic::error(format!("unknown marker rule place `{name}`"))
                .with_label(Label::primary(span, "place is not in scope here")),
        );
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
                if let Some(ty) = &stmt.ty {
                    self.check_marker_qualified_type(ty, scopes, true);
                }
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
            Stmt::Forever(stmt) => {
                self.report_legacy_task_statement(stmt.span, "`forever`", "`go` state cycles");
                self.check_block(&stmt.body, scopes, expected_return);
            }
            Stmt::Exit(stmt) => {
                let value_type =
                    self.check_expr_with_expected(&stmt.value, scopes, Some(expected_return));
                self.require_same_type(stmt.span, expected_return, &value_type);
            }
            Stmt::Delegate(stmt) => {
                self.report_legacy_task_statement(stmt.span, "`delegate`", "`go`");
                for arg in &stmt.args {
                    self.check_expr(arg, scopes);
                }
            }
            Stmt::Go(stmt) => {
                self.check_go_stmt(stmt, scopes);
            }
            Stmt::Observe(stmt) => {
                let left = self.check_expr(&stmt.left, scopes);
                let right = self.check_expr(&stmt.right, scopes);
                self.require_observe_types(stmt.span, stmt.op, &left, &right);
                self.check_block(&stmt.else_block, scopes, expected_return);
            }
            Stmt::UnsafeMarker(stmt) => {
                self.check_unsafe_marker_construction(
                    stmt.construction.marker.span,
                    stmt.construction.marker.value.as_str(),
                    &stmt.construction.args,
                    scopes,
                );
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
                    let item = self.item_signatures.get(name.value.as_str())?.clone();
                    if item.kind == ItemKind::Task {
                        self.report_task_item_used_as_expression(name.span);
                        Some(SemanticType::Unknown)
                    } else {
                        Some(SemanticType::Function(item.signature))
                    }
                })
                .or_else(|| {
                    self.host_signatures
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
            ExprKind::MarkerRefinement { subject, marker } => {
                self.check_expr(subject, scopes);
                self.check_marker_annotation(marker, scopes);
                self.report_runtime_marker_refinement(expr.span);
                SemanticType::Bool
            }
            ExprKind::UnsafeMarker(construction) => self.check_unsafe_marker_construction(
                construction.marker.span,
                construction.marker.value.as_str(),
                &construction.args,
                scopes,
            ),
            ExprKind::Grouped(expr) => self.check_expr(expr, scopes),
        };

        self.facts.record_expr(expr.span, ty)
    }

    fn check_marker_qualified_type(
        &mut self,
        ty: &Type,
        scopes: &TypeScopeStack,
        allow_marker_here: bool,
    ) {
        match &ty.kind {
            TypeKind::With { base, markers } => {
                if !allow_marker_here {
                    self.report_nested_marker_type(ty.span);
                }
                self.check_marker_qualified_type(base, scopes, false);
                for marker in markers {
                    self.check_marker_annotation(marker, scopes);
                }
            }
            TypeKind::Tuple(elements) => {
                for element in elements {
                    self.check_marker_qualified_type(element, scopes, false);
                }
            }
            TypeKind::Array { element, .. } => {
                self.check_marker_qualified_type(element, scopes, false);
            }
            TypeKind::Applied { args, .. } => {
                for arg in args {
                    if let GenericArg::Type(ty) = arg {
                        self.check_marker_qualified_type(ty, scopes, false);
                    }
                }
            }
            TypeKind::Unit | TypeKind::Named(_) => {}
        }
    }

    fn check_marker_annotation(&mut self, marker: &MarkerAnnotation, scopes: &TypeScopeStack) {
        let Some(family) = self.resolve_marker_family(&marker.name.value) else {
            self.report_unknown_marker(marker.name.span, marker.name.value.as_str());
            return;
        };

        if !self.marker_type_arity_is_valid(&family, marker.args.len()) {
            self.report_marker_arity(marker.name.span, marker.name.value.as_str());
        }

        for arg in &marker.args {
            self.check_marker_arg(arg, scopes);
        }
    }

    fn marker_type_arity_is_valid(&self, family: &HirMarkerFamily, arity: usize) -> bool {
        match family {
            HirMarkerFamily::Event => arity == 0,
            HirMarkerFamily::True | HirMarkerFamily::False => arity <= 1,
            HirMarkerFamily::Equal
            | HirMarkerFamily::LessThan
            | HirMarkerFamily::GreaterThan
            | HirMarkerFamily::LessOrEqual
            | HirMarkerFamily::GreaterOrEqual
            | HirMarkerFamily::MemberOf => (1..=2).contains(&arity),
            HirMarkerFamily::User(name) => {
                self.marker_families.get(name).is_some_and(|signature| {
                    arity == signature.arity
                        || (signature.arity > 0 && arity + 1 == signature.arity)
                })
            }
        }
    }

    fn check_marker_arg(&mut self, arg: &MarkerArg, scopes: &TypeScopeStack) {
        match &arg.kind {
            MarkerArgKind::Name(name) => {
                scopes.lookup(name.value.as_str());
            }
            MarkerArgKind::PatternBinding(name) => {
                self.report_marker_pattern_binding_outside_refinement(name.span);
            }
            MarkerArgKind::Field { base, field } => {
                if field.value != "length" {
                    self.report_unknown_marker_field(field.span, field.value.as_str());
                    return;
                }
                let Some(base_type) = scopes.lookup(base.value.as_str()) else {
                    return;
                };
                if !matches!(base_type, SemanticType::Array { .. }) {
                    self.report_marker_length_requires_array(base.span);
                }
            }
            MarkerArgKind::Int(_) | MarkerArgKind::Bool(_) => {}
        }
    }

    fn resolve_marker_family(&self, name: &str) -> Option<HirMarkerFamily> {
        lower_builtin_marker_family(name).or_else(|| {
            self.marker_families
                .contains_key(name)
                .then(|| HirMarkerFamily::User(name.to_owned()))
        })
    }

    fn check_unsafe_marker_construction(
        &mut self,
        marker_span: Span,
        marker_name: &str,
        args: &[Expr],
        scopes: &mut TypeScopeStack,
    ) -> SemanticType {
        let Some(family) = self.resolve_marker_family(marker_name) else {
            self.report_unknown_marker(marker_span, marker_name);
            for arg in args {
                self.check_expr(arg, scopes);
            }
            return SemanticType::Unknown;
        };

        let expected = self.unsafe_marker_arity(&family);
        if args.len() != expected {
            self.report_marker_constructor_arity(marker_span, marker_name, expected, args.len());
        }

        let mut arg_types = Vec::new();
        for arg in args {
            arg_types.push(self.check_expr(arg, scopes));
        }

        arg_types
            .into_iter()
            .next()
            .unwrap_or(SemanticType::Unknown)
    }

    fn unsafe_marker_arity(&self, family: &HirMarkerFamily) -> usize {
        match family {
            HirMarkerFamily::Event | HirMarkerFamily::True | HirMarkerFamily::False => 1,
            HirMarkerFamily::Equal
            | HirMarkerFamily::LessThan
            | HirMarkerFamily::GreaterThan
            | HirMarkerFamily::LessOrEqual
            | HirMarkerFamily::GreaterOrEqual
            | HirMarkerFamily::MemberOf => 2,
            HirMarkerFamily::User(name) => self
                .marker_families
                .get(name)
                .map(|signature| signature.arity.max(1))
                .unwrap_or(1),
        }
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
                self.record_builtin_callee_type(callee, vec![value], return_type.clone());
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
                self.record_builtin_callee_type(callee, Vec::new(), return_type.clone());
                return_type
            }
            HostBuiltin::Ok => {
                if !self.require_builtin_arity(callee.span, builtin, 1, args.len()) {
                    return SemanticType::Result {
                        ok: Box::new(SemanticType::Unknown),
                        err: Box::new(SemanticType::Unknown),
                    };
                }
                let (expected_ok, result_err) = match expected {
                    Some(SemanticType::Result { ok, err }) => {
                        (Some(ok.as_ref()), Some((**err).clone()))
                    }
                    _ => (None, None),
                };
                let value = self.check_expr_with_expected(&args[0], scopes, expected_ok);
                if let Some(expected_ok) = expected_ok {
                    self.require_same_type(args[0].span, expected_ok, &value);
                }
                let result_err = match result_err {
                    Some(err) => err,
                    None => {
                        self.report_cannot_infer_builtin(call_span, "ok");
                        SemanticType::Unknown
                    }
                };
                let return_type = SemanticType::Result {
                    ok: Box::new(value.clone()),
                    err: Box::new(result_err),
                };
                self.record_builtin_callee_type(callee, vec![value], return_type.clone());
                return_type
            }
            HostBuiltin::Err => {
                if !self.require_builtin_arity(callee.span, builtin, 1, args.len()) {
                    return SemanticType::Result {
                        ok: Box::new(SemanticType::Unknown),
                        err: Box::new(SemanticType::Unknown),
                    };
                }
                let (result_ok, expected_err) = match expected {
                    Some(SemanticType::Result { ok, err }) => {
                        (Some((**ok).clone()), Some(err.as_ref()))
                    }
                    _ => {
                        self.report_cannot_infer_builtin(call_span, "err");
                        (None, None)
                    }
                };
                let err_type = self.check_expr_with_expected(&args[0], scopes, expected_err);
                if let Some(expected_err) = expected_err {
                    self.require_same_type(args[0].span, expected_err, &err_type);
                }
                let return_type = SemanticType::Result {
                    ok: Box::new(result_ok.unwrap_or(SemanticType::Unknown)),
                    err: Box::new(err_type.clone()),
                };
                self.record_builtin_callee_type(callee, vec![err_type], return_type.clone());
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
                self.record_builtin_callee_type(callee, Vec::new(), return_type.clone());
                return_type
            }
            HostBuiltin::ReadU32
            | HostBuiltin::PrintU32
            | HostBuiltin::PrintBool
            | HostBuiltin::PrintNewline => {
                let signature = host_builtin_signature(builtin);
                self.record_builtin_callee_type(
                    callee,
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
        callee: &Expr,
        params: Vec<SemanticType>,
        return_type: SemanticType,
    ) {
        let ty = SemanticType::Function(FunctionType {
            params,
            return_type: Box::new(return_type),
        });
        self.facts.record_expr(callee.span, ty.clone());
        let resolved_name_span = name_span(callee);
        if resolved_name_span != callee.span {
            self.facts.record_expr(resolved_name_span, ty);
        }
    }

    fn require_builtin_arity(
        &mut self,
        callee_span: Span,
        _builtin: HostBuiltin,
        expected: usize,
        found: usize,
    ) -> bool {
        if expected == found {
            return true;
        }
        self.report_call_arity_mismatch(callee_span, expected, found);
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

    fn check_go_stmt(&mut self, stmt: &langlog_syntax::ast::GoStmt, scopes: &mut TypeScopeStack) {
        let Some(states) = self.task_state_stack.last() else {
            self.report_task_statement_outside_task(stmt.span, "`go`");
            for arg in &stmt.args {
                self.check_expr(arg, scopes);
            }
            return;
        };
        let Some(target) = states.get(stmt.target.value.as_str()).cloned() else {
            self.report_unknown_task_state(stmt.target.span, stmt.target.value.as_str());
            for arg in &stmt.args {
                self.check_expr(arg, scopes);
            }
            return;
        };

        if stmt.args.len() != target.params.len() {
            self.report_go_arity_mismatch(stmt.target.span, target.params.len(), stmt.args.len());
            for arg in &stmt.args {
                self.check_expr(arg, scopes);
            }
            return;
        }

        for (arg, expected) in stmt.args.iter().zip(target.params.iter()) {
            let found = self.check_expr_with_expected(arg, scopes, Some(expected));
            self.require_same_type(arg.span, expected, &found);
        }
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

    fn report_task_item_used_as_expression(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("task items cannot be used as expressions").with_label(
                Label::primary(span, "use `go` inside the task state graph instead"),
            ),
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

    fn report_task_statement_outside_task(&mut self, span: Span, statement: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("{statement} is only valid inside task states")).with_label(
                Label::primary(span, "move this statement into a `state` body"),
            ),
        );
    }

    fn report_task_state_fallthrough(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("task states must not fall through").with_label(Label::primary(
                span,
                "end every reachable path with `exit` or `go`",
            )),
        );
    }

    fn report_legacy_task_statement(&mut self, span: Span, statement: &str, replacement: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("{statement} is not part of target task states"))
                .with_label(Label::primary(span, format!("use {replacement} instead"))),
        );
    }

    fn report_duplicate_task_field(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate task field `{name}`"))
                .with_label(Label::primary(span, "field is declared again here"))
                .with_label(Label::secondary(previous, "first declaration is here")),
        );
    }

    fn report_duplicate_task_state(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate task state `{name}`"))
                .with_label(Label::primary(span, "state is declared again here"))
                .with_label(Label::secondary(previous, "first declaration is here")),
        );
    }

    fn report_missing_start_state(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("tasks must declare a `start` state")
                .with_label(Label::primary(span, "add `state start(...) { ... }` here")),
        );
    }

    fn report_duplicate_start_state(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("tasks must declare exactly one `start` state")
                .with_label(Label::primary(span, "`start` is declared more than once")),
        );
    }

    fn report_state_param_collides_with_task_field(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("state parameters must not shadow task fields").with_label(
                Label::primary(span, "rename this state parameter or task field"),
            ),
        );
    }

    fn report_unknown_task_state(&mut self, span: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("unknown task state `{name}`"))
                .with_label(Label::primary(span, "state is not declared in this task")),
        );
    }

    fn report_go_arity_mismatch(&mut self, span: Span, expected: usize, found: usize) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "go arity mismatch: expected {expected} argument(s), found {found}"
            ))
            .with_label(Label::primary(span, "adjust this state transition")),
        );
    }

    fn require_state_signature_matches_task(
        &mut self,
        span: Span,
        task_params: &[SemanticType],
        start_params: &[SemanticType],
    ) {
        if task_params.len() != start_params.len() {
            self.diagnostics.push(
                Diagnostic::error(format!(
                    "`start` state arity mismatch: expected {} parameter(s), found {}",
                    task_params.len(),
                    start_params.len()
                ))
                .with_label(Label::primary(span, "`start` must mirror task parameters")),
            );
            return;
        }

        for (index, (task_param, start_param)) in
            task_params.iter().zip(start_params.iter()).enumerate()
        {
            if task_param != start_param {
                self.diagnostics.push(
                    Diagnostic::error(format!(
                        "`start` state parameter {index} has type {}, expected {}",
                        start_param.describe(),
                        task_param.describe()
                    ))
                    .with_label(Label::primary(span, "`start` must mirror task parameters")),
                );
            }
        }
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

    fn report_nested_marker_type(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("marker-qualified types can only describe complete value places")
                .with_label(Label::primary(
                    span,
                    "move this marker qualifier to the outer value type",
                )),
        );
    }

    fn report_unknown_marker(&mut self, span: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("unknown marker `{name}`"))
                .with_label(Label::primary(span, "expected a known marker family here")),
        );
    }

    fn report_marker_arity(&mut self, span: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("wrong number of arguments for marker `{name}`")).with_label(
                Label::primary(span, "marker arguments do not match this family"),
            ),
        );
    }

    fn report_duplicate_marker_family(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate marker family `{name}`"))
                .with_label(Label::primary(
                    span,
                    "duplicate marker family declared here",
                ))
                .with_label(Label::secondary(
                    previous,
                    "first marker family declared here",
                )),
        );
    }

    fn report_builtin_marker_family_shadow(&mut self, span: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "marker family `{name}` conflicts with a builtin marker family"
            ))
            .with_label(Label::primary(
                span,
                "choose a marker family name that is not builtin",
            )),
        );
    }

    fn report_duplicate_marker_family_param(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate marker family parameter `{name}`"))
                .with_label(Label::primary(span, "duplicate parameter declared here"))
                .with_label(Label::secondary(previous, "first parameter declared here")),
        );
    }

    fn report_marker_constructor_arity(
        &mut self,
        span: Span,
        name: &str,
        expected: usize,
        found: usize,
    ) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "wrong number of arguments for unsafe marker `{name}`: expected {expected}, found {found}"
            ))
            .with_label(Label::primary(span, "marker constructor arity mismatch")),
        );
    }

    fn report_unknown_marker_field(&mut self, span: Span, field: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("unknown marker place field `{field}`"))
                .with_label(Label::primary(span, "only `.length` is supported here")),
        );
    }

    fn report_marker_length_requires_array(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("marker place field `.length` requires an array value")
                .with_label(Label::primary(span, "this place is not an array")),
        );
    }

    fn report_unknown_companion_rule(&mut self, span: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("unknown companion marker rule `{name}`")).with_label(
                Label::primary(
                    span,
                    "expected a builtin comparison or checked-arithmetic companion rule name here",
                ),
            ),
        );
    }

    fn report_duplicate_companion_rule(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate companion marker rule `{name}`"))
                .with_label(Label::primary(span, "duplicate marker rule declared here"))
                .with_label(Label::secondary(
                    previous,
                    "first marker rule declared here",
                )),
        );
    }

    fn report_companion_rule_param_count(
        &mut self,
        span: Span,
        name: &str,
        expected: usize,
        found: usize,
    ) {
        self.diagnostics.push(
            Diagnostic::error(format!(
                "wrong number of parameters for companion marker rule `{name}`: expected {expected}, found {found}"
            ))
            .with_label(Label::primary(span, "adjust this marker rule signature")),
        );
    }

    fn report_duplicate_marker_rule_param(&mut self, span: Span, previous: Span, name: &str) {
        self.diagnostics.push(
            Diagnostic::error(format!("duplicate marker rule parameter `{name}`"))
                .with_label(Label::primary(span, "duplicate parameter declared here"))
                .with_label(Label::secondary(previous, "first parameter declared here")),
        );
    }

    fn report_marker_pattern_binding_outside_refinement(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("marker-pattern bindings are only allowed in marker refinements")
                .with_label(Label::primary(
                    span,
                    "`?name` binds a place only in `with` patterns",
                )),
        );
    }

    fn report_marker_rule_field_place(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("marker rule places must be named rule places").with_label(
                Label::primary(span, "field places are not supported in marker rules"),
            ),
        );
    }

    fn report_runtime_marker_refinement(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("marker refinements are only allowed inside marker rules")
                .with_label(Label::primary(
                    span,
                    "this proof-only query has no runtime value",
                )),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerRuleMarkerContext {
    Refinement,
    Implied,
}

#[derive(Debug, Default)]
struct MarkerRuleScopeStack {
    scopes: Vec<HashSet<String>>,
}

impl MarkerRuleScopeStack {
    fn push(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn insert_place(&mut self, name: String) {
        self.scopes
            .last_mut()
            .expect("marker rule scope stack must not be empty")
            .insert(name);
    }

    fn contains(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
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

fn item_name_and_kind(item: &Item) -> Option<(&Spanned<String>, ItemKind)> {
    match item {
        Item::Function(function) => Some((&function.name, ItemKind::Function)),
        Item::Task(task) => Some((&task.name, ItemKind::Task)),
        Item::MarkerFamily(_) | Item::MarkerRule(_) => None,
    }
}

fn is_builtin_companion_rule(name: &str) -> bool {
    matches!(
        name,
        "Equal"
            | "LessThan"
            | "GreaterThan"
            | "LessOrEqual"
            | "GreaterOrEqual"
            | "Add"
            | "Sub"
            | "Mul"
            | "Div"
            | "Rem"
    )
}

fn companion_rule_param_count(name: &str) -> Option<usize> {
    is_builtin_companion_rule(name).then_some(3)
}

fn marker_pattern_bindings(marker: &MarkerAnnotation) -> Vec<Spanned<String>> {
    marker
        .args
        .iter()
        .filter_map(|arg| match &arg.kind {
            MarkerArgKind::PatternBinding(name) => Some(name.clone()),
            MarkerArgKind::Name(_)
            | MarkerArgKind::Field { .. }
            | MarkerArgKind::Int(_)
            | MarkerArgKind::Bool(_) => None,
        })
        .collect()
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

fn task_signature(task: &Task) -> FunctionType {
    FunctionType {
        params: task
            .params
            .iter()
            .map(|param| lower_type(&param.ty))
            .collect(),
        return_type: Box::new(lower_type(&task.return_type)),
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
        TypeKind::With { base, .. } => lower_type(base),
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
        langlog_syntax::ast::TypeKind::With { base, .. } => is_bounded_iterable_type(base),
        langlog_syntax::ast::TypeKind::Array { .. } => true,
        langlog_syntax::ast::TypeKind::Applied { base, .. } => {
            matches!(base.value.as_str(), "Set" | "Map")
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalKind {
    Function,
    TaskState,
}

fn terminal_kind(item_kind: ItemKind) -> TerminalKind {
    match item_kind {
        ItemKind::Function => TerminalKind::Function,
        ItemKind::Task => TerminalKind::TaskState,
    }
}

fn is_terminal_block(block: &Block, kind: TerminalKind) -> bool {
    block.trailing_expr.is_none()
        && block
            .statements
            .last()
            .is_some_and(|statement| is_terminal_statement(statement, kind))
}

fn is_terminal_statement(statement: &Stmt, kind: TerminalKind) -> bool {
    match statement {
        Stmt::Return(_) => kind == TerminalKind::Function,
        Stmt::Exit(_) | Stmt::Go(_) => kind == TerminalKind::TaskState,
        Stmt::If(stmt) => {
            is_terminal_block(&stmt.then_block, kind)
                && stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| is_terminal_else_branch(branch, kind))
        }
        Stmt::Match(stmt) => {
            !stmt.arms.is_empty()
                && stmt
                    .arms
                    .iter()
                    .all(|arm| is_terminal_match_body(&arm.body, kind))
        }
        _ => false,
    }
}

fn is_terminal_else_branch(branch: &ElseBranch, kind: TerminalKind) -> bool {
    match branch {
        ElseBranch::Block(block) => is_terminal_block(block, kind),
        ElseBranch::If(stmt) => {
            is_terminal_block(&stmt.then_block, kind)
                && stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| is_terminal_else_branch(branch, kind))
        }
    }
}

fn is_terminal_match_body(body: &MatchBody, kind: TerminalKind) -> bool {
    match body {
        MatchBody::Block(block) => is_terminal_block(block, kind),
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
