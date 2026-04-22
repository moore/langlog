use std::collections::{HashMap, HashSet};

use langlog_sema::CheckedProgram;
use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, Item, MatchBody, ObserveOp, Stmt, Type,
    TypeKind,
};
use langlog_syntax::{Diagnostic, Label, Severity, Span, Spanned};

const U32_MAX_U64: u64 = u32::MAX as u64;

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

    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.severity, Severity::Warning))
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
    subject: BindingId,
    op: ObserveOp,
    value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BindingId {
    declaration_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedFact {
    fact: KnownFact,
    binding_span: Span,
    mutable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MutableControlFlowHint {
    fact: KnownFact,
    origin_span: Span,
    binding_span: Span,
}

#[derive(Debug, Clone, Default)]
struct RecordedControlFlow {
    stable_facts: Vec<KnownFact>,
    mutable_hints: Vec<MutableControlFlowHint>,
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
    stable_facts: Vec<KnownFact>,
    mutable_hints: Vec<MutableControlFlowHint>,
    arrays: ArrayScopes,
}

struct Checker<'a> {
    program: &'a CheckedProgram,
    obligations: usize,
    diagnostics: Vec<Diagnostic>,
    facts: Vec<ProofFact>,
    warned_obligations: HashSet<(Span, Span)>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a CheckedProgram) -> Self {
        Self {
            program,
            obligations: 0,
            diagnostics: Vec::new(),
            facts: Vec::new(),
            warned_obligations: HashSet::new(),
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
        let stable_fact_len = state.stable_facts.len();
        let hint_len = state.mutable_hints.len();
        state.arrays.push();
        for statement in &block.statements {
            self.check_statement(statement, state);
        }
        if let Some(expr) = &block.trailing_expr {
            self.check_expr(expr, state);
        }
        state.arrays.pop();
        state.stable_facts.truncate(stable_fact_len);
        state.mutable_hints.truncate(hint_len);
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
                                .and_then(|expr| known_array_length(expr, &state.arrays))
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

                let stable_snapshot = state.stable_facts.len();
                let hint_snapshot = state.mutable_hints.len();
                state.stable_facts.extend(branch_facts.stable_facts);
                state.mutable_hints.extend(branch_facts.mutable_hints);
                self.check_block(&stmt.then_block, state);
                state.stable_facts.truncate(stable_snapshot);
                state.mutable_hints.truncate(hint_snapshot);

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
                if let Some(fact) = self.known_fact(&stmt.left, stmt.op, &stmt.right) {
                    if !fact.mutable {
                        state.stable_facts.push(fact.fact);
                    }
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

                match op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                        self.obligations += 1;
                        if !arithmetic_is_proven_safe(
                            self.program,
                            *op,
                            left,
                            right,
                            &state.stable_facts,
                        ) {
                            self.report_mutable_hint_warning_if_needed(expr.span, state, |facts| {
                                arithmetic_is_proven_safe(self.program, *op, left, right, facts)
                            });
                            self.report_arithmetic_overflow(expr.span);
                        }
                    }
                    BinaryOp::Div | BinaryOp::Rem => {
                        self.obligations += 1;
                        if !non_zero_is_proven(self.program, right, &state.stable_facts) {
                            self.report_mutable_hint_warning_if_needed(expr.span, state, |facts| {
                                non_zero_is_proven(self.program, right, facts)
                            });
                            self.report_divide_by_zero(right.span);
                        }
                    }
                    _ => {}
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
                if !index_is_proven_in_bounds(
                    self.program,
                    target,
                    index,
                    &state.arrays,
                    &state.stable_facts,
                ) {
                    self.report_mutable_hint_warning_if_needed(index.span, state, |facts| {
                        index_is_proven_in_bounds(self.program, target, index, &state.arrays, facts)
                    });
                    self.report_out_of_bounds_index(index.span);
                }
            }
        }
    }

    fn record_control_flow_facts(&mut self, condition: &Expr) -> RecordedControlFlow {
        match &condition.kind {
            ExprKind::Grouped(expr) => self.record_control_flow_facts(expr),
            ExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                let mut facts = self.record_control_flow_facts(left);
                let right_facts = self.record_control_flow_facts(right);
                facts.stable_facts.extend(right_facts.stable_facts);
                facts.mutable_hints.extend(right_facts.mutable_hints);
                facts
            }
            ExprKind::Binary { op, left, right } => {
                let Some(op) = comparison_to_observe_op(*op) else {
                    return RecordedControlFlow::default();
                };
                let proof_fact = ProofFact {
                    source: FactSource::ControlFlow,
                    origin_span: condition.span,
                    left_span: left.span,
                    op,
                    right_span: right.span,
                };
                self.facts.push(proof_fact.clone());

                let Some(fact) = self.known_fact(left, op, right) else {
                    return RecordedControlFlow::default();
                };

                if fact.mutable {
                    RecordedControlFlow {
                        stable_facts: Vec::new(),
                        mutable_hints: vec![MutableControlFlowHint {
                            fact: fact.fact,
                            origin_span: proof_fact.origin_span,
                            binding_span: fact.binding_span,
                        }],
                    }
                } else {
                    RecordedControlFlow {
                        stable_facts: vec![fact.fact],
                        mutable_hints: Vec::new(),
                    }
                }
            }
            _ => RecordedControlFlow::default(),
        }
    }

    fn known_fact(&self, left: &Expr, op: ObserveOp, right: &Expr) -> Option<ResolvedFact> {
        let subject = bare_name(left)?;
        let resolution = self.program.resolution(subject.span)?;
        let value = eval_const_u64(right)?;
        Some(ResolvedFact {
            fact: KnownFact {
                subject: BindingId {
                    declaration_span: resolution.declaration_span,
                },
                op,
                value,
            },
            binding_span: resolution.declaration_span,
            mutable: resolution.mutable,
        })
    }

    fn report_mutable_hint_warning_if_needed<F>(
        &mut self,
        obligation_span: Span,
        state: &FlowState,
        is_proven: F,
    ) where
        F: Fn(&[KnownFact]) -> bool,
    {
        if state.mutable_hints.is_empty() {
            return;
        }

        let combined_facts = stable_and_mutable_facts(state, None);
        if !is_proven(&combined_facts) {
            return;
        }

        for (index, hint) in state.mutable_hints.iter().enumerate() {
            let facts_without_hint = stable_and_mutable_facts(state, Some(index));
            if !is_proven(&facts_without_hint)
                && self
                    .warned_obligations
                    .insert((hint.origin_span, obligation_span))
            {
                self.report_mutable_control_flow_hint(hint, obligation_span);
            }
        }
    }

    fn report_divide_by_zero(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible divide-by-zero is not proven safe")
                .with_label(Label::primary(span, "prove this value is non-zero")),
        );
    }

    fn report_arithmetic_overflow(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible arithmetic overflow is not proven safe").with_label(
                Label::primary(span, "prove this operation stays within u32 bounds"),
            ),
        );
    }

    fn report_out_of_bounds_index(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible out-of-bounds indexing is not proven safe")
                .with_label(Label::primary(span, "prove this index stays within bounds")),
        );
    }

    fn report_mutable_control_flow_hint(
        &mut self,
        hint: &MutableControlFlowHint,
        obligation_span: Span,
    ) {
        self.diagnostics.push(
            Diagnostic::warning(
                "mutable control-flow comparison cannot discharge this proof obligation",
            )
            .with_label(Label::primary(
                hint.origin_span,
                "comparison was observed here",
            ))
            .with_label(Label::secondary(
                hint.binding_span,
                "this binding is declared `mut`",
            ))
            .with_label(Label::secondary(
                obligation_span,
                "this operation still needs a stable proof",
            )),
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

fn stable_and_mutable_facts(state: &FlowState, skip_hint: Option<usize>) -> Vec<KnownFact> {
    let mut facts = state.stable_facts.clone();
    for (index, hint) in state.mutable_hints.iter().enumerate() {
        if Some(index) != skip_hint {
            facts.push(hint.fact.clone());
        }
    }
    facts
}

fn arithmetic_is_proven_safe(
    program: &CheckedProgram,
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    facts: &[KnownFact],
) -> bool {
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
            proven_binary_u32_range(program, op, left, right, facts).is_some()
        }
        _ => true,
    }
}

