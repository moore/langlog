use std::collections::HashMap;

use langlog_sema::CheckedProgram;
use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, Item, MatchBody, ObserveOp, Stmt, Type,
    TypeKind,
};
use langlog_syntax::{Diagnostic, Label, Severity, Span, Spanned};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactSource {
    Observe,
    ControlFlow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFact {
    pub source: FactSource,
    pub origin_span: Span,
    pub left_span: Span,
    pub op: ObserveOp,
    pub right_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProof {
    pub obligations: usize,
    pub observations: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub facts: Vec<ProofFact>,
}

impl CheckedProof {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.severity, Severity::Error))
    }
}

pub fn check(program: &CheckedProgram) -> CheckedProof {
    let (obligations, diagnostics, facts) = {
        let mut checker = Checker::new(program);
        checker.check_module();
        (checker.obligations, checker.diagnostics, checker.facts)
    };

    CheckedProof {
        obligations,
        observations: facts.len(),
        diagnostics,
        facts,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnownFact {
    subject_name: String,
    op: ObserveOp,
    value: u64,
}

#[derive(Debug, Clone, Default)]
struct ArrayScopes {
    frames: Vec<HashMap<String, u64>>,
}

impl ArrayScopes {
    fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.frames.pop();
    }

    fn insert(&mut self, name: String, length: u64) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name, length);
        }
    }

    fn lookup(&self, name: &str) -> Option<u64> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| frame.get(name).copied())
    }
}

#[derive(Debug, Clone, Default)]
struct FlowState {
    facts: Vec<KnownFact>,
    arrays: ArrayScopes,
}

struct Checker<'a> {
    program: &'a CheckedProgram,
    obligations: usize,
    diagnostics: Vec<Diagnostic>,
    facts: Vec<ProofFact>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a CheckedProgram) -> Self {
        Self {
            program,
            obligations: 0,
            diagnostics: Vec::new(),
            facts: Vec::new(),
        }
    }

    fn check_module(&mut self) {
        for item in &self.program.parsed.module.items {
            let Item::Function(function) = item;
            self.check_function(function);
        }
    }

    fn check_function(&mut self, function: &Function) {
        let mut state = FlowState::default();
        state.arrays.push();
        for param in &function.params {
            if let Some(length) = array_length_from_type(&param.ty) {
                state.arrays.insert(param.name.value.clone(), length);
            }
        }
        self.check_block(&function.body, &mut state);
        state.arrays.pop();
    }

    fn check_block(&mut self, block: &Block, state: &mut FlowState) {
        let fact_len = state.facts.len();
        state.arrays.push();
        for statement in &block.statements {
            self.check_statement(statement, state);
        }
        if let Some(expr) = &block.trailing_expr {
            self.check_expr(expr, state);
        }
        state.arrays.pop();
        state.facts.truncate(fact_len);
    }

    fn check_statement(&mut self, statement: &Stmt, state: &mut FlowState) {
        match statement {
            Stmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.check_expr(value, state);
                }
                if let Some(length) =
                    stmt.ty
                        .as_ref()
                        .and_then(array_length_from_type)
                        .or_else(|| {
                            stmt.value
                                .as_ref()
                                .and_then(|expr| known_array_length(expr, state))
                        })
                {
                    state.arrays.insert(stmt.name.value.clone(), length);
                }
            }
            Stmt::Assign(stmt) => {
                self.check_expr(&stmt.target, state);
                self.check_expr(&stmt.value, state);
            }
            Stmt::Expr(stmt) => {
                self.check_expr(&stmt.expr, state);
            }
            Stmt::If(stmt) => {
                let branch_facts = self.record_control_flow_facts(&stmt.condition);
                self.check_expr(&stmt.condition, state);

                let fact_snapshot = state.facts.len();
                state.facts.extend(branch_facts);
                self.check_block(&stmt.then_block, state);
                state.facts.truncate(fact_snapshot);

                if let Some(else_branch) = &stmt.else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.check_block(block, state),
                        ElseBranch::If(stmt) => {
                            self.check_statement(&Stmt::If(*stmt.clone()), state);
                        }
                    }
                }
            }
            Stmt::Match(stmt) => {
                self.check_expr(&stmt.expr, state);
                for arm in &stmt.arms {
                    match &arm.body {
                        MatchBody::Block(block) => self.check_block(block, state),
                        MatchBody::Expr(expr) => self.check_expr(expr, state),
                    }
                }
            }
            Stmt::For(stmt) => {
                self.check_expr(&stmt.iterable, state);
                self.check_block(&stmt.body, state);
            }
            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.check_expr(value, state);
                }
            }
            Stmt::Observe(stmt) => {
                self.check_expr(&stmt.left, state);
                self.check_expr(&stmt.right, state);
                self.check_block(&stmt.else_block, state);
                self.facts.push(ProofFact {
                    source: FactSource::Observe,
                    origin_span: stmt.span,
                    left_span: stmt.left.span,
                    op: stmt.op,
                    right_span: stmt.right.span,
                });
                if let Some(fact) = known_fact(&stmt.left, stmt.op, &stmt.right) {
                    state.facts.push(fact);
                }
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr, state: &mut FlowState) {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Name(_) => {}
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.check_expr(element, state);
                }
            }
            ExprKind::Block(block) => {
                self.check_block(block, state);
            }
            ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
                self.check_expr(expr, state);
            }
            ExprKind::Binary { op, left, right } => {
                self.check_expr(left, state);
                self.check_expr(right, state);

                if matches!(op, BinaryOp::Div | BinaryOp::Rem) {
                    self.obligations += 1;
                    if !non_zero_is_proven(right, &state.facts) {
                        self.report_divide_by_zero(right.span);
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                self.check_expr(callee, state);
                for arg in args {
                    self.check_expr(arg, state);
                }
            }
            ExprKind::Index { target, index } => {
                self.check_expr(target, state);
                self.check_expr(index, state);

                self.obligations += 1;
                if !index_is_proven_in_bounds(target, index, state) {
                    self.report_out_of_bounds_index(index.span);
                }
            }
        }
    }

    fn record_control_flow_facts(&mut self, condition: &Expr) -> Vec<KnownFact> {
        match &condition.kind {
            ExprKind::Grouped(expr) => self.record_control_flow_facts(expr),
            ExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                let mut facts = self.record_control_flow_facts(left);
                facts.extend(self.record_control_flow_facts(right));
                facts
            }
            ExprKind::Binary { op, left, right } => {
                let Some(op) = comparison_to_observe_op(*op) else {
                    return Vec::new();
                };
                self.facts.push(ProofFact {
                    source: FactSource::ControlFlow,
                    origin_span: condition.span,
                    left_span: left.span,
                    op,
                    right_span: right.span,
                });

                known_fact(left, op, right)
                    .map(|fact| vec![fact])
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    fn report_divide_by_zero(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible divide-by-zero is not proven safe")
                .with_label(Label::primary(span, "prove this value is non-zero")),
        );
    }

    fn report_out_of_bounds_index(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible out-of-bounds indexing is not proven safe")
                .with_label(Label::primary(span, "prove this index stays within bounds")),
        );
    }
}

