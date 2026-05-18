use std::collections::HashMap;

use langlog_sema::{
    CheckedProgram, HirBinding, HirBindingId, HirBlock, HirElseBranch, HirExpr, HirExprKind,
    HirFunction, HirItemId, HirMarkerFamily, HirMarkerPlace, HirMarkerRequirement, HirMarkerRule,
    HirMarkerRuleStmt, HirMarkerTemplateArg, HirMatchBody, HirPattern, HirPatternKind, HirProgram,
    HirStmt, HirType, HostBuiltin,
};
use langlog_syntax::ast::{BinaryOp, ObserveOp};
use langlog_syntax::{ByteOffset, Diagnostic, FileId, Label, Severity, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaceId {
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofPlace {
    pub id: PlaceId,
    pub kind: PlaceKind,
    pub ty: HirType,
    pub span: Span,
    pub display: String,
    pub value: Option<PlaceValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceKind {
    Binding { binding: HirBindingId, version: u32 },
    Temporary,
    ConstantU32(u64),
    ConstantBool(bool),
    ArrayLength { array: PlaceId, length: u64 },
    Item,
    HostBuiltin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceValue {
    U32(u64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerPattern {
    True { value: PlaceId },
    False { value: PlaceId },
    Equal { left: PlaceId, right: PlaceId },
    LessThan { left: PlaceId, right: PlaceId },
    GreaterThan { left: PlaceId, right: PlaceId },
    LessOrEqual { left: PlaceId, right: PlaceId },
    GreaterOrEqual { left: PlaceId, right: PlaceId },
    MemberOf { key: PlaceId, map: PlaceId },
    SetMember { element_type: HirType },
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerFactSource {
    Observe,
    ControlFlowTruth,
    ControlFlowFalse,
    CompanionRule,
    AssignmentIdentity,
    ImmutableCarryForward,
    TrustedBuiltin,
    UnsafeConstruction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerFact {
    pub target: PlaceId,
    pub marker: MarkerPattern,
    pub source: MarkerFactSource,
    pub origin_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerObligation {
    pub target: ObligationTarget,
    pub required: MarkerPattern,
    pub source: ObligationSource,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObligationTarget {
    Place(PlaceId),
    StateCycle { span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObligationSource {
    Index { array: PlaceId, index: PlaceId },
    MapLookup { map: PlaceId, key: PlaceId },
    MarkerRequirement,
    EventCycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProof {
    pub obligations: usize,
    pub observations: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub facts: Vec<MarkerFact>,
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
    let mut checker = Checker::new(&proof_ir);
    checker.check_program();

    let facts = checker.facts;
    CheckedProof {
        obligations: checker.obligations,
        observations: facts.len(),
        diagnostics: checker.diagnostics,
        facts,
        proof_ir: Some(proof_ir),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofProgram {
    pub places: Vec<ProofPlace>,
    pub functions: Vec<ProofFunction>,
    pub marker_rules: Vec<ProofMarkerRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFunction {
    pub body: ProofBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofMarkerRule {
    pub name: String,
    pub params: Vec<ProofMarkerRuleParam>,
    pub body: ProofMarkerRuleBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofMarkerRuleParam {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofMarkerRuleBlock {
    pub statements: Vec<ProofMarkerRuleStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofMarkerRuleStmt {
    If(ProofMarkerRuleIfStmt),
    Implies(ProofMarkerImplication),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofMarkerRuleIfStmt {
    pub subject: String,
    pub marker: ProofMarkerTemplate,
    pub body: ProofMarkerRuleBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofMarkerImplication {
    pub marker: ProofMarkerTemplate,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofMarkerTemplate {
    pub family: HirMarkerFamily,
    pub args: Vec<ProofMarkerTemplateArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofMarkerTemplateArg {
    Place(String),
    Binding(String),
    U32(u64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofBlock {
    pub entries: Vec<ProofEntry>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofEntry {
    Fact {
        fact: MarkerFact,
        span: Span,
    },
    Let {
        binding: HirBindingId,
        place: PlaceId,
        value: Option<PlaceId>,
        span: Span,
    },
    Assign {
        binding: HirBindingId,
        old_place: PlaceId,
        new_place: PlaceId,
        value: PlaceId,
        span: Span,
    },
    Branch {
        condition: ProofExpr,
        then_facts: Vec<MarkerFact>,
        else_facts: Vec<MarkerFact>,
        then_block: ProofBlock,
        else_block: Option<ProofBlock>,
        span: Span,
    },
    Observe {
        left: ProofExpr,
        op: ObserveOp,
        right: ProofExpr,
        result: PlaceId,
        facts: Vec<MarkerFact>,
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
        obligation: MarkerObligation,
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
            Self::Fact { span, .. }
            | Self::Let { span, .. }
            | Self::Assign { span, .. }
            | Self::Branch { span, .. }
            | Self::Observe { span, .. }
            | Self::For { span, .. }
            | Self::Obligation { span, .. }
            | Self::Eval { span, .. }
            | Self::Scope { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSetMembership {
    pub member: PlaceId,
    pub element_type: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofExpr {
    pub kind: ProofExprKind,
    pub ty: HirType,
    pub span: Span,
    pub place: PlaceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofExprKind {
    Binding(HirBindingId),
    Item(HirItemId),
    HostBuiltin(HostBuiltin),
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
    UnsafeMarker {
        marker: HirMarkerFamily,
        args: Vec<ProofExpr>,
    },
}

pub fn lower_to_proof_ir(hir: &HirProgram) -> ProofProgram {
    ProofLowerer::new(collect_bindings(hir), collect_function_markers(hir)).lower_program(hir)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionMarkerSignature {
    params: Vec<ParamMarkerSignature>,
    return_markers: Vec<HirMarkerRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParamMarkerSignature {
    binding: HirBindingId,
    markers: Vec<HirMarkerRequirement>,
}

struct ProofLowerer {
    binding_info: HashMap<HirBindingId, BindingInfo>,
    function_markers: HashMap<HirItemId, FunctionMarkerSignature>,
    current_return_markers: Vec<HirMarkerRequirement>,
    binding_places: BindingPlaceMap,
    binding_versions: BindingVersionMap,
    places: Vec<ProofPlace>,
}

type BindingPlaceMap = HashMap<HirBindingId, PlaceId>;
type BindingVersionMap = HashMap<HirBindingId, u32>;
type BranchStateRef<'a> = (&'a BindingPlaceMap, &'a BindingVersionMap);

fn lower_marker_rule(rule: &HirMarkerRule) -> ProofMarkerRule {
    ProofMarkerRule {
        name: rule.name.clone(),
        params: rule
            .params
            .iter()
            .map(|param| ProofMarkerRuleParam {
                name: param.name.clone(),
                span: param.span,
            })
            .collect(),
        body: lower_marker_rule_block(&rule.body),
        span: rule.span,
    }
}

fn lower_marker_rule_block(block: &langlog_sema::HirMarkerRuleBlock) -> ProofMarkerRuleBlock {
    ProofMarkerRuleBlock {
        statements: block
            .statements
            .iter()
            .map(lower_marker_rule_statement)
            .collect(),
        span: block.span,
    }
}

fn lower_marker_rule_statement(statement: &HirMarkerRuleStmt) -> ProofMarkerRuleStmt {
    match statement {
        HirMarkerRuleStmt::If(stmt) => ProofMarkerRuleStmt::If(ProofMarkerRuleIfStmt {
            subject: stmt.refinement.subject.clone(),
            marker: lower_marker_template(&stmt.refinement.marker),
            body: lower_marker_rule_block(&stmt.body),
            span: stmt.span,
        }),
        HirMarkerRuleStmt::Implies(stmt) => ProofMarkerRuleStmt::Implies(ProofMarkerImplication {
            marker: lower_marker_template(&stmt.marker),
            target: stmt.target.clone(),
            span: stmt.span,
        }),
    }
}

fn lower_marker_template(template: &langlog_sema::HirMarkerTemplate) -> ProofMarkerTemplate {
    ProofMarkerTemplate {
        family: template.family,
        args: template
            .args
            .iter()
            .map(lower_marker_template_arg)
            .collect(),
        span: template.span,
    }
}

fn lower_marker_template_arg(arg: &HirMarkerTemplateArg) -> ProofMarkerTemplateArg {
    match arg {
        HirMarkerTemplateArg::Place(name) => ProofMarkerTemplateArg::Place(name.clone()),
        HirMarkerTemplateArg::Binding(name) => ProofMarkerTemplateArg::Binding(name.clone()),
        HirMarkerTemplateArg::U32(value) => ProofMarkerTemplateArg::U32(*value),
        HirMarkerTemplateArg::Bool(value) => ProofMarkerTemplateArg::Bool(*value),
    }
}

impl ProofLowerer {
    fn new(
        binding_info: HashMap<HirBindingId, BindingInfo>,
        function_markers: HashMap<HirItemId, FunctionMarkerSignature>,
    ) -> Self {
        Self {
            binding_info,
            function_markers,
            current_return_markers: Vec::new(),
            binding_places: HashMap::new(),
            binding_versions: HashMap::new(),
            places: Vec::new(),
        }
    }

    fn lower_program(mut self, hir: &HirProgram) -> ProofProgram {
        let mut functions = Vec::new();
        for function in &hir.functions {
            self.binding_places.clear();
            self.binding_versions.clear();
            self.current_return_markers = function.return_markers.clone();
            let mut entry_facts = Vec::new();
            for param in &function.params {
                let place = self.bind_initial_place(param, None);
                for marker in &param.markers {
                    if let Some(fact) = self.marker_requirement_fact(
                        marker,
                        place,
                        &HashMap::new(),
                        MarkerFactSource::TrustedBuiltin,
                        param.span,
                    ) {
                        entry_facts.push(fact);
                    }
                }
            }
            let mut body = self.lower_block(&function.body);
            self.insert_trailing_return_marker_obligations(&mut body);
            body.entries.splice(
                0..0,
                entry_facts.into_iter().map(|fact| ProofEntry::Fact {
                    span: fact.origin_span,
                    fact,
                }),
            );
            functions.push(ProofFunction {
                body,
                span: function.span,
            });
        }

        ProofProgram {
            places: self.places,
            functions,
            marker_rules: hir.marker_rules.iter().map(lower_marker_rule).collect(),
        }
    }

    fn lower_block(&mut self, block: &HirBlock) -> ProofBlock {
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

    fn lower_statement(&mut self, statement: &HirStmt, entries: &mut Vec<ProofEntry>) {
        match statement {
            HirStmt::Let(stmt) => {
                let value = stmt
                    .value
                    .as_ref()
                    .and_then(|value| self.lower_expr(value, entries));
                let value_place = value.as_ref().map(|value| value.place);
                let place = self.bind_initial_place(
                    &stmt.binding,
                    value_place.and_then(|place| self.place_value(place)),
                );
                entries.push(ProofEntry::Let {
                    binding: stmt.binding.id,
                    place,
                    value: value_place,
                    span: stmt.span,
                });
                self.push_marker_obligations(
                    entries,
                    &stmt.binding.markers,
                    place,
                    &HashMap::new(),
                    stmt.span,
                );
            }
            HirStmt::Assign(stmt) => {
                let value = self.lower_expr(&stmt.value, entries);
                let target = self.lower_expr(&stmt.target, entries);
                if let (
                    Some(value),
                    Some(ProofExpr {
                        kind: ProofExprKind::Binding(binding),
                        place: old_place,
                        ..
                    }),
                ) = (value, target)
                {
                    let new_place = self.advance_binding_place(
                        binding,
                        stmt.target.span,
                        self.place_value(value.place),
                    );
                    entries.push(ProofEntry::Assign {
                        binding,
                        old_place,
                        new_place,
                        value: value.place,
                        span: stmt.span,
                    });
                }
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
                let then_facts = self.condition_facts(&condition, true, stmt.condition.span);
                let else_facts = self.condition_facts(&condition, false, stmt.condition.span);

                let saved_places = self.binding_places.clone();
                let saved_versions = self.binding_versions.clone();

                self.binding_places = saved_places.clone();
                self.binding_versions = saved_versions.clone();
                let then_block = self.lower_block(&stmt.then_block);
                let then_places = self.binding_places.clone();
                let then_versions = self.binding_versions.clone();

                self.binding_places = saved_places.clone();
                self.binding_versions = saved_versions.clone();
                let else_block = stmt
                    .else_branch
                    .as_ref()
                    .map(|branch| self.lower_else_branch(branch));
                let else_places = self.binding_places.clone();
                let else_versions = self.binding_versions.clone();

                self.binding_places = saved_places;
                self.binding_versions = saved_versions;
                self.merge_branch_state(
                    &then_places,
                    &then_versions,
                    else_block.as_ref().map(|_| (&else_places, &else_versions)),
                );

                entries.push(ProofEntry::Branch {
                    condition,
                    then_facts,
                    else_facts,
                    then_block,
                    else_block,
                    span: stmt.span,
                });
            }
            HirStmt::Match(stmt) => {
                self.lower_expr(&stmt.expr, entries);
                let saved_places = self.binding_places.clone();
                let saved_versions = self.binding_versions.clone();
                for arm in &stmt.arms {
                    self.binding_places = saved_places.clone();
                    self.binding_versions = saved_versions.clone();
                    self.bind_pattern(&arm.pattern);
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
                self.binding_places = saved_places;
                self.binding_versions = saved_versions;
            }
            HirStmt::For(stmt) => {
                let Some(iterable) = self.lower_expr(&stmt.iterable, entries) else {
                    return;
                };
                let membership = self.bind_for_membership(stmt);
                entries.push(ProofEntry::For {
                    iterable,
                    membership,
                    body: self.lower_block(&stmt.body),
                    span: stmt.span,
                });
            }
            HirStmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    if let Some(value) = self.lower_expr(value, entries) {
                        self.push_return_marker_obligations(entries, &value, stmt.span);
                    }
                }
            }
            HirStmt::Forever(stmt) => entries.push(ProofEntry::Scope {
                block: self.lower_block(&stmt.body),
                span: stmt.span,
            }),
            HirStmt::Exit(stmt) => {
                if let Some(value) = self.lower_expr(&stmt.value, entries) {
                    self.push_return_marker_obligations(entries, &value, stmt.span);
                }
            }
            HirStmt::Delegate(stmt) => {
                let args: Vec<_> = stmt
                    .args
                    .iter()
                    .filter_map(|arg| self.lower_expr(arg, entries))
                    .collect();
                self.push_call_marker_obligations(entries, stmt.target, &args, stmt.span);
            }
            HirStmt::UnsafeMarker(stmt) => {
                let args: Vec<_> = stmt
                    .args
                    .iter()
                    .filter_map(|arg| self.lower_expr(arg, entries))
                    .collect();
                if let Some(fact) = self.unsafe_marker_fact(stmt.marker, &args, stmt.span) {
                    entries.push(ProofEntry::Fact {
                        span: stmt.span,
                        fact,
                    });
                }
            }
            HirStmt::Observe(stmt) => {
                let (Some(left), Some(right)) = (
                    self.lower_expr(&stmt.left, entries),
                    self.lower_expr(&stmt.right, entries),
                ) else {
                    return;
                };
                let result = self.new_temp(HirType::Bool, stmt.span, Some(PlaceValue::Bool(true)));
                let facts = self.truth_facts(result, true, MarkerFactSource::Observe, stmt.span);
                entries.push(ProofEntry::Observe {
                    left,
                    op: stmt.op,
                    right,
                    result,
                    facts,
                    else_block: self.lower_block(&stmt.else_block),
                    span: stmt.span,
                });
            }
        }
    }

    fn lower_else_branch(&mut self, branch: &HirElseBranch) -> ProofBlock {
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

    fn lower_expr(&mut self, expr: &HirExpr, entries: &mut Vec<ProofEntry>) -> Option<ProofExpr> {
        let (kind, place) = match &expr.kind {
            HirExprKind::Binding(id) => {
                let place = self.binding_place(*id);
                (ProofExprKind::Binding(*id), place)
            }
            HirExprKind::Item(item) => {
                let place =
                    self.new_place(PlaceKind::Item, expr.ty.clone(), expr.span, "item", None);
                (ProofExprKind::Item(*item), place)
            }
            HirExprKind::HostBuiltin(builtin) => {
                let place = self.new_place(
                    PlaceKind::HostBuiltin,
                    expr.ty.clone(),
                    expr.span,
                    "host builtin",
                    None,
                );
                (ProofExprKind::HostBuiltin(*builtin), place)
            }
            HirExprKind::Int(value) => {
                let place = self.u32_place(*value, expr.span);
                (ProofExprKind::Int(*value), place)
            }
            HirExprKind::Bool(value) => {
                let place = self.bool_place(*value, expr.span);
                (ProofExprKind::Bool(*value), place)
            }
            HirExprKind::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .filter_map(|element| self.lower_expr(element, entries))
                    .collect();
                let place = self.new_temp(expr.ty.clone(), expr.span, None);
                (ProofExprKind::Tuple(elements), place)
            }
            HirExprKind::Array(elements) => {
                let elements = elements
                    .iter()
                    .filter_map(|element| self.lower_expr(element, entries))
                    .collect();
                let place = self.new_temp(expr.ty.clone(), expr.span, None);
                (ProofExprKind::Array(elements), place)
            }
            HirExprKind::Block(block) => {
                entries.push(ProofEntry::Scope {
                    block: self.lower_block(block),
                    span: block.span,
                });
                return None;
            }
            HirExprKind::Unary { expr: inner, .. } => {
                let inner = self.lower_expr(inner, entries)?;
                let place = self.new_temp(expr.ty.clone(), expr.span, None);
                (
                    ProofExprKind::Unary {
                        expr: Box::new(inner),
                    },
                    place,
                )
            }
            HirExprKind::Binary { op, left, right } => {
                let left = self.lower_expr(left, entries)?;
                let right = self.lower_expr(right, entries)?;
                let value = eval_binary_value(*op, left.value(self), right.value(self));
                let place = self.new_temp(expr.ty.clone(), expr.span, value);
                (
                    ProofExprKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    place,
                )
            }
            HirExprKind::Recover { expr, fallback, .. } => {
                self.lower_expr(expr, entries);
                self.lower_expr(fallback, entries);
                return None;
            }
            HirExprKind::Call { callee, args } => {
                let callee = self.lower_expr(callee, entries)?;
                let args = args
                    .iter()
                    .filter_map(|arg| self.lower_expr(arg, entries))
                    .collect::<Vec<_>>();
                let place = self.new_temp(expr.ty.clone(), expr.span, None);
                self.push_call_marker_obligations_for_callee(entries, &callee, &args, expr.span);
                self.push_call_return_marker_facts(entries, &callee, &args, place, expr.span);
                (
                    ProofExprKind::Call {
                        callee: Box::new(callee),
                        args,
                    },
                    place,
                )
            }
            HirExprKind::UnsafeMarker { marker, args } => {
                let args = args
                    .iter()
                    .filter_map(|arg| self.lower_expr(arg, entries))
                    .collect::<Vec<_>>();
                let place = args.first()?.place;
                if let Some(fact) = self.unsafe_marker_fact(*marker, &args, expr.span) {
                    entries.push(ProofEntry::Fact {
                        span: expr.span,
                        fact,
                    });
                }
                (
                    ProofExprKind::UnsafeMarker {
                        marker: *marker,
                        args,
                    },
                    place,
                )
            }
            HirExprKind::Index { target, index } => {
                let target = self.lower_expr(target, entries)?;
                let index = self.lower_expr(index, entries)?;
                if let Some(obligation) = self.proof_obligation_for_index(&target, &index) {
                    entries.push(ProofEntry::Obligation {
                        span: index.span,
                        obligation,
                    });
                }
                let place = self.new_temp(expr.ty.clone(), expr.span, None);
                (
                    ProofExprKind::Index {
                        target: Box::new(target),
                        index: Box::new(index),
                    },
                    place,
                )
            }
        };

        Some(ProofExpr {
            kind,
            ty: expr.ty.clone(),
            span: expr.span,
            place,
        })
    }

    fn insert_trailing_return_marker_obligations(&mut self, body: &mut ProofBlock) {
        let Some(ProofEntry::Eval { expr, span }) = body.entries.last().cloned() else {
            return;
        };
        let insert_at = body.entries.len().saturating_sub(1);
        let mut obligations = Vec::new();
        self.push_return_marker_obligations(&mut obligations, &expr, span);
        body.entries.splice(insert_at..insert_at, obligations);
    }

    fn push_return_marker_obligations(
        &mut self,
        entries: &mut Vec<ProofEntry>,
        value: &ProofExpr,
        span: Span,
    ) {
        let markers = self.current_return_markers.clone();
        self.push_marker_obligations(entries, &markers, value.place, &HashMap::new(), span);
    }

    fn push_call_marker_obligations_for_callee(
        &mut self,
        entries: &mut Vec<ProofEntry>,
        callee: &ProofExpr,
        args: &[ProofExpr],
        span: Span,
    ) {
        if let ProofExprKind::Item(item) = callee.kind {
            self.push_call_marker_obligations(entries, item, args, span);
        }
    }

    fn push_call_marker_obligations(
        &mut self,
        entries: &mut Vec<ProofEntry>,
        item: HirItemId,
        args: &[ProofExpr],
        span: Span,
    ) {
        let Some(signature) = self.function_markers.get(&item).cloned() else {
            return;
        };
        let substitution = call_marker_substitution(&signature, args);
        for (param, arg) in signature.params.iter().zip(args.iter()) {
            self.push_marker_obligations(entries, &param.markers, arg.place, &substitution, span);
        }
    }

    fn push_call_return_marker_facts(
        &mut self,
        entries: &mut Vec<ProofEntry>,
        callee: &ProofExpr,
        args: &[ProofExpr],
        result: PlaceId,
        span: Span,
    ) {
        match callee.kind {
            ProofExprKind::HostBuiltin(HostBuiltin::ReadU32) => {
                entries.push(ProofEntry::Fact {
                    span,
                    fact: MarkerFact {
                        target: result,
                        marker: MarkerPattern::Event,
                        source: MarkerFactSource::TrustedBuiltin,
                        origin_span: span,
                    },
                });
            }
            ProofExprKind::Item(item) => {
                let Some(signature) = self.function_markers.get(&item).cloned() else {
                    return;
                };
                let substitution = call_marker_substitution(&signature, args);
                for marker in &signature.return_markers {
                    if let Some(fact) = self.marker_requirement_fact(
                        marker,
                        result,
                        &substitution,
                        MarkerFactSource::TrustedBuiltin,
                        span,
                    ) {
                        entries.push(ProofEntry::Fact { span, fact });
                    }
                }
            }
            _ => {}
        }
    }

    fn push_marker_obligations(
        &mut self,
        entries: &mut Vec<ProofEntry>,
        markers: &[HirMarkerRequirement],
        subject: PlaceId,
        substitution: &HashMap<HirBindingId, PlaceId>,
        span: Span,
    ) {
        for marker in markers {
            if let Some((target, required)) =
                self.marker_requirement_pattern(marker, subject, substitution)
            {
                entries.push(ProofEntry::Obligation {
                    span: marker.span,
                    obligation: MarkerObligation {
                        target: ObligationTarget::Place(target),
                        required,
                        source: ObligationSource::MarkerRequirement,
                        span,
                    },
                });
            }
        }
    }

    fn marker_requirement_fact(
        &mut self,
        marker: &HirMarkerRequirement,
        subject: PlaceId,
        substitution: &HashMap<HirBindingId, PlaceId>,
        source: MarkerFactSource,
        origin_span: Span,
    ) -> Option<MarkerFact> {
        let (target, marker) = self.marker_requirement_pattern(marker, subject, substitution)?;
        Some(MarkerFact {
            target,
            marker,
            source,
            origin_span,
        })
    }

    fn marker_requirement_pattern(
        &mut self,
        marker: &HirMarkerRequirement,
        subject: PlaceId,
        substitution: &HashMap<HirBindingId, PlaceId>,
    ) -> Option<(PlaceId, MarkerPattern)> {
        let args = marker
            .args
            .iter()
            .map(|arg| self.marker_place(arg, subject, substitution))
            .collect::<Option<Vec<_>>>()?;
        self.marker_pattern(marker.family, &args, subject)
    }

    fn unsafe_marker_fact(
        &mut self,
        family: HirMarkerFamily,
        args: &[ProofExpr],
        origin_span: Span,
    ) -> Option<MarkerFact> {
        let places = args.iter().map(|arg| arg.place).collect::<Vec<_>>();
        let subject = *places.first()?;
        let (target, marker) = self.marker_pattern(family, &places, subject)?;
        Some(MarkerFact {
            target,
            marker,
            source: MarkerFactSource::UnsafeConstruction,
            origin_span,
        })
    }

    fn marker_pattern(
        &mut self,
        family: HirMarkerFamily,
        args: &[PlaceId],
        subject: PlaceId,
    ) -> Option<(PlaceId, MarkerPattern)> {
        match family {
            HirMarkerFamily::Event => Some((subject, MarkerPattern::Event)),
            HirMarkerFamily::True => {
                let value = *args.first()?;
                Some((value, MarkerPattern::True { value }))
            }
            HirMarkerFamily::False => {
                let value = *args.first()?;
                Some((value, MarkerPattern::False { value }))
            }
            HirMarkerFamily::Equal => {
                let (left, right) = two_places(args)?;
                Some((left, MarkerPattern::Equal { left, right }))
            }
            HirMarkerFamily::LessThan => {
                let (left, right) = two_places(args)?;
                Some((left, MarkerPattern::LessThan { left, right }))
            }
            HirMarkerFamily::GreaterThan => {
                let (left, right) = two_places(args)?;
                Some((left, MarkerPattern::GreaterThan { left, right }))
            }
            HirMarkerFamily::LessOrEqual => {
                let (left, right) = two_places(args)?;
                Some((left, MarkerPattern::LessOrEqual { left, right }))
            }
            HirMarkerFamily::GreaterOrEqual => {
                let (left, right) = two_places(args)?;
                Some((left, MarkerPattern::GreaterOrEqual { left, right }))
            }
            HirMarkerFamily::MemberOf => {
                let (key, map) = two_places(args)?;
                Some((key, MarkerPattern::MemberOf { key, map }))
            }
        }
    }

    fn marker_place(
        &mut self,
        place: &HirMarkerPlace,
        subject: PlaceId,
        substitution: &HashMap<HirBindingId, PlaceId>,
    ) -> Option<PlaceId> {
        match place {
            HirMarkerPlace::Subject => Some(subject),
            HirMarkerPlace::Binding(binding) => substitution
                .get(binding)
                .copied()
                .or_else(|| self.binding_places.get(binding).copied())
                .or_else(|| Some(self.binding_place(*binding))),
            HirMarkerPlace::U32(value) => Some(self.u32_place(*value, self.place_span(subject))),
            HirMarkerPlace::Bool(value) => Some(self.bool_place(*value, self.place_span(subject))),
            HirMarkerPlace::ArrayLength(binding) => {
                let array = substitution
                    .get(binding)
                    .copied()
                    .or_else(|| self.binding_places.get(binding).copied())
                    .or_else(|| Some(self.binding_place(*binding)))?;
                let HirType::Array { length, .. } = &self.places.get(array.index)?.ty else {
                    return None;
                };
                Some(self.array_length_place(array, *length, self.place_span(array)))
            }
        }
    }

    fn proof_obligation_for_index(
        &mut self,
        target: &ProofExpr,
        index: &ProofExpr,
    ) -> Option<MarkerObligation> {
        match &target.ty {
            HirType::Array { length, .. } => {
                let length_place = self.array_length_place(target.place, *length, target.span);
                Some(MarkerObligation {
                    target: ObligationTarget::Place(index.place),
                    required: MarkerPattern::LessThan {
                        left: index.place,
                        right: length_place,
                    },
                    source: ObligationSource::Index {
                        array: target.place,
                        index: index.place,
                    },
                    span: index.span,
                })
            }
            HirType::Map { .. } => Some(MarkerObligation {
                target: ObligationTarget::Place(index.place),
                required: MarkerPattern::MemberOf {
                    key: index.place,
                    map: target.place,
                },
                source: ObligationSource::MapLookup {
                    map: target.place,
                    key: index.place,
                },
                span: index.span,
            }),
            _ => None,
        }
    }

    fn bind_initial_place(&mut self, binding: &HirBinding, value: Option<PlaceValue>) -> PlaceId {
        self.binding_versions.insert(binding.id, 0);
        let place = self.new_place(
            PlaceKind::Binding {
                binding: binding.id,
                version: 0,
            },
            binding.ty.clone(),
            binding.span,
            binding.name.clone(),
            value,
        );
        self.binding_places.insert(binding.id, place);
        place
    }

    fn advance_binding_place(
        &mut self,
        binding: HirBindingId,
        span: Span,
        value: Option<PlaceValue>,
    ) -> PlaceId {
        let next_version = self.binding_versions.get(&binding).copied().unwrap_or(0) + 1;
        self.binding_versions.insert(binding, next_version);
        let info = self
            .binding_info
            .get(&binding)
            .cloned()
            .unwrap_or_else(|| BindingInfo {
                name: "binding".to_owned(),
                mutable: false,
                ty: HirType::Unit,
                span,
            });
        let place = self.new_place(
            PlaceKind::Binding {
                binding,
                version: next_version,
            },
            info.ty,
            span,
            info.name,
            value,
        );
        self.binding_places.insert(binding, place);
        place
    }

    fn binding_place(&mut self, binding: HirBindingId) -> PlaceId {
        if let Some(place) = self.binding_places.get(&binding).copied() {
            return place;
        }

        let info = self
            .binding_info
            .get(&binding)
            .cloned()
            .unwrap_or_else(|| BindingInfo {
                name: "binding".to_owned(),
                mutable: false,
                ty: HirType::Unit,
                span: binding.declaration_span,
            });
        let binding = HirBinding {
            id: binding,
            name: info.name,
            kind: langlog_sema::BindingKind::Local,
            mutable: info.mutable,
            ty: info.ty,
            markers: Vec::new(),
            span: info.span,
        };
        self.bind_initial_place(&binding, None)
    }

    fn bind_pattern(&mut self, pattern: &HirPattern) -> Option<PlaceId> {
        match &pattern.kind {
            HirPatternKind::Binding(binding) => Some(self.bind_initial_place(binding, None)),
            HirPatternKind::Wildcard | HirPatternKind::Int(_) | HirPatternKind::Bool(_) => None,
        }
    }

    fn bind_for_membership(
        &mut self,
        stmt: &langlog_sema::HirForStmt,
    ) -> Option<ProofSetMembership> {
        let member = self.bind_pattern(&stmt.binding)?;
        let HirType::Set { element, .. } = &stmt.iterable.ty else {
            return None;
        };

        Some(ProofSetMembership {
            member,
            element_type: (**element).clone(),
            span: stmt.iterable.span,
        })
    }

    fn merge_branch_state(
        &mut self,
        then_places: &BindingPlaceMap,
        then_versions: &BindingVersionMap,
        else_state: Option<BranchStateRef<'_>>,
    ) {
        let original = self.binding_places.clone();
        for (binding, original_place) in original {
            let then_changed = then_places
                .get(&binding)
                .map(|place| *place != original_place)
                .unwrap_or(false);
            let else_changed = else_state
                .and_then(|(places, _)| places.get(&binding))
                .map(|place| *place != original_place)
                .unwrap_or(false);
            if !then_changed && !else_changed {
                continue;
            }

            let mut merged_version = self.binding_versions.get(&binding).copied().unwrap_or(0);
            merged_version = merged_version.max(then_versions.get(&binding).copied().unwrap_or(0));
            if let Some((_, else_versions)) = else_state {
                merged_version =
                    merged_version.max(else_versions.get(&binding).copied().unwrap_or(0));
            }
            self.binding_versions.insert(binding, merged_version);
            self.advance_binding_place(binding, binding.declaration_span, None);
        }
    }

    fn condition_facts(
        &mut self,
        condition: &ProofExpr,
        truth: bool,
        origin_span: Span,
    ) -> Vec<MarkerFact> {
        let source = if truth {
            MarkerFactSource::ControlFlowTruth
        } else {
            MarkerFactSource::ControlFlowFalse
        };
        let mut facts = self.truth_facts(condition.place, truth, source, origin_span);

        match &condition.kind {
            ProofExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } if truth => {
                facts.extend(self.condition_facts(left, true, left.span));
                facts.extend(self.condition_facts(right, true, right.span));
            }
            ProofExprKind::Binary {
                op: BinaryOp::Or, ..
            } => {}
            ProofExprKind::Binary { .. } => {}
            _ => {}
        }

        facts
    }

    fn truth_facts(
        &self,
        value: PlaceId,
        truth: bool,
        source: MarkerFactSource,
        origin_span: Span,
    ) -> Vec<MarkerFact> {
        vec![MarkerFact {
            target: value,
            marker: if truth {
                MarkerPattern::True { value }
            } else {
                MarkerPattern::False { value }
            },
            source,
            origin_span,
        }]
    }

    fn new_temp(&mut self, ty: HirType, span: Span, value: Option<PlaceValue>) -> PlaceId {
        self.new_place(PlaceKind::Temporary, ty, span, "temporary", value)
    }

    fn u32_place(&mut self, value: u64, span: Span) -> PlaceId {
        self.new_place(
            PlaceKind::ConstantU32(value),
            HirType::U32,
            span,
            value.to_string(),
            Some(PlaceValue::U32(value)),
        )
    }

    fn bool_place(&mut self, value: bool, span: Span) -> PlaceId {
        self.new_place(
            PlaceKind::ConstantBool(value),
            HirType::Bool,
            span,
            value.to_string(),
            Some(PlaceValue::Bool(value)),
        )
    }

    fn array_length_place(&mut self, array: PlaceId, length: u64, span: Span) -> PlaceId {
        let display = format!("{}.length", self.place_display(array));
        self.new_place(
            PlaceKind::ArrayLength { array, length },
            HirType::U32,
            span,
            display,
            Some(PlaceValue::U32(length)),
        )
    }

    fn new_place(
        &mut self,
        kind: PlaceKind,
        ty: HirType,
        span: Span,
        display: impl Into<String>,
        value: Option<PlaceValue>,
    ) -> PlaceId {
        let id = PlaceId {
            index: self.places.len(),
        };
        self.places.push(ProofPlace {
            id,
            kind,
            ty,
            span,
            display: display.into(),
            value,
        });
        id
    }

    fn place_value(&self, place: PlaceId) -> Option<PlaceValue> {
        self.places.get(place.index).and_then(|place| place.value)
    }

    fn place_span(&self, place: PlaceId) -> Span {
        self.places
            .get(place.index)
            .map(|place| place.span)
            .expect("proof places should only reference allocated places")
    }

    fn place_display(&self, place: PlaceId) -> String {
        self.places
            .get(place.index)
            .map(|place| place.display.clone())
            .unwrap_or_else(|| format!("place#{}", place.index))
    }
}

impl ProofExpr {
    fn value(&self, lowerer: &ProofLowerer) -> Option<PlaceValue> {
        lowerer.place_value(self.place)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompanionRuleSet {
    rules: HashMap<String, ProofMarkerRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompanionInvocation {
    name: &'static str,
    places: Vec<PlaceId>,
    origin_span: Span,
}

impl CompanionRuleSet {
    fn new(source_rules: &[ProofMarkerRule]) -> Self {
        let mut rules = builtin_companion_rules();
        for rule in source_rules {
            rules.insert(rule.name.clone(), rule.clone());
        }
        Self { rules }
    }

    fn get(&self, name: &str) -> Option<&ProofMarkerRule> {
        self.rules.get(name)
    }
}

struct Checker<'a> {
    program: &'a ProofProgram,
    companion_rules: CompanionRuleSet,
    obligations: usize,
    diagnostics: Vec<Diagnostic>,
    facts: Vec<MarkerFact>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a ProofProgram) -> Self {
        Self {
            program,
            companion_rules: CompanionRuleSet::new(&program.marker_rules),
            obligations: 0,
            diagnostics: Vec::new(),
            facts: Vec::new(),
        }
    }

    fn check_program(&mut self) {
        for function in &self.program.functions {
            let mut env = MarkerEnv::default();
            self.check_block(&function.body, &mut env);
        }
    }

    fn check_block(&mut self, block: &ProofBlock, env: &mut MarkerEnv) {
        let snapshot = env.facts.len();
        for entry in &block.entries {
            self.check_entry(entry, env);
        }
        env.facts.truncate(snapshot);
    }

    fn check_entry(&mut self, entry: &ProofEntry, env: &mut MarkerEnv) {
        match entry {
            ProofEntry::Fact { fact, .. } => {
                self.add_fact(env, fact.clone());
            }
            ProofEntry::Let {
                place, value, span, ..
            } => {
                if let Some(value) = value {
                    self.copy_marker_facts(env, *value, *place, *span);
                }
            }
            ProofEntry::Assign {
                new_place,
                value,
                span,
                ..
            } => {
                self.copy_marker_facts(env, *value, *new_place, *span);
            }
            ProofEntry::Branch {
                condition,
                then_facts,
                else_facts,
                then_block,
                else_block,
                ..
            } => {
                let snapshot = env.facts.len();
                self.add_facts(env, then_facts);
                self.apply_condition_companions(env, condition, true);
                self.check_block(then_block, env);
                env.facts.truncate(snapshot);

                if let Some(else_block) = else_block {
                    let snapshot = env.facts.len();
                    self.add_facts(env, else_facts);
                    self.apply_condition_companions(env, condition, false);
                    self.check_block(else_block, env);
                    env.facts.truncate(snapshot);
                }
            }
            ProofEntry::Observe {
                left,
                op,
                right,
                result,
                facts,
                else_block,
                span,
                ..
            } => {
                let snapshot = env.facts.len();
                self.check_block(else_block, env);
                env.facts.truncate(snapshot);
                self.add_facts(env, facts);
                self.apply_observe_companion(env, left, *op, right, *result, *span);
            }
            ProofEntry::For {
                membership, body, ..
            } => {
                let snapshot = env.facts.len();
                if let Some(membership) = membership {
                    let fact = MarkerFact {
                        target: membership.member,
                        marker: MarkerPattern::SetMember {
                            element_type: membership.element_type.clone(),
                        },
                        source: MarkerFactSource::TrustedBuiltin,
                        origin_span: membership.span,
                    };
                    self.add_fact(env, fact);
                }
                self.check_block(body, env);
                env.facts.truncate(snapshot);
            }
            ProofEntry::Obligation { obligation, .. } => {
                self.obligations += 1;
                if !self.obligation_is_satisfied(env, obligation) {
                    self.report_marker_obligation(env, obligation);
                }
            }
            ProofEntry::Eval { .. } => {}
            ProofEntry::Scope { block, .. } => self.check_block(block, env),
        }
    }

    fn add_facts(&mut self, env: &mut MarkerEnv, facts: &[MarkerFact]) {
        for fact in facts {
            self.add_fact(env, fact.clone());
        }
    }

    fn add_fact(&mut self, env: &mut MarkerEnv, fact: MarkerFact) {
        let normalized = self.normalized_less_than_fact(&fact);
        env.facts.push(fact.clone());
        self.facts.push(fact);
        if let Some(normalized) = normalized {
            env.facts.push(normalized.clone());
            self.facts.push(normalized);
        }
    }

    fn normalized_less_than_fact(&self, fact: &MarkerFact) -> Option<MarkerFact> {
        let MarkerPattern::LessOrEqual { left, right } = fact.marker else {
            return None;
        };
        let right = self.successor_literal_place(right)?;
        Some(MarkerFact {
            target: left,
            marker: MarkerPattern::LessThan { left, right },
            source: MarkerFactSource::TrustedBuiltin,
            origin_span: fact.origin_span,
        })
    }

    fn successor_literal_place(&self, place: PlaceId) -> Option<PlaceId> {
        let PlaceValue::U32(value) = self.literal_value(place)? else {
            return None;
        };
        let successor = value.checked_add(1)?;
        self.program
            .places
            .iter()
            .find(|candidate| self.literal_value(candidate.id) == Some(PlaceValue::U32(successor)))
            .map(|place| place.id)
    }

    fn copy_marker_facts(
        &mut self,
        env: &mut MarkerEnv,
        source: PlaceId,
        destination: PlaceId,
        origin_span: Span,
    ) {
        let copied: Vec<_> = env
            .facts
            .iter()
            .filter(|fact| fact.target == source)
            .filter_map(|fact| {
                substitute_marker_place(&fact.marker, source, destination).map(|marker| {
                    MarkerFact {
                        target: destination,
                        marker,
                        source: MarkerFactSource::AssignmentIdentity,
                        origin_span,
                    }
                })
            })
            .collect();
        self.add_facts(env, &copied);
    }

    fn apply_condition_companions(
        &mut self,
        env: &mut MarkerEnv,
        condition: &ProofExpr,
        truth: bool,
    ) {
        match &condition.kind {
            ProofExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } if truth => {
                self.apply_condition_companions(env, left, true);
                self.apply_condition_companions(env, right, true);
            }
            ProofExprKind::Binary {
                op: BinaryOp::Or, ..
            } => {}
            ProofExprKind::Binary { op, left, right } => {
                if let Some(invocation) = comparison_invocation(
                    *op,
                    left.place,
                    right.place,
                    condition.place,
                    condition.span,
                ) {
                    self.apply_companion_rule(env, invocation);
                }
            }
            _ => {}
        }
    }

    fn apply_observe_companion(
        &mut self,
        env: &mut MarkerEnv,
        left: &ProofExpr,
        op: ObserveOp,
        right: &ProofExpr,
        result: PlaceId,
        origin_span: Span,
    ) {
        if let Some(invocation) =
            observe_invocation(op, left.place, right.place, result, origin_span)
        {
            self.apply_companion_rule(env, invocation);
        }
    }

    fn apply_companion_rule(&mut self, env: &mut MarkerEnv, invocation: CompanionInvocation) {
        let Some(rule) = self.companion_rules.get(invocation.name).cloned() else {
            return;
        };
        if rule.params.len() != invocation.places.len() {
            return;
        }

        let mut bindings = HashMap::new();
        for (param, place) in rule.params.iter().zip(invocation.places.iter().copied()) {
            bindings.insert(param.name.clone(), place);
        }
        self.apply_companion_rule_block(env, &rule.body, &bindings, invocation.origin_span);
    }

    fn apply_companion_rule_block(
        &mut self,
        env: &mut MarkerEnv,
        block: &ProofMarkerRuleBlock,
        bindings: &HashMap<String, PlaceId>,
        origin_span: Span,
    ) {
        for statement in &block.statements {
            match statement {
                ProofMarkerRuleStmt::If(stmt) => {
                    for refined in self.match_marker_refinement(env, stmt, bindings) {
                        self.apply_companion_rule_block(env, &stmt.body, &refined, origin_span);
                    }
                }
                ProofMarkerRuleStmt::Implies(implication) => {
                    if let Some(fact) =
                        self.instantiate_implication(implication, bindings, origin_span)
                    {
                        self.add_fact(env, fact);
                    }
                }
            }
        }
    }

    fn match_marker_refinement(
        &self,
        env: &MarkerEnv,
        stmt: &ProofMarkerRuleIfStmt,
        bindings: &HashMap<String, PlaceId>,
    ) -> Vec<HashMap<String, PlaceId>> {
        let Some(subject) = bindings.get(&stmt.subject).copied() else {
            return Vec::new();
        };

        env.facts
            .iter()
            .filter(|fact| fact.target == subject)
            .filter_map(|fact| {
                let mut refined = bindings.clone();
                self.match_marker_template(&stmt.marker, subject, &fact.marker, &mut refined)
                    .then_some(refined)
            })
            .collect()
    }

    fn match_marker_template(
        &self,
        template: &ProofMarkerTemplate,
        subject: PlaceId,
        actual: &MarkerPattern,
        bindings: &mut HashMap<String, PlaceId>,
    ) -> bool {
        match (template.family, actual) {
            (HirMarkerFamily::True, MarkerPattern::True { value })
            | (HirMarkerFamily::False, MarkerPattern::False { value }) => {
                self.same_place(subject, *value) && template.args.is_empty()
            }
            (HirMarkerFamily::Event, MarkerPattern::Event) => template.args.is_empty(),
            (HirMarkerFamily::Equal, MarkerPattern::Equal { left, right })
            | (HirMarkerFamily::LessThan, MarkerPattern::LessThan { left, right })
            | (HirMarkerFamily::GreaterThan, MarkerPattern::GreaterThan { left, right })
            | (HirMarkerFamily::LessOrEqual, MarkerPattern::LessOrEqual { left, right })
            | (HirMarkerFamily::GreaterOrEqual, MarkerPattern::GreaterOrEqual { left, right }) => {
                self.match_two_place_template(&template.args, *left, *right, bindings)
            }
            (HirMarkerFamily::MemberOf, MarkerPattern::MemberOf { key, map }) => {
                self.match_two_place_template(&template.args, *key, *map, bindings)
            }
            _ => false,
        }
    }

    fn match_two_place_template(
        &self,
        args: &[ProofMarkerTemplateArg],
        left: PlaceId,
        right: PlaceId,
        bindings: &mut HashMap<String, PlaceId>,
    ) -> bool {
        let [left_arg, right_arg] = args else {
            return false;
        };
        self.match_template_arg(left_arg, left, bindings)
            && self.match_template_arg(right_arg, right, bindings)
    }

    fn match_template_arg(
        &self,
        arg: &ProofMarkerTemplateArg,
        actual: PlaceId,
        bindings: &mut HashMap<String, PlaceId>,
    ) -> bool {
        match arg {
            ProofMarkerTemplateArg::Place(name) => bindings
                .get(name)
                .is_some_and(|expected| self.same_place_or_literal(*expected, actual)),
            ProofMarkerTemplateArg::Binding(name) => match bindings.get(name).copied() {
                Some(expected) => self.same_place_or_literal(expected, actual),
                None => {
                    bindings.insert(name.clone(), actual);
                    true
                }
            },
            ProofMarkerTemplateArg::U32(_) | ProofMarkerTemplateArg::Bool(_) => false,
        }
    }

    fn instantiate_implication(
        &self,
        implication: &ProofMarkerImplication,
        bindings: &HashMap<String, PlaceId>,
        origin_span: Span,
    ) -> Option<MarkerFact> {
        let target = bindings.get(&implication.target).copied()?;
        let marker = self.instantiate_marker_template(&implication.marker, target, bindings)?;
        Some(MarkerFact {
            target,
            marker,
            source: MarkerFactSource::CompanionRule,
            origin_span,
        })
    }

    fn instantiate_marker_template(
        &self,
        template: &ProofMarkerTemplate,
        target: PlaceId,
        bindings: &HashMap<String, PlaceId>,
    ) -> Option<MarkerPattern> {
        match template.family {
            HirMarkerFamily::Event => Some(MarkerPattern::Event),
            HirMarkerFamily::True if template.args.is_empty() => {
                Some(MarkerPattern::True { value: target })
            }
            HirMarkerFamily::False if template.args.is_empty() => {
                Some(MarkerPattern::False { value: target })
            }
            HirMarkerFamily::Equal
            | HirMarkerFamily::LessThan
            | HirMarkerFamily::GreaterThan
            | HirMarkerFamily::LessOrEqual
            | HirMarkerFamily::GreaterOrEqual
            | HirMarkerFamily::MemberOf => {
                let (left, right) = self.instantiate_two_template_args(&template.args, bindings)?;
                match template.family {
                    HirMarkerFamily::Equal => Some(MarkerPattern::Equal { left, right }),
                    HirMarkerFamily::LessThan => Some(MarkerPattern::LessThan { left, right }),
                    HirMarkerFamily::GreaterThan => {
                        Some(MarkerPattern::GreaterThan { left, right })
                    }
                    HirMarkerFamily::LessOrEqual => {
                        Some(MarkerPattern::LessOrEqual { left, right })
                    }
                    HirMarkerFamily::GreaterOrEqual => {
                        Some(MarkerPattern::GreaterOrEqual { left, right })
                    }
                    HirMarkerFamily::MemberOf => Some(MarkerPattern::MemberOf {
                        key: left,
                        map: right,
                    }),
                    HirMarkerFamily::Event | HirMarkerFamily::True | HirMarkerFamily::False => None,
                }
            }
            HirMarkerFamily::True | HirMarkerFamily::False => None,
        }
    }

    fn instantiate_two_template_args(
        &self,
        args: &[ProofMarkerTemplateArg],
        bindings: &HashMap<String, PlaceId>,
    ) -> Option<(PlaceId, PlaceId)> {
        let [left, right] = args else {
            return None;
        };
        Some((
            self.instantiate_template_arg(left, bindings)?,
            self.instantiate_template_arg(right, bindings)?,
        ))
    }

    fn instantiate_template_arg(
        &self,
        arg: &ProofMarkerTemplateArg,
        bindings: &HashMap<String, PlaceId>,
    ) -> Option<PlaceId> {
        match arg {
            ProofMarkerTemplateArg::Place(name) | ProofMarkerTemplateArg::Binding(name) => {
                bindings.get(name).copied()
            }
            ProofMarkerTemplateArg::U32(_) | ProofMarkerTemplateArg::Bool(_) => None,
        }
    }

    fn obligation_is_satisfied(
        &mut self,
        env: &mut MarkerEnv,
        obligation: &MarkerObligation,
    ) -> bool {
        if env
            .facts
            .iter()
            .any(|fact| self.fact_satisfies_obligation(fact, obligation))
        {
            return true;
        }

        if self.constant_relation_satisfies(&obligation.required) {
            return true;
        }

        if let MarkerPattern::MemberOf { key, map } = obligation.required {
            if self.trusted_set_to_map_transfer(env, key, map, obligation.span) {
                return true;
            }
        }

        false
    }

    fn fact_satisfies_obligation(&self, fact: &MarkerFact, obligation: &MarkerObligation) -> bool {
        if let ObligationTarget::Place(target) = obligation.target {
            if fact.target != target {
                return false;
            }
        }
        self.marker_matches(&fact.marker, &obligation.required)
    }

    fn trusted_set_to_map_transfer(
        &mut self,
        env: &mut MarkerEnv,
        key: PlaceId,
        map: PlaceId,
        origin_span: Span,
    ) -> bool {
        let Some(HirType::Map { key: map_key, .. }) =
            self.program.places.get(map.index).map(|place| &place.ty)
        else {
            return false;
        };

        let has_set_member = env.facts.iter().any(|fact| {
            fact.target == key
                && matches!(
                    &fact.marker,
                    MarkerPattern::SetMember { element_type } if element_type == map_key.as_ref()
                )
        });
        if !has_set_member {
            return false;
        }

        let fact = MarkerFact {
            target: key,
            marker: MarkerPattern::MemberOf { key, map },
            source: MarkerFactSource::TrustedBuiltin,
            origin_span,
        };
        self.add_fact(env, fact);
        true
    }

    fn marker_matches(&self, actual: &MarkerPattern, required: &MarkerPattern) -> bool {
        match (actual, required) {
            (MarkerPattern::True { value: a }, MarkerPattern::True { value: b })
            | (MarkerPattern::False { value: a }, MarkerPattern::False { value: b }) => {
                self.same_place(*a, *b)
            }
            (
                MarkerPattern::Equal {
                    left: actual_left,
                    right: actual_right,
                },
                MarkerPattern::Equal {
                    left: required_left,
                    right: required_right,
                },
            )
            | (
                MarkerPattern::LessThan {
                    left: actual_left,
                    right: actual_right,
                },
                MarkerPattern::LessThan {
                    left: required_left,
                    right: required_right,
                },
            )
            | (
                MarkerPattern::GreaterThan {
                    left: actual_left,
                    right: actual_right,
                },
                MarkerPattern::GreaterThan {
                    left: required_left,
                    right: required_right,
                },
            )
            | (
                MarkerPattern::LessOrEqual {
                    left: actual_left,
                    right: actual_right,
                },
                MarkerPattern::LessOrEqual {
                    left: required_left,
                    right: required_right,
                },
            )
            | (
                MarkerPattern::GreaterOrEqual {
                    left: actual_left,
                    right: actual_right,
                },
                MarkerPattern::GreaterOrEqual {
                    left: required_left,
                    right: required_right,
                },
            ) => {
                self.same_place(*actual_left, *required_left)
                    && self.same_place_or_literal(*actual_right, *required_right)
            }
            (
                MarkerPattern::MemberOf {
                    key: actual_key,
                    map: actual_map,
                },
                MarkerPattern::MemberOf {
                    key: required_key,
                    map: required_map,
                },
            ) => {
                self.same_place(*actual_key, *required_key)
                    && self.same_place(*actual_map, *required_map)
            }
            (
                MarkerPattern::SetMember {
                    element_type: actual_type,
                },
                MarkerPattern::SetMember {
                    element_type: required_type,
                },
            ) => actual_type == required_type,
            (MarkerPattern::Event, MarkerPattern::Event) => true,
            _ => false,
        }
    }

    fn constant_relation_satisfies(&self, required: &MarkerPattern) -> bool {
        match required {
            MarkerPattern::LessThan { left, right } => {
                match (self.place_value(*left), self.place_value(*right)) {
                    (Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => left < right,
                    _ => false,
                }
            }
            MarkerPattern::LessOrEqual { left, right } => {
                match (self.place_value(*left), self.place_value(*right)) {
                    (Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => left <= right,
                    _ => false,
                }
            }
            MarkerPattern::GreaterThan { left, right } => {
                match (self.place_value(*left), self.place_value(*right)) {
                    (Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => left > right,
                    _ => false,
                }
            }
            MarkerPattern::GreaterOrEqual { left, right } => {
                match (self.place_value(*left), self.place_value(*right)) {
                    (Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => left >= right,
                    _ => false,
                }
            }
            MarkerPattern::Equal { left, right } => {
                self.place_value(*left).is_some()
                    && self.place_value(*left) == self.place_value(*right)
            }
            MarkerPattern::True { value } => {
                self.place_value(*value) == Some(PlaceValue::Bool(true))
            }
            MarkerPattern::False { value } => {
                self.place_value(*value) == Some(PlaceValue::Bool(false))
            }
            MarkerPattern::MemberOf { .. }
            | MarkerPattern::SetMember { .. }
            | MarkerPattern::Event => false,
        }
    }

    fn report_marker_obligation(&mut self, env: &MarkerEnv, obligation: &MarkerObligation) {
        let message = match obligation.source {
            ObligationSource::Index { .. } => "possible out-of-bounds indexing is not proven safe",
            ObligationSource::MapLookup { .. } => "possible missing map key is not proven present",
            ObligationSource::MarkerRequirement => "required marker is not proven for this value",
            ObligationSource::EventCycle => "task cycle is not proven productive",
        };
        let label = match obligation.source {
            ObligationSource::Index { .. } => "prove this index stays within bounds",
            ObligationSource::MapLookup { .. } => "prove this key is present in the map",
            ObligationSource::MarkerRequirement => "prove this marker requirement",
            ObligationSource::EventCycle => "introduce an Event marker on every cyclic path",
        };

        let target = match obligation.target {
            ObligationTarget::Place(place) => self.place_display(place),
            ObligationTarget::StateCycle { .. } => "state cycle".to_owned(),
        };
        let required = self.marker_display(&obligation.required);

        let mut diagnostic = Diagnostic::error(message)
            .with_label(Label::primary(obligation.span, label))
            .with_note(format!("required marker: {required}"))
            .with_note(format!("target place: {target}"));

        if let Some(near_miss) = self.near_miss_marker(env, &obligation.required) {
            diagnostic = diagnostic.with_note(format!("known near-miss marker: {near_miss}"));
        } else if let Some(known) = self.known_markers_for_target(env, obligation.target.clone()) {
            diagnostic = diagnostic.with_note(format!("known marker facts for target: {known}"));
        }

        if let Some(suggestion) = self.guard_suggestion(&obligation.required) {
            diagnostic = diagnostic.with_note(suggestion);
        }

        self.diagnostics.push(diagnostic);
    }

    fn near_miss_marker(&self, env: &MarkerEnv, required: &MarkerPattern) -> Option<String> {
        env.facts
            .iter()
            .find(|fact| self.marker_is_near_miss(&fact.marker, required))
            .map(|fact| self.marker_display(&fact.marker))
    }

    fn marker_is_near_miss(&self, actual: &MarkerPattern, required: &MarkerPattern) -> bool {
        match (actual, required) {
            (
                MarkerPattern::LessThan {
                    left: actual_left,
                    right: actual_right,
                },
                MarkerPattern::LessThan {
                    left: required_left,
                    right: required_right,
                },
            ) => {
                !self.same_place(*actual_left, *required_left)
                    && self.same_binding_name(*actual_left, *required_left)
                    && self.same_place_or_literal(*actual_right, *required_right)
            }
            (
                MarkerPattern::MemberOf {
                    key: actual_key,
                    map: actual_map,
                },
                MarkerPattern::MemberOf {
                    key: required_key,
                    map: required_map,
                },
            ) => {
                !self.same_place(*actual_key, *required_key)
                    && self.same_binding_name(*actual_key, *required_key)
                    && self.same_place(*actual_map, *required_map)
            }
            _ => false,
        }
    }

    fn known_markers_for_target(
        &self,
        env: &MarkerEnv,
        target: ObligationTarget,
    ) -> Option<String> {
        let ObligationTarget::Place(target) = target else {
            return None;
        };
        let markers: Vec<_> = env
            .facts
            .iter()
            .filter(|fact| fact.target == target)
            .map(|fact| self.marker_display(&fact.marker))
            .collect();
        (!markers.is_empty()).then(|| markers.join(", "))
    }

    fn guard_suggestion(&self, required: &MarkerPattern) -> Option<String> {
        match required {
            MarkerPattern::LessThan { left, right } => Some(format!(
                "add a guard such as `if {} < {} {{ ... }}` or an `observe` before the operation",
                self.place_display(*left),
                self.place_display(*right)
            )),
            MarkerPattern::MemberOf { key, map } => Some(format!(
                "iterate a matching key set or add a checked map-presence guard for `{}` in `{}`",
                self.place_display(*key),
                self.place_display(*map)
            )),
            MarkerPattern::Event => Some(
                "receive fresh external input or an externally scheduled occurrence before the next `go`".to_owned(),
            ),
            _ => None,
        }
    }

    fn same_place(&self, left: PlaceId, right: PlaceId) -> bool {
        left == right
    }

    fn same_place_or_literal(&self, left: PlaceId, right: PlaceId) -> bool {
        left == right
            || self.literal_value(left).is_some()
                && self.literal_value(left) == self.literal_value(right)
    }

    fn same_binding_name(&self, left: PlaceId, right: PlaceId) -> bool {
        let Some(left) = self.program.places.get(left.index) else {
            return false;
        };
        let Some(right) = self.program.places.get(right.index) else {
            return false;
        };
        matches!(
            (&left.kind, &right.kind),
            (
                PlaceKind::Binding {
                    binding: left_binding,
                    ..
                },
                PlaceKind::Binding {
                    binding: right_binding,
                    ..
                }
            ) if left_binding == right_binding
        )
    }

    fn literal_value(&self, place: PlaceId) -> Option<PlaceValue> {
        match self.program.places.get(place.index)?.kind {
            PlaceKind::ConstantU32(_)
            | PlaceKind::ConstantBool(_)
            | PlaceKind::ArrayLength { .. } => self.place_value(place),
            PlaceKind::Binding { .. }
            | PlaceKind::Temporary
            | PlaceKind::Item
            | PlaceKind::HostBuiltin => None,
        }
    }

    fn place_value(&self, place: PlaceId) -> Option<PlaceValue> {
        self.program
            .places
            .get(place.index)
            .and_then(|place| place.value)
    }

    fn place_display(&self, place: PlaceId) -> String {
        self.program
            .places
            .get(place.index)
            .map(|place| match place.kind {
                PlaceKind::Binding { version, .. } if version > 0 => {
                    format!("{}#{}", place.display, version)
                }
                _ => place.display.clone(),
            })
            .unwrap_or_else(|| format!("place#{}", place.index))
    }

    fn marker_display(&self, marker: &MarkerPattern) -> String {
        match marker {
            MarkerPattern::True { value } => format!("True({})", self.place_display(*value)),
            MarkerPattern::False { value } => format!("False({})", self.place_display(*value)),
            MarkerPattern::Equal { left, right } => {
                format!(
                    "Equal({}, {})",
                    self.place_display(*left),
                    self.place_display(*right)
                )
            }
            MarkerPattern::LessThan { left, right } => format!(
                "LessThan({}, {})",
                self.place_display(*left),
                self.place_display(*right)
            ),
            MarkerPattern::GreaterThan { left, right } => format!(
                "GreaterThan({}, {})",
                self.place_display(*left),
                self.place_display(*right)
            ),
            MarkerPattern::LessOrEqual { left, right } => format!(
                "LessOrEqual({}, {})",
                self.place_display(*left),
                self.place_display(*right)
            ),
            MarkerPattern::GreaterOrEqual { left, right } => format!(
                "GreaterOrEqual({}, {})",
                self.place_display(*left),
                self.place_display(*right)
            ),
            MarkerPattern::MemberOf { key, map } => format!(
                "MemberOf({}, {})",
                self.place_display(*key),
                self.place_display(*map)
            ),
            MarkerPattern::SetMember { element_type } => {
                format!("SetMember({})", format_hir_type(element_type))
            }
            MarkerPattern::Event => "Event".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct MarkerEnv {
    facts: Vec<MarkerFact>,
}

fn substitute_marker_place(
    marker: &MarkerPattern,
    source: PlaceId,
    destination: PlaceId,
) -> Option<MarkerPattern> {
    let replace = |place: PlaceId| if place == source { destination } else { place };
    Some(match marker {
        MarkerPattern::True { value } => MarkerPattern::True {
            value: replace(*value),
        },
        MarkerPattern::False { value } => MarkerPattern::False {
            value: replace(*value),
        },
        MarkerPattern::Equal { left, right } => MarkerPattern::Equal {
            left: replace(*left),
            right: replace(*right),
        },
        MarkerPattern::LessThan { left, right } => MarkerPattern::LessThan {
            left: replace(*left),
            right: replace(*right),
        },
        MarkerPattern::GreaterThan { left, right } => MarkerPattern::GreaterThan {
            left: replace(*left),
            right: replace(*right),
        },
        MarkerPattern::LessOrEqual { left, right } => MarkerPattern::LessOrEqual {
            left: replace(*left),
            right: replace(*right),
        },
        MarkerPattern::GreaterOrEqual { left, right } => MarkerPattern::GreaterOrEqual {
            left: replace(*left),
            right: replace(*right),
        },
        MarkerPattern::MemberOf { key, map } => MarkerPattern::MemberOf {
            key: replace(*key),
            map: replace(*map),
        },
        MarkerPattern::SetMember { element_type } => MarkerPattern::SetMember {
            element_type: element_type.clone(),
        },
        MarkerPattern::Event => MarkerPattern::Event,
    })
}

fn binary_to_observe_op(op: BinaryOp) -> Option<ObserveOp> {
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

fn comparison_invocation(
    op: BinaryOp,
    left: PlaceId,
    right: PlaceId,
    result: PlaceId,
    origin_span: Span,
) -> Option<CompanionInvocation> {
    let op = binary_to_observe_op(op)?;
    observe_invocation(op, left, right, result, origin_span)
}

fn observe_invocation(
    op: ObserveOp,
    left: PlaceId,
    right: PlaceId,
    result: PlaceId,
    origin_span: Span,
) -> Option<CompanionInvocation> {
    let name = match op {
        ObserveOp::Eq => "Equal",
        ObserveOp::Lt => "LessThan",
        ObserveOp::LtEq => "LessOrEqual",
        ObserveOp::Gt => "GreaterThan",
        ObserveOp::GtEq => "GreaterOrEqual",
        ObserveOp::NotEq => return None,
    };
    Some(CompanionInvocation {
        name,
        places: vec![left, right, result],
        origin_span,
    })
}

fn builtin_companion_rules() -> HashMap<String, ProofMarkerRule> {
    [
        builtin_equal_rule(),
        builtin_order_rule(
            "LessThan",
            HirMarkerFamily::LessThan,
            HirMarkerFamily::GreaterThan,
            HirMarkerFamily::GreaterOrEqual,
            HirMarkerFamily::LessOrEqual,
        ),
        builtin_order_rule(
            "GreaterThan",
            HirMarkerFamily::GreaterThan,
            HirMarkerFamily::LessThan,
            HirMarkerFamily::LessOrEqual,
            HirMarkerFamily::GreaterOrEqual,
        ),
        builtin_order_rule(
            "LessOrEqual",
            HirMarkerFamily::LessOrEqual,
            HirMarkerFamily::GreaterOrEqual,
            HirMarkerFamily::GreaterThan,
            HirMarkerFamily::LessThan,
        ),
        builtin_order_rule(
            "GreaterOrEqual",
            HirMarkerFamily::GreaterOrEqual,
            HirMarkerFamily::LessOrEqual,
            HirMarkerFamily::LessThan,
            HirMarkerFamily::GreaterThan,
        ),
    ]
    .into_iter()
    .map(|rule| (rule.name.clone(), rule))
    .collect()
}

fn builtin_equal_rule() -> ProofMarkerRule {
    builtin_rule(
        "Equal",
        vec![ProofMarkerRuleStmt::If(ProofMarkerRuleIfStmt {
            subject: "result".to_owned(),
            marker: marker_template(HirMarkerFamily::True, Vec::new()),
            body: ProofMarkerRuleBlock {
                statements: vec![
                    implies(HirMarkerFamily::Equal, "a", "b", "a"),
                    implies(HirMarkerFamily::Equal, "b", "a", "b"),
                ],
                span: empty_span(),
            },
            span: empty_span(),
        })],
    )
}

fn builtin_order_rule(
    name: &str,
    true_left: HirMarkerFamily,
    true_right: HirMarkerFamily,
    false_left: HirMarkerFamily,
    false_right: HirMarkerFamily,
) -> ProofMarkerRule {
    builtin_rule(
        name,
        vec![
            ProofMarkerRuleStmt::If(ProofMarkerRuleIfStmt {
                subject: "result".to_owned(),
                marker: marker_template(HirMarkerFamily::True, Vec::new()),
                body: ProofMarkerRuleBlock {
                    statements: vec![
                        implies(true_left, "a", "b", "a"),
                        implies(true_right, "b", "a", "b"),
                    ],
                    span: empty_span(),
                },
                span: empty_span(),
            }),
            ProofMarkerRuleStmt::If(ProofMarkerRuleIfStmt {
                subject: "result".to_owned(),
                marker: marker_template(HirMarkerFamily::False, Vec::new()),
                body: ProofMarkerRuleBlock {
                    statements: vec![
                        implies(false_left, "a", "b", "a"),
                        implies(false_right, "b", "a", "b"),
                    ],
                    span: empty_span(),
                },
                span: empty_span(),
            }),
        ],
    )
}

fn builtin_rule(name: &str, statements: Vec<ProofMarkerRuleStmt>) -> ProofMarkerRule {
    ProofMarkerRule {
        name: name.to_owned(),
        params: ["a", "b", "result"]
            .into_iter()
            .map(|name| ProofMarkerRuleParam {
                name: name.to_owned(),
                span: empty_span(),
            })
            .collect(),
        body: ProofMarkerRuleBlock {
            statements,
            span: empty_span(),
        },
        span: empty_span(),
    }
}

fn implies(family: HirMarkerFamily, left: &str, right: &str, target: &str) -> ProofMarkerRuleStmt {
    ProofMarkerRuleStmt::Implies(ProofMarkerImplication {
        marker: marker_template(
            family,
            vec![
                ProofMarkerTemplateArg::Place(left.to_owned()),
                ProofMarkerTemplateArg::Place(right.to_owned()),
            ],
        ),
        target: target.to_owned(),
        span: empty_span(),
    })
}

fn marker_template(
    family: HirMarkerFamily,
    args: Vec<ProofMarkerTemplateArg>,
) -> ProofMarkerTemplate {
    ProofMarkerTemplate {
        family,
        args,
        span: empty_span(),
    }
}

fn empty_span() -> Span {
    Span::new(FileId::new(0), ByteOffset::new(0), ByteOffset::new(0))
}

fn two_places(args: &[PlaceId]) -> Option<(PlaceId, PlaceId)> {
    match args {
        [left, right] => Some((*left, *right)),
        _ => None,
    }
}

fn eval_binary_value(
    op: BinaryOp,
    left: Option<PlaceValue>,
    right: Option<PlaceValue>,
) -> Option<PlaceValue> {
    match (op, left, right) {
        (BinaryOp::EqEq, Some(left), Some(right)) => Some(PlaceValue::Bool(left == right)),
        (BinaryOp::NotEq, Some(left), Some(right)) => Some(PlaceValue::Bool(left != right)),
        (BinaryOp::Lt, Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => {
            Some(PlaceValue::Bool(left < right))
        }
        (BinaryOp::LtEq, Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => {
            Some(PlaceValue::Bool(left <= right))
        }
        (BinaryOp::Gt, Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => {
            Some(PlaceValue::Bool(left > right))
        }
        (BinaryOp::GtEq, Some(PlaceValue::U32(left)), Some(PlaceValue::U32(right))) => {
            Some(PlaceValue::Bool(left >= right))
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BindingInfo {
    name: String,
    mutable: bool,
    ty: HirType,
    span: Span,
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

fn collect_function_markers(hir: &HirProgram) -> HashMap<HirItemId, FunctionMarkerSignature> {
    hir.functions
        .iter()
        .map(|function| (function.id, function_marker_signature(function)))
        .collect()
}

fn function_marker_signature(function: &HirFunction) -> FunctionMarkerSignature {
    FunctionMarkerSignature {
        params: function
            .params
            .iter()
            .map(|param| ParamMarkerSignature {
                binding: param.id,
                markers: param.markers.clone(),
            })
            .collect(),
        return_markers: function.return_markers.clone(),
    }
}

fn call_marker_substitution(
    signature: &FunctionMarkerSignature,
    args: &[ProofExpr],
) -> HashMap<HirBindingId, PlaceId> {
    signature
        .params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (param.binding, arg.place))
        .collect()
}

fn collect_binding(bindings: &mut HashMap<HirBindingId, BindingInfo>, binding: &HirBinding) {
    bindings.insert(
        binding.id,
        BindingInfo {
            name: binding.name.clone(),
            mutable: binding.mutable,
            ty: binding.ty.clone(),
            span: binding.span,
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
        HirStmt::Expr(stmt) => collect_expr_bindings(bindings, &stmt.expr),
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
        HirStmt::Forever(stmt) => collect_block_bindings(bindings, &stmt.body),
        HirStmt::Exit(stmt) => collect_expr_bindings(bindings, &stmt.value),
        HirStmt::Delegate(stmt) => {
            for arg in &stmt.args {
                collect_expr_bindings(bindings, arg);
            }
        }
        HirStmt::Observe(stmt) => {
            collect_expr_bindings(bindings, &stmt.left);
            collect_expr_bindings(bindings, &stmt.right);
            collect_block_bindings(bindings, &stmt.else_block);
        }
        HirStmt::UnsafeMarker(stmt) => {
            for arg in &stmt.args {
                collect_expr_bindings(bindings, arg);
            }
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
        HirExprKind::UnsafeMarker { args, .. } => {
            for arg in args {
                collect_expr_bindings(bindings, arg);
            }
        }
    }
}

fn format_hir_type(ty: &HirType) -> String {
    match ty {
        HirType::Unit => "()".to_owned(),
        HirType::Bool => "bool".to_owned(),
        HirType::U32 => "u32".to_owned(),
        HirType::ArithmeticError => "ArithmeticError".to_owned(),
        HirType::Tuple(elements) => {
            let elements = elements
                .iter()
                .map(format_hir_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({elements})")
        }
        HirType::Array { element, length } => {
            format!("[{}; {length}]", format_hir_type(element))
        }
        HirType::Option(element) => format!("Option<{}>", format_hir_type(element)),
        HirType::Result { ok, err } => {
            format!("Result<{}, {}>", format_hir_type(ok), format_hir_type(err))
        }
        HirType::Set { element, capacity } => {
            format!("Set<{}, {capacity}>", format_hir_type(element))
        }
        HirType::Map {
            key,
            value,
            capacity,
        } => format!(
            "Map<{}, {}, {capacity}>",
            format_hir_type(key),
            format_hir_type(value)
        ),
        HirType::Range(element) => format!("Range<{}>", format_hir_type(element)),
        HirType::Named(name) => name.clone(),
        HirType::Function(function) => {
            let params = function
                .params
                .iter()
                .map(format_hir_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({params}) -> {}", format_hir_type(&function.return_type))
        }
    }
}