fn proven_binary_u32_range(
    program: &CheckedProgram,
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    facts: &[KnownFact],
) -> Option<(u64, u64)> {
    let (left_low, left_high) = proven_u32_range(program, left, facts)?;
    let (right_low, right_high) = proven_u32_range(program, right, facts)?;

    match op {
        BinaryOp::Add => {
            let low = left_low.checked_add(right_low)?;
            let high = left_high.checked_add(right_high)?;
            (high <= U32_MAX_U64).then_some((low, high))
        }
        BinaryOp::Sub => {
            if left_low < right_high {
                return None;
            }
            Some((left_low - right_high, left_high - right_low))
        }
        BinaryOp::Mul => {
            let low = left_low.checked_mul(right_low)?;
            let high = left_high.checked_mul(right_high)?;
            (high <= U32_MAX_U64).then_some((low, high))
        }
        _ => None,
    }
}

fn proven_u32_range(
    program: &CheckedProgram,
    expr: &Expr,
    facts: &[KnownFact],
) -> Option<(u64, u64)> {
    match &expr.kind {
        ExprKind::Int(value) => (*value <= U32_MAX_U64).then_some((*value, *value)),
        ExprKind::Name(_) => bounds_for_binding(binding_id(program, expr)?, facts),
        ExprKind::Grouped(expr) => proven_u32_range(program, expr, facts),
        ExprKind::Binary { op, left, right } => {
            proven_binary_u32_range(program, *op, left, right, facts)
        }
        ExprKind::Unary { .. }
        | ExprKind::Bool(_)
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::Block(_)
        | ExprKind::Call { .. }
        | ExprKind::Index { .. } => None,
    }
}

