use std::collections::{HashMap, HashSet};

use langlog_sema::{
    CheckedProgram, HirBinding, HirBindingId, HirBlock, HirElseBranch, HirExpr, HirExprKind,
    HirFunction, HirMatchBody, HirPattern, HirPatternKind, HirProgram, HirStmt, HirType,
};
use langlog_syntax::ast::{BinaryOp, ObserveOp};
use langlog_syntax::{Diagnostic, Label, Severity, Span};

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
    pub proof_ir: Option<ProofProgram>,
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
    let Some(hir) = program.hir.as_ref() else {
        return CheckedProof {
            obligations: 0,
            observations: 0,
            diagnostics: Vec::new(),
            facts: Vec::new(),
            proof_ir: None,
        };
    };

    let proof_ir = lower_to_proof_ir(hir);
    let (obligations, diagnostics, facts) = {
        let mut checker = Checker::new(hir);
        checker.check_module();
        (checker.obligations, checker.diagnostics, checker.facts)
    };

    CheckedProof {
        obligations,
        observations: facts.len(),
        diagnostics,
        facts,
        proof_ir: Some(proof_ir),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofProgram {
    pub functions: Vec<ProofFunction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFunction {
    pub body: ProofBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofBlock {
    pub entries: Vec<ProofEntry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofEntry {
    Branch {
        condition: ProofExpr,
        facts: Vec<ProofRelation>,
        mutable_hints: Vec<ProofRelation>,
        then_block: ProofBlock,
        else_block: Option<ProofBlock>,
        span: Span,
    },
    Observe {
        left: ProofExpr,
        op: ObserveOp,
        right: ProofExpr,
        fact: ProofRelation,
        else_block: ProofBlock,
        span: Span,
    },
    For {
        iterable: ProofExpr,
        membership: Option<ProofSetMembership>,
        body: ProofBlock,
        span: Span,
    },
    Obligation {
        kind: ProofObligationKind,
        span: Span,
    },
    Eval {
        expr: ProofExpr,
        span: Span,
    },
    Scope {
        block: ProofBlock,
        span: Span,
    },
}

impl ProofEntry {
    pub fn span(&self) -> Span {
        match self {
            Self::Branch { span, .. }
            | Self::Observe { span, .. }
            | Self::For { span, .. }
            | Self::Obligation { span, .. }
            | Self::Eval { span, .. }
            | Self::Scope { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofObligationKind {
    InBounds {
        target: ProofExpr,
        index: ProofExpr,
        length: u64,
    },
    MapPresence {
        target: ProofExpr,
        key: ProofExpr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofRelation {
    pub source: FactSource,
    pub origin_span: Span,
    pub left_span: Span,
    pub subject: Option<HirBindingId>,
    pub op: ObserveOp,
    pub right: ProofExpr,
    pub right_span: Span,
    pub stable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSetMembership {
    pub member: HirBindingId,
    pub element_type: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofExpr {
    pub kind: ProofExprKind,
    pub ty: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofExprKind {
    Binding(HirBindingId),
    Item,
    HostBuiltin,
    Int(u64),
    Bool(bool),
    Tuple(Vec<ProofExpr>),
    Array(Vec<ProofExpr>),
    Unary {
        expr: Box<ProofExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<ProofExpr>,
        right: Box<ProofExpr>,
    },
    Call {
        callee: Box<ProofExpr>,
        args: Vec<ProofExpr>,
    },
    Index {
        target: Box<ProofExpr>,
        index: Box<ProofExpr>,
    },
}

pub fn lower_to_proof_ir(hir: &HirProgram) -> ProofProgram {
    ProofLowerer::new(collect_bindings(hir)).lower_program(hir)
}

struct ProofLowerer {
    bindings: HashMap<HirBindingId, BindingInfo>,
}

impl ProofLowerer {
    fn new(bindings: HashMap<HirBindingId, BindingInfo>) -> Self {
        Self { bindings }
    }

    fn lower_program(&self, hir: &HirProgram) -> ProofProgram {
        ProofProgram {
            functions: hir
                .functions
                .iter()
                .map(|function| ProofFunction {
                    body: self.lower_block(&function.body),
                    span: function.span,
                })
                .collect(),
        }
    }

    fn lower_block(&self, block: &HirBlock) -> ProofBlock {
        let mut entries = Vec::new();
        for statement in &block.statements {
            self.lower_statement(statement, &mut entries);
        }
        if let Some(result) = &block.result {
            if let Some(expr) = self.lower_expr(result, &mut entries) {
                entries.push(ProofEntry::Eval {
                    span: expr.span,
                    expr,
                });
            }
        }
        ProofBlock {
            entries,
            span: block.span,
        }
    }

    fn lower_statement(&self, statement: &HirStmt, entries: &mut Vec<ProofEntry>) {
        match statement {
            HirStmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.lower_expr(value, entries);
                }
            }
            HirStmt::Assign(stmt) => {
                self.lower_expr(&stmt.target, entries);
                self.lower_expr(&stmt.value, entries);
            }
            HirStmt::Expr(stmt) => {
                if let Some(expr) = self.lower_expr(&stmt.expr, entries) {
                    entries.push(ProofEntry::Eval {
                        span: stmt.span,
                        expr,
                    });
                }
            }
            HirStmt::If(stmt) => {
                let Some(condition) = self.lower_expr(&stmt.condition, entries) else {
                    return;
                };
                let (facts, mutable_hints) = self.control_flow_relations(&stmt.condition);
                entries.push(ProofEntry::Branch {
                    condition,
                    facts,
                    mutable_hints,
                    then_block: self.lower_block(&stmt.then_block),
                    else_block: stmt
                        .else_branch
                        .as_ref()
                        .map(|branch| self.lower_else_branch(branch)),
                    span: stmt.span,
                });
            }
            HirStmt::Match(stmt) => {
                self.lower_expr(&stmt.expr, entries);
                for arm in &stmt.arms {
                    match &arm.body {
                        HirMatchBody::Block(block) => entries.push(ProofEntry::Scope {
                            block: self.lower_block(block),
                            span: arm.span,
                        }),
                        HirMatchBody::Expr(expr) => {
                            if let Some(expr) = self.lower_expr(expr, entries) {
                                entries.push(ProofEntry::Eval {
                                    span: arm.span,
                                    expr,
                                });
                            }
                        }
                    }
                }
            }
            HirStmt::For(stmt) => {
                let Some(iterable) = self.lower_expr(&stmt.iterable, entries) else {
                    return;
                };
                entries.push(ProofEntry::For {
                    membership: proof_set_membership_for_loop(stmt),
                    body: self.lower_block(&stmt.body),
                    span: stmt.span,
                    iterable,
                });
            }
            HirStmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.lower_expr(value, entries);
                }
            }
            HirStmt::Observe(stmt) => {
                let (Some(left), Some(right)) = (
                    self.lower_expr(&stmt.left, entries),
                    self.lower_expr(&stmt.right, entries),
                ) else {
                    return;
                };
                let fact = self.relation(
                    FactSource::Observe,
                    stmt.span,
                    &stmt.left,
                    stmt.op,
                    &stmt.right,
                );
                entries.push(ProofEntry::Observe {
                    left,
                    op: stmt.op,
                    right,
                    fact,
                    else_block: self.lower_block(&stmt.else_block),
                    span: stmt.span,
                });
            }
        }
    }

    fn lower_else_branch(&self, branch: &HirElseBranch) -> ProofBlock {
        match branch {
            HirElseBranch::Block(block) => self.lower_block(block),
            HirElseBranch::If(stmt) => {
                let mut entries = Vec::new();
                self.lower_statement(&HirStmt::If(*stmt.clone()), &mut entries);
                ProofBlock {
                    entries,
                    span: stmt.span,
                }
            }
        }
    }

    fn lower_expr(&self, expr: &HirExpr, entries: &mut Vec<ProofEntry>) -> Option<ProofExpr> {
        let kind = match &expr.kind {
            HirExprKind::Binding(id) => ProofExprKind::Binding(*id),
            HirExprKind::Item(_) => ProofExprKind::Item,
            HirExprKind::HostBuiltin(_) => ProofExprKind::HostBuiltin,
            HirExprKind::Int(value) => ProofExprKind::Int(*value),
            HirExprKind::Bool(value) => ProofExprKind::Bool(*value),
            HirExprKind::Tuple(elements) => ProofExprKind::Tuple(
                elements
                    .iter()
                    .filter_map(|element| self.lower_expr(element, entries))
                    .collect(),
            ),
            HirExprKind::Array(elements) => ProofExprKind::Array(
                elements
                    .iter()
                    .filter_map(|element| self.lower_expr(element, entries))
                    .collect(),
            ),
            HirExprKind::Block(block) => {
                entries.push(ProofEntry::Scope {
                    block: self.lower_block(block),
                    span: block.span,
                });
                return None;
            }
            HirExprKind::Unary { expr, .. } => ProofExprKind::Unary {
                expr: Box::new(self.lower_expr(expr, entries)?),
            },
            HirExprKind::Binary { op, left, right } => ProofExprKind::Binary {
                op: *op,
                left: Box::new(self.lower_expr(left, entries)?),
                right: Box::new(self.lower_expr(right, entries)?),
            },
            HirExprKind::Recover { expr, fallback, .. } => {
                self.lower_expr(expr, entries);
                self.lower_expr(fallback, entries);
                return None;
            }
            HirExprKind::Call { callee, args } => ProofExprKind::Call {
                callee: Box::new(self.lower_expr(callee, entries)?),
                args: args
                    .iter()
                    .filter_map(|arg| self.lower_expr(arg, entries))
                    .collect(),
            },
            HirExprKind::Index { target, index } => {
                let target = self.lower_expr(target, entries)?;
                let index = self.lower_expr(index, entries)?;
                if let Some(kind) = proof_obligation_for_index(&target, &index) {
                    entries.push(ProofEntry::Obligation {
                        span: index.span,
                        kind,
                    });
                }
                ProofExprKind::Index {
                    target: Box::new(target),
                    index: Box::new(index),
                }
            }
        };

        Some(ProofExpr {
            kind,
            ty: expr.ty.clone(),
            span: expr.span,
        })
    }

    fn control_flow_relations(
        &self,
        condition: &HirExpr,
    ) -> (Vec<ProofRelation>, Vec<ProofRelation>) {
        match &condition.kind {
            HirExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                let (mut facts, mut hints) = self.control_flow_relations(left);
                let (right_facts, right_hints) = self.control_flow_relations(right);
                facts.extend(right_facts);
                hints.extend(right_hints);
                (facts, hints)
            }
            HirExprKind::Binary { op, left, right } => {
                let Some(op) = comparison_to_observe_op(*op) else {
                    return (Vec::new(), Vec::new());
                };
                let relation =
                    self.relation(FactSource::ControlFlow, condition.span, left, op, right);
                if relation.stable {
                    (vec![relation], Vec::new())
                } else {
                    (Vec::new(), vec![relation])
                }
            }
            _ => (Vec::new(), Vec::new()),
        }
    }

    fn relation(
        &self,
        source: FactSource,
        origin_span: Span,
        left: &HirExpr,
        op: ObserveOp,
        right: &HirExpr,
    ) -> ProofRelation {
        let subject = binding_id(left);
        let stable = subject
            .and_then(|subject| self.bindings.get(&subject))
            .map(|binding| !binding.mutable)
            .unwrap_or(true);
        let mut nested_entries = Vec::new();
        let right = self
            .lower_expr(right, &mut nested_entries)
            .unwrap_or_else(|| ProofExpr {
                kind: ProofExprKind::Tuple(Vec::new()),
                ty: right.ty.clone(),
                span: right.span,
            });
        ProofRelation {
            source,
            origin_span,
            left_span: left.span,
            subject,
            op,
            right_span: right.span,
            right,
            stable,
        }
    }
}

fn proof_obligation_for_index(
    target: &ProofExpr,
    index: &ProofExpr,
) -> Option<ProofObligationKind> {
    match &target.ty {
        HirType::Array { length, .. } => Some(ProofObligationKind::InBounds {
            target: target.clone(),
            index: index.clone(),
            length: *length,
        }),
        HirType::Map { .. } => Some(ProofObligationKind::MapPresence {
            target: target.clone(),
            key: index.clone(),
        }),
        _ => None,
    }
}

fn proof_set_membership_for_loop(stmt: &langlog_sema::HirForStmt) -> Option<ProofSetMembership> {
    let HirType::Set { element, .. } = &stmt.iterable.ty else {
        return None;
    };
    let HirPatternKind::Binding(binding) = &stmt.binding.kind else {
        return None;
    };

    Some(ProofSetMembership {
        member: binding.id,
        element_type: (**element).clone(),
        span: stmt.iterable.span,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnownFact {
    subject: HirBindingId,
    op: ObserveOp,
    value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BindingInfo {
    mutable: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct KnownSetMembership {
    member: HirBindingId,
    element_type: HirType,
}

#[derive(Debug, Clone, Default)]
struct RecordedControlFlow {
    stable_facts: Vec<KnownFact>,
    mutable_hints: Vec<MutableControlFlowHint>,
}

#[derive(Debug, Clone, Default)]
struct FlowState {
    stable_facts: Vec<KnownFact>,
    mutable_hints: Vec<MutableControlFlowHint>,
    set_memberships: Vec<KnownSetMembership>,
}

struct Checker<'a> {
    hir: &'a HirProgram,
    bindings: HashMap<HirBindingId, BindingInfo>,
    obligations: usize,
    diagnostics: Vec<Diagnostic>,
    facts: Vec<ProofFact>,
    warned_obligations: HashSet<(Span, Span)>,
}

impl<'a> Checker<'a> {
    fn new(hir: &'a HirProgram) -> Self {
        Self {
            hir,
            bindings: collect_bindings(hir),
            obligations: 0,
            diagnostics: Vec::new(),
            facts: Vec::new(),
            warned_obligations: HashSet::new(),
        }
    }

    fn check_module(&mut self) {
        let hir = self.hir;
        for function in &hir.functions {
            self.check_function(function);
        }
    }

    fn check_function(&mut self, function: &HirFunction) {
        let mut state = FlowState::default();
        self.check_block(&function.body, &mut state);
    }

    fn check_block(&mut self, block: &HirBlock, state: &mut FlowState) {
        let stable_fact_len = state.stable_facts.len();
        let hint_len = state.mutable_hints.len();
        for statement in &block.statements {
            self.check_statement(statement, state);
        }
        if let Some(expr) = &block.result {
            self.check_expr(expr, state);
        }
        state.stable_facts.truncate(stable_fact_len);
        state.mutable_hints.truncate(hint_len);
    }

    fn check_statement(&mut self, statement: &HirStmt, state: &mut FlowState) {
        match statement {
            HirStmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.check_expr(value, state);
                }
            }
            HirStmt::Assign(stmt) => {
                self.check_expr(&stmt.target, state);
                self.check_expr(&stmt.value, state);
            }
            HirStmt::Expr(stmt) => {
                self.check_expr(&stmt.expr, state);
            }
            HirStmt::If(stmt) => {
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
                    self.check_else_branch(else_branch, state);
                }
            }
            HirStmt::Match(stmt) => {
                self.check_expr(&stmt.expr, state);
                for arm in &stmt.arms {
                    match &arm.body {
                        HirMatchBody::Block(block) => self.check_block(block, state),
                        HirMatchBody::Expr(expr) => self.check_expr(expr, state),
                    }
                }
            }
            HirStmt::For(stmt) => {
                self.check_expr(&stmt.iterable, state);
                let membership_snapshot = state.set_memberships.len();
                if let Some(membership) = set_membership_for_loop(stmt) {
                    state.set_memberships.push(membership);
                }
                self.check_block(&stmt.body, state);
                state.set_memberships.truncate(membership_snapshot);
            }
            HirStmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.check_expr(value, state);
                }
            }
            HirStmt::Observe(stmt) => {
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

    fn check_else_branch(&mut self, branch: &HirElseBranch, state: &mut FlowState) {
        match branch {
            HirElseBranch::Block(block) => self.check_block(block, state),
            HirElseBranch::If(stmt) => {
                self.check_statement(&HirStmt::If(*stmt.clone()), state);
            }
        }
    }

    fn check_expr(&mut self, expr: &HirExpr, state: &mut FlowState) {
        match &expr.kind {
            HirExprKind::Binding(_)
            | HirExprKind::Item(_)
            | HirExprKind::HostBuiltin(_)
            | HirExprKind::Int(_)
            | HirExprKind::Bool(_) => {}
            HirExprKind::Tuple(elements) | HirExprKind::Array(elements) => {
                for element in elements {
                    self.check_expr(element, state);
                }
            }
            HirExprKind::Block(block) => {
                self.check_block(block, state);
            }
            HirExprKind::Unary { expr, .. } => {
                self.check_expr(expr, state);
            }
            HirExprKind::Binary { left, right, .. } => {
                self.check_expr(left, state);
                self.check_expr(right, state);
            }
            HirExprKind::Recover { expr, fallback, .. } => {
                self.check_expr(expr, state);
                self.check_expr(fallback, state);
            }
            HirExprKind::Call { callee, args } => {
                self.check_expr(callee, state);
                for arg in args {
                    self.check_expr(arg, state);
                }
            }
            HirExprKind::Index { target, index } => {
                self.check_expr(target, state);
                self.check_expr(index, state);

                self.obligations += 1;
                match &target.ty {
                    HirType::Array { .. } => {
                        if !index_is_proven_in_bounds(target, index, &state.stable_facts) {
                            self.report_mutable_hint_warning_if_needed(
                                index.span,
                                state,
                                |facts| index_is_proven_in_bounds(target, index, facts),
                            );
                            self.report_out_of_bounds_index(index.span);
                        }
                    }
                    HirType::Map { .. } => {
                        if !map_key_is_proven_present(target, index, &state.set_memberships) {
                            self.report_missing_map_key(index.span);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn record_control_flow_facts(&mut self, condition: &HirExpr) -> RecordedControlFlow {
        match &condition.kind {
            HirExprKind::Binary {
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
            HirExprKind::Binary { op, left, right } => {
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
            HirExprKind::Recover { .. } => RecordedControlFlow::default(),
            _ => RecordedControlFlow::default(),
        }
    }

    fn known_fact(&self, left: &HirExpr, op: ObserveOp, right: &HirExpr) -> Option<ResolvedFact> {
        let subject = binding_id(left)?;
        let binding = self.bindings.get(&subject)?;
        let value = eval_const_u64(right)?;
        Some(ResolvedFact {
            fact: KnownFact { subject, op, value },
            binding_span: subject.declaration_span,
            mutable: binding.mutable,
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

    fn report_out_of_bounds_index(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible out-of-bounds indexing is not proven safe")
                .with_label(Label::primary(span, "prove this index stays within bounds")),
        );
    }

    fn report_missing_map_key(&mut self, span: Span) {
        self.diagnostics.push(
            Diagnostic::error("possible missing map key is not proven present")
                .with_label(Label::primary(span, "prove this key is present in the map")),
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

fn stable_and_mutable_facts(state: &FlowState, skip_hint: Option<usize>) -> Vec<KnownFact> {
    let mut facts = state.stable_facts.clone();
    for (index, hint) in state.mutable_hints.iter().enumerate() {
        if Some(index) != skip_hint {
            facts.push(hint.fact.clone());
        }
    }
    facts
}

fn proven_binary_u32_range(
    op: BinaryOp,
    left: &HirExpr,
    right: &HirExpr,
    facts: &[KnownFact],
) -> Option<(u64, u64)> {
    let (left_low, left_high) = proven_u32_range(left, facts)?;
    let (right_low, right_high) = proven_u32_range(right, facts)?;

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

fn proven_u32_range(expr: &HirExpr, facts: &[KnownFact]) -> Option<(u64, u64)> {
    match &expr.kind {
        HirExprKind::Int(value) => (*value <= U32_MAX_U64).then_some((*value, *value)),
        HirExprKind::Binding(subject) => bounds_for_binding(*subject, facts),
        HirExprKind::Binary { op, left, right } => proven_binary_u32_range(*op, left, right, facts),
        HirExprKind::Unary { .. }
        | HirExprKind::Item(_)
        | HirExprKind::HostBuiltin(_)
        | HirExprKind::Bool(_)
        | HirExprKind::Tuple(_)
        | HirExprKind::Array(_)
        | HirExprKind::Block(_)
        | HirExprKind::Recover { .. }
        | HirExprKind::Call { .. }
        | HirExprKind::Index { .. } => None,
    }
}

fn binding_id(expr: &HirExpr) -> Option<HirBindingId> {
    match expr.kind {
        HirExprKind::Binding(id) => Some(id),
        _ => None,
    }
}

fn bounds_for_binding(subject: HirBindingId, facts: &[KnownFact]) -> Option<(u64, u64)> {
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

fn eval_const_u64(expr: &HirExpr) -> Option<u64> {
    match &expr.kind {
        HirExprKind::Int(value) => Some(*value),
        HirExprKind::Unary { .. }
        | HirExprKind::Binding(_)
        | HirExprKind::Item(_)
        | HirExprKind::HostBuiltin(_)
        | HirExprKind::Bool(_)
        | HirExprKind::Tuple(_)
        | HirExprKind::Array(_)
        | HirExprKind::Block(_)
        | HirExprKind::Recover { .. }
        | HirExprKind::Call { .. }
        | HirExprKind::Index { .. } => None,
        HirExprKind::Binary { op, left, right } => {
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

fn index_is_proven_in_bounds(target: &HirExpr, index: &HirExpr, facts: &[KnownFact]) -> bool {
    let Some(length) = array_length(target) else {
        return false;
    };

    if let Some(value) = eval_const_u64(index) {
        return value < length;
    }

    proven_u32_range(index, facts)
        .map(|(_, upper)| upper < length)
        .unwrap_or(false)
}

fn map_key_is_proven_present(
    target: &HirExpr,
    index: &HirExpr,
    memberships: &[KnownSetMembership],
) -> bool {
    let HirType::Map { key, .. } = &target.ty else {
        return false;
    };
    let Some(member) = binding_id(index) else {
        return false;
    };

    memberships
        .iter()
        .any(|membership| membership.member == member && membership.element_type == **key)
}

fn set_membership_for_loop(stmt: &langlog_sema::HirForStmt) -> Option<KnownSetMembership> {
    let HirType::Set { element, .. } = &stmt.iterable.ty else {
        return None;
    };
    let HirPatternKind::Binding(binding) = &stmt.binding.kind else {
        return None;
    };

    Some(KnownSetMembership {
        member: binding.id,
        element_type: (**element).clone(),
    })
}

fn array_length(expr: &HirExpr) -> Option<u64> {
    match &expr.ty {
        HirType::Array { length, .. } => Some(*length),
        _ => None,
    }
}

fn collect_bindings(hir: &HirProgram) -> HashMap<HirBindingId, BindingInfo> {
    let mut bindings = HashMap::new();
    for function in &hir.functions {
        for param in &function.params {
            collect_binding(&mut bindings, param);
        }
        collect_block_bindings(&mut bindings, &function.body);
    }
    bindings
}

fn collect_binding(bindings: &mut HashMap<HirBindingId, BindingInfo>, binding: &HirBinding) {
    bindings.insert(
        binding.id,
        BindingInfo {
            mutable: binding.mutable,
        },
    );
}

fn collect_block_bindings(bindings: &mut HashMap<HirBindingId, BindingInfo>, block: &HirBlock) {
    for statement in &block.statements {
        collect_statement_bindings(bindings, statement);
    }
    if let Some(expr) = &block.result {
        collect_expr_bindings(bindings, expr);
    }
}

fn collect_statement_bindings(bindings: &mut HashMap<HirBindingId, BindingInfo>, stmt: &HirStmt) {
    match stmt {
        HirStmt::Let(stmt) => {
            collect_binding(bindings, &stmt.binding);
            if let Some(value) = &stmt.value {
                collect_expr_bindings(bindings, value);
            }
        }
        HirStmt::Assign(stmt) => {
            collect_expr_bindings(bindings, &stmt.target);
            collect_expr_bindings(bindings, &stmt.value);
        }
        HirStmt::Expr(stmt) => {
            collect_expr_bindings(bindings, &stmt.expr);
        }
        HirStmt::If(stmt) => {
            collect_expr_bindings(bindings, &stmt.condition);
            collect_block_bindings(bindings, &stmt.then_block);
            if let Some(else_branch) = &stmt.else_branch {
                collect_else_branch_bindings(bindings, else_branch);
            }
        }
        HirStmt::Match(stmt) => {
            collect_expr_bindings(bindings, &stmt.expr);
            for arm in &stmt.arms {
                collect_pattern_bindings(bindings, &arm.pattern);
                match &arm.body {
                    HirMatchBody::Block(block) => collect_block_bindings(bindings, block),
                    HirMatchBody::Expr(expr) => collect_expr_bindings(bindings, expr),
                }
            }
        }
        HirStmt::For(stmt) => {
            collect_pattern_bindings(bindings, &stmt.binding);
            collect_expr_bindings(bindings, &stmt.iterable);
            collect_block_bindings(bindings, &stmt.body);
        }
        HirStmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                collect_expr_bindings(bindings, value);
            }
        }
        HirStmt::Observe(stmt) => {
            collect_expr_bindings(bindings, &stmt.left);
            collect_expr_bindings(bindings, &stmt.right);
            collect_block_bindings(bindings, &stmt.else_block);
        }
    }
}

fn collect_else_branch_bindings(
    bindings: &mut HashMap<HirBindingId, BindingInfo>,
    branch: &HirElseBranch,
) {
    match branch {
        HirElseBranch::Block(block) => collect_block_bindings(bindings, block),
        HirElseBranch::If(stmt) => {
            collect_statement_bindings(bindings, &HirStmt::If(*stmt.clone()))
        }
    }
}

fn collect_pattern_bindings(
    bindings: &mut HashMap<HirBindingId, BindingInfo>,
    pattern: &HirPattern,
) {
    if let HirPatternKind::Binding(binding) = &pattern.kind {
        collect_binding(bindings, binding);
    }
}

fn collect_expr_bindings(bindings: &mut HashMap<HirBindingId, BindingInfo>, expr: &HirExpr) {
    match &expr.kind {
        HirExprKind::Binding(_)
        | HirExprKind::Item(_)
        | HirExprKind::HostBuiltin(_)
        | HirExprKind::Int(_)
        | HirExprKind::Bool(_) => {}
        HirExprKind::Tuple(elements) | HirExprKind::Array(elements) => {
            for element in elements {
                collect_expr_bindings(bindings, element);
            }
        }
        HirExprKind::Block(block) => collect_block_bindings(bindings, block),
        HirExprKind::Unary { expr, .. } => collect_expr_bindings(bindings, expr),
        HirExprKind::Binary { left, right, .. } => {
            collect_expr_bindings(bindings, left);
            collect_expr_bindings(bindings, right);
        }
        HirExprKind::Recover {
            expr,
            error_binding,
            fallback,
        } => {
            collect_expr_bindings(bindings, expr);
            if let Some(binding) = error_binding {
                collect_binding(bindings, binding);
            }
            collect_expr_bindings(bindings, fallback);
        }
        HirExprKind::Call { callee, args } => {
            collect_expr_bindings(bindings, callee);
            for arg in args {
                collect_expr_bindings(bindings, arg);
            }
        }
        HirExprKind::Index { target, index } => {
            collect_expr_bindings(bindings, target);
            collect_expr_bindings(bindings, index);
        }
    }
}