fn comparison_to_observe_op(op: BinaryOp) -> Option<ObserveOp> {
    match op {
        BinaryOp::EqEq => Some(ObserveOp::Eq),
        BinaryOp::NotEq => Some(ObserveOp::NotEq),
        BinaryOp::Lt => Some(ObserveOp::Lt),
        BinaryOp::LtEq => Some(ObserveOp::LtEq),
        BinaryOp::Gt => Some(ObserveOp::Gt),
        BinaryOp::GtEq => Some(ObserveOp::GtEq),
        _ => None,
    }
}

fn bare_name(expr: &Expr) -> Option<&Spanned<String>> {
    match &expr.kind {
        ExprKind::Name(name) => Some(name),
        ExprKind::Grouped(expr) => bare_name(expr),
        _ => None,
    }
}

fn known_fact(left: &Expr, op: ObserveOp, right: &Expr) -> Option<KnownFact> {
    let subject = bare_name(left)?;
    let value = eval_const_u64(right)?;
    Some(KnownFact {
        subject_name: subject.value.clone(),
        op,
        value,
    })
}

fn eval_const_u64(expr: &Expr) -> Option<u64> {
    match &expr.kind {
        ExprKind::Int(value) => Some(*value),
        ExprKind::Grouped(expr) => eval_const_u64(expr),
        ExprKind::Unary { .. }
        | ExprKind::Bool(_)
        | ExprKind::Name(_)
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::Block(_)
        | ExprKind::Call { .. }
        | ExprKind::Index { .. } => None,
        ExprKind::Binary { op, left, right } => {
            let left = eval_const_u64(left)?;
            let right = eval_const_u64(right)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div => (right != 0).then(|| left / right),
                BinaryOp::Rem => (right != 0).then(|| left % right),
                _ => None,
            }
        }
    }
}

fn non_zero_is_proven(expr: &Expr, facts: &[KnownFact]) -> bool {
    if let Some(value) = eval_const_u64(expr) {
        return value != 0;
    }

    let Some(name) = bare_name(expr) else {
        return false;
    };

    facts.iter().rev().any(|fact| {
        fact.subject_name == name.value
            && match fact.op {
                ObserveOp::Eq => fact.value != 0,
                ObserveOp::NotEq => fact.value == 0,
                ObserveOp::Gt => true,
                ObserveOp::GtEq => fact.value > 0,
                ObserveOp::Lt | ObserveOp::LtEq => false,
            }
    })
}

fn array_length_from_type(ty: &Type) -> Option<u64> {
    match &ty.kind {
        TypeKind::Array { length, .. } => Some(length.value),
        _ => None,
    }
}

fn known_array_length(expr: &Expr, state: &FlowState) -> Option<u64> {
    match &expr.kind {
        ExprKind::Array(elements) => Some(elements.len() as u64),
        ExprKind::Name(name) => state.arrays.lookup(&name.value),
        ExprKind::Grouped(expr) => known_array_length(expr, state),
        _ => None,
    }
}

fn index_is_proven_in_bounds(target: &Expr, index: &Expr, state: &FlowState) -> bool {
    let Some(length) = known_array_length(target, state) else {
        return false;
    };

    if let Some(value) = eval_const_u64(index) {
        return value < length;
    }

    let Some(name) = bare_name(index) else {
        return false;
    };

    state.facts.iter().rev().any(|fact| {
        fact.subject_name == name.value
            && match fact.op {
                ObserveOp::Eq => fact.value < length,
                ObserveOp::Lt => fact.value <= length,
                ObserveOp::LtEq => fact.value < length,
                ObserveOp::NotEq | ObserveOp::Gt | ObserveOp::GtEq => false,
            }
    })
}