fn binding_id(program: &CheckedProgram, expr: &Expr) -> Option<BindingId> {
    let name = bare_name(expr)?;
    let resolution = program.resolution(name.span)?;
    Some(BindingId {
        declaration_span: resolution.declaration_span,
    })
}

fn bounds_for_binding(subject: BindingId, facts: &[KnownFact]) -> Option<(u64, u64)> {
    let mut lower = 0;
    let mut upper = U32_MAX_U64;

    for fact in facts.iter().filter(|fact| fact.subject == subject) {
        match fact.op {
            ObserveOp::Eq => {
                lower = lower.max(fact.value);
                upper = upper.min(fact.value);
            }
            ObserveOp::NotEq => {}
            ObserveOp::Lt => {
                upper = upper.min(fact.value.saturating_sub(1));
            }
            ObserveOp::LtEq => {
                upper = upper.min(fact.value);
            }
            ObserveOp::Gt => {
                lower = lower.max(fact.value.saturating_add(1));
            }
            ObserveOp::GtEq => {
                lower = lower.max(fact.value);
            }
        }
    }

    (lower <= upper).then_some((lower, upper))
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

fn non_zero_is_proven(program: &CheckedProgram, expr: &Expr, facts: &[KnownFact]) -> bool {
    if let Some(value) = eval_const_u64(expr) {
        return value != 0;
    }

    let Some(subject) = binding_id(program, expr) else {
        return false;
    };

    facts.iter().rev().any(|fact| {
        fact.subject == subject
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

fn known_array_length(expr: &Expr, arrays: &ArrayScopes) -> Option<u64> {
    match &expr.kind {
        ExprKind::Array(elements) => Some(elements.len() as u64),
        ExprKind::Name(name) => arrays.lookup(&name.value),
        ExprKind::Grouped(expr) => known_array_length(expr, arrays),
        _ => None,
    }
}

fn index_is_proven_in_bounds(
    program: &CheckedProgram,
    target: &Expr,
    index: &Expr,
    arrays: &ArrayScopes,
    facts: &[KnownFact],
) -> bool {
    let Some(length) = known_array_length(target, arrays) else {
        return false;
    };

    if let Some(value) = eval_const_u64(index) {
        return value < length;
    }

    proven_u32_range(program, index, facts)
        .map(|(_, upper)| upper < length)
        .unwrap_or(false)
}
