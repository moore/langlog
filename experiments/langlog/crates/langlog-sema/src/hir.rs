use std::collections::HashMap;

use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, Item, MarkerAnnotation, MarkerArg,
    MarkerArgKind, MarkerImplicationStmt, MarkerRefinement, MarkerRule, MarkerRuleBlock,
    MarkerRuleStmt, MatchBody, ObserveOp, Pattern, PatternKind, Stmt, Task, Type, TypeKind,
    UnaryOp,
};
use langlog_syntax::{ParsedModule, Span, Spanned};

use crate::{
    lower_type, BindingKind, FunctionType, HostBuiltin, ResolvedName, SemanticType, TypeFacts,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirItemId {
    pub declaration_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirBindingId {
    pub declaration_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirProgram {
    pub functions: Vec<HirFunction>,
    pub marker_rules: Vec<HirMarkerRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirFunctionKind {
    Function,
    Task,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub kind: HirFunctionKind,
    pub id: HirItemId,
    pub name: String,
    pub params: Vec<HirBinding>,
    pub return_type: HirType,
    pub return_markers: Vec<HirMarkerRequirement>,
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirBinding {
    pub id: HirBindingId,
    pub name: String,
    pub kind: BindingKind,
    pub mutable: bool,
    pub ty: HirType,
    pub markers: Vec<HirMarkerRequirement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerRule {
    pub name: String,
    pub params: Vec<HirMarkerRuleParam>,
    pub body: HirMarkerRuleBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerRuleParam {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerRuleBlock {
    pub statements: Vec<HirMarkerRuleStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirMarkerRuleStmt {
    If(HirMarkerRuleIfStmt),
    Implies(HirMarkerImplication),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerRuleIfStmt {
    pub refinement: HirMarkerRefinement,
    pub body: HirMarkerRuleBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerRefinement {
    pub subject: String,
    pub marker: HirMarkerTemplate,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerImplication {
    pub marker: HirMarkerTemplate,
    pub target: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerTemplate {
    pub family: HirMarkerFamily,
    pub args: Vec<HirMarkerTemplateArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirMarkerTemplateArg {
    Place(String),
    Binding(String),
    U32(u64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirBlock {
    pub statements: Vec<HirStmt>,
    pub result: Option<Box<HirExpr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmt {
    Let(HirLetStmt),
    Assign(HirAssignStmt),
    Expr(HirExprStmt),
    If(HirIfStmt),
    Match(HirMatchStmt),
    For(HirForStmt),
    Return(HirReturnStmt),
    Forever(HirForeverStmt),
    Exit(HirExitStmt),
    Delegate(HirDelegateStmt),
    Observe(HirObserveStmt),
    UnsafeMarker(HirUnsafeMarkerStmt),
}

impl HirStmt {
    pub fn span(&self) -> Span {
        match self {
            Self::Let(stmt) => stmt.span,
            Self::Assign(stmt) => stmt.span,
            Self::Expr(stmt) => stmt.span,
            Self::If(stmt) => stmt.span,
            Self::Match(stmt) => stmt.span,
            Self::For(stmt) => stmt.span,
            Self::Return(stmt) => stmt.span,
            Self::Forever(stmt) => stmt.span,
            Self::Exit(stmt) => stmt.span,
            Self::Delegate(stmt) => stmt.span,
            Self::Observe(stmt) => stmt.span,
            Self::UnsafeMarker(stmt) => stmt.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirLetStmt {
    pub binding: HirBinding,
    pub annotation: Option<HirType>,
    pub value: Option<HirExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirAssignStmt {
    pub target: HirExpr,
    pub value: HirExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExprStmt {
    pub expr: HirExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirIfStmt {
    pub condition: HirExpr,
    pub then_block: HirBlock,
    pub else_branch: Option<HirElseBranch>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirElseBranch {
    Block(HirBlock),
    If(Box<HirIfStmt>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMatchStmt {
    pub expr: HirExpr,
    pub arms: Vec<HirMatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub body: HirMatchBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirMatchBody {
    Block(HirBlock),
    Expr(HirExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirForStmt {
    pub binding: HirPattern,
    pub iterable: HirExpr,
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirReturnStmt {
    pub value: Option<HirExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirForeverStmt {
    pub body: HirBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExitStmt {
    pub value: HirExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirDelegateStmt {
    pub target: HirItemId,
    pub args: Vec<HirExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirObserveStmt {
    pub left: HirExpr,
    pub op: ObserveOp,
    pub right: HirExpr,
    pub else_block: HirBlock,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirUnsafeMarkerStmt {
    pub marker: HirMarkerFamily,
    pub args: Vec<HirExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirPattern {
    pub kind: HirPatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirPatternKind {
    Wildcard,
    Binding(HirBinding),
    Int(u64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExprKind {
    Binding(HirBindingId),
    Item(HirItemId),
    HostBuiltin(HostBuiltin),
    Int(u64),
    Bool(bool),
    Tuple(Vec<HirExpr>),
    Array(Vec<HirExpr>),
    Block(HirBlock),
    Unary {
        op: UnaryOp,
        expr: Box<HirExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    Recover {
        expr: Box<HirExpr>,
        error_binding: Option<HirBinding>,
        fallback: Box<HirExpr>,
    },
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    Index {
        target: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    UnsafeMarker {
        marker: HirMarkerFamily,
        args: Vec<HirExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirMarkerFamily {
    True,
    False,
    Equal,
    LessThan,
    GreaterThan,
    LessOrEqual,
    GreaterOrEqual,
    MemberOf,
    Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirMarkerRequirement {
    pub family: HirMarkerFamily,
    pub args: Vec<HirMarkerPlace>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirMarkerPlace {
    Subject,
    Binding(HirBindingId),
    U32(u64),
    Bool(bool),
    ArrayLength(HirBindingId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirType {
    Unit,
    Bool,
    U32,
    ArithmeticError,
    Tuple(Vec<HirType>),
    Array {
        element: Box<HirType>,
        length: u64,
    },
    Option(Box<HirType>),
    Result {
        ok: Box<HirType>,
        err: Box<HirType>,
    },
    Set {
        element: Box<HirType>,
        capacity: u64,
    },
    Map {
        key: Box<HirType>,
        value: Box<HirType>,
        capacity: u64,
    },
    Range(Box<HirType>),
    Named(String),
    Function(HirFunctionType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunctionType {
    pub params: Vec<HirType>,
    pub return_type: Box<HirType>,
}

pub(crate) fn lower_program(
    parsed: &ParsedModule,
    resolutions: &[ResolvedName],
    types: &TypeFacts,
) -> HirProgram {
    let lowerer = HirLowerer::new(resolutions, types);
    HirProgram {
        functions: parsed
            .module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some(lowerer.lower_function(function)),
                Item::Task(task) => Some(lowerer.lower_task(task)),
                Item::MarkerRule(_) => None,
            })
            .collect(),
        marker_rules: parsed
            .module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::MarkerRule(rule) => Some(lowerer.lower_marker_rule(rule)),
                Item::Function(_) | Item::Task(_) => None,
            })
            .collect(),
    }
}

struct HirLowerer<'a> {
    resolutions: HashMap<Span, &'a ResolvedName>,
    types: &'a TypeFacts,
}

impl<'a> HirLowerer<'a> {
    fn new(resolutions: &'a [ResolvedName], types: &'a TypeFacts) -> Self {
        Self {
            resolutions: resolutions
                .iter()
                .map(|resolution| (resolution.use_span, resolution))
                .collect(),
            types,
        }
    }

    fn lower_function(&self, function: &Function) -> HirFunction {
        HirFunction {
            kind: HirFunctionKind::Function,
            id: HirItemId {
                declaration_span: function.name.span,
            },
            name: function.name.value.clone(),
            params: function
                .params
                .iter()
                .map(|param| {
                    self.lower_named_binding_with_type(
                        &param.name,
                        BindingKind::Param,
                        false,
                        Some(&param.ty),
                    )
                })
                .collect(),
            return_type: function
                .return_type
                .as_ref()
                .map(lower_ast_type)
                .unwrap_or(HirType::Unit),
            return_markers: function
                .return_type
                .as_ref()
                .map(|ty| self.lower_marker_requirements(ty))
                .unwrap_or_default(),
            body: self.lower_block(&function.body),
            span: function.span,
        }
    }

    fn lower_task(&self, task: &Task) -> HirFunction {
        HirFunction {
            kind: HirFunctionKind::Task,
            id: HirItemId {
                declaration_span: task.name.span,
            },
            name: task.name.value.clone(),
            params: task
                .params
                .iter()
                .map(|param| {
                    self.lower_named_binding_with_type(
                        &param.name,
                        BindingKind::Param,
                        false,
                        Some(&param.ty),
                    )
                })
                .collect(),
            return_type: lower_ast_type(&task.return_type),
            return_markers: self.lower_marker_requirements(&task.return_type),
            body: self.lower_block(&task.body),
            span: task.span,
        }
    }

    fn lower_marker_rule(&self, rule: &MarkerRule) -> HirMarkerRule {
        HirMarkerRule {
            name: rule.name.value.clone(),
            params: rule
                .params
                .iter()
                .map(|param| HirMarkerRuleParam {
                    name: param.name.value.clone(),
                    span: param.span,
                })
                .collect(),
            body: self.lower_marker_rule_block(&rule.body),
            span: rule.span,
        }
    }

    fn lower_marker_rule_block(&self, block: &MarkerRuleBlock) -> HirMarkerRuleBlock {
        HirMarkerRuleBlock {
            statements: block
                .statements
                .iter()
                .map(|statement| self.lower_marker_rule_statement(statement))
                .collect(),
            span: block.span,
        }
    }

    fn lower_marker_rule_statement(&self, statement: &MarkerRuleStmt) -> HirMarkerRuleStmt {
        match statement {
            MarkerRuleStmt::If(stmt) => HirMarkerRuleStmt::If(HirMarkerRuleIfStmt {
                refinement: self.lower_marker_refinement(&stmt.refinement),
                body: self.lower_marker_rule_block(&stmt.body),
                span: stmt.span,
            }),
            MarkerRuleStmt::Implies(stmt) => {
                HirMarkerRuleStmt::Implies(self.lower_marker_implication(stmt))
            }
        }
    }

    fn lower_marker_refinement(&self, refinement: &MarkerRefinement) -> HirMarkerRefinement {
        HirMarkerRefinement {
            subject: refinement.subject.value.clone(),
            marker: self.lower_marker_template(&refinement.marker),
            span: refinement.span,
        }
    }

    fn lower_marker_implication(
        &self,
        implication: &MarkerImplicationStmt,
    ) -> HirMarkerImplication {
        HirMarkerImplication {
            marker: self.lower_marker_template(&implication.marker),
            target: implication.target.value.clone(),
            span: implication.span,
        }
    }

    fn lower_marker_template(&self, marker: &MarkerAnnotation) -> HirMarkerTemplate {
        HirMarkerTemplate {
            family: lower_marker_family(&marker.name.value)
                .expect("checked marker rules must use builtin marker families"),
            args: marker
                .args
                .iter()
                .map(|arg| self.lower_marker_template_arg(arg))
                .collect(),
            span: marker.span,
        }
    }

    fn lower_marker_template_arg(&self, arg: &MarkerArg) -> HirMarkerTemplateArg {
        match &arg.kind {
            MarkerArgKind::Name(name) => HirMarkerTemplateArg::Place(name.value.clone()),
            MarkerArgKind::PatternBinding(name) => {
                HirMarkerTemplateArg::Binding(name.value.clone())
            }
            MarkerArgKind::Field { base, field } => {
                HirMarkerTemplateArg::Place(format!("{}.{}", base.value, field.value))
            }
            MarkerArgKind::Int(value) => HirMarkerTemplateArg::U32(*value),
            MarkerArgKind::Bool(value) => HirMarkerTemplateArg::Bool(*value),
        }
    }

    fn lower_block(&self, block: &Block) -> HirBlock {
        HirBlock {
            statements: block
                .statements
                .iter()
                .map(|statement| self.lower_statement(statement))
                .collect(),
            result: block
                .trailing_expr
                .as_deref()
                .map(|expr| Box::new(self.lower_expr(expr))),
            span: block.span,
        }
    }

    fn lower_statement(&self, statement: &Stmt) -> HirStmt {
        match statement {
            Stmt::Let(stmt) => HirStmt::Let(HirLetStmt {
                binding: self.lower_named_binding_with_type(
                    &stmt.name,
                    BindingKind::Local,
                    stmt.mutable,
                    stmt.ty.as_ref(),
                ),
                annotation: stmt.ty.as_ref().map(lower_ast_type),
                value: stmt.value.as_ref().map(|expr| self.lower_expr(expr)),
                span: stmt.span,
            }),
            Stmt::Assign(stmt) => HirStmt::Assign(HirAssignStmt {
                target: self.lower_expr(&stmt.target),
                value: self.lower_expr(&stmt.value),
                span: stmt.span,
            }),
            Stmt::Expr(stmt) => HirStmt::Expr(HirExprStmt {
                expr: self.lower_expr(&stmt.expr),
                span: stmt.span,
            }),
            Stmt::If(stmt) => HirStmt::If(HirIfStmt {
                condition: self.lower_expr(&stmt.condition),
                then_block: self.lower_block(&stmt.then_block),
                else_branch: stmt
                    .else_branch
                    .as_ref()
                    .map(|branch| self.lower_else_branch(branch)),
                span: stmt.span,
            }),
            Stmt::Match(stmt) => HirStmt::Match(HirMatchStmt {
                expr: self.lower_expr(&stmt.expr),
                arms: stmt
                    .arms
                    .iter()
                    .map(|arm| HirMatchArm {
                        pattern: self.lower_pattern(&arm.pattern),
                        body: self.lower_match_body(&arm.body),
                        span: arm.span,
                    })
                    .collect(),
                span: stmt.span,
            }),
            Stmt::For(stmt) => HirStmt::For(HirForStmt {
                binding: self.lower_pattern(&stmt.binding),
                iterable: self.lower_expr(&stmt.iterable),
                body: self.lower_block(&stmt.body),
                span: stmt.span,
            }),
            Stmt::Return(stmt) => HirStmt::Return(HirReturnStmt {
                value: stmt.value.as_ref().map(|expr| self.lower_expr(expr)),
                span: stmt.span,
            }),
            Stmt::Forever(stmt) => HirStmt::Forever(HirForeverStmt {
                body: self.lower_block(&stmt.body),
                span: stmt.span,
            }),
            Stmt::Exit(stmt) => HirStmt::Exit(HirExitStmt {
                value: self.lower_expr(&stmt.value),
                span: stmt.span,
            }),
            Stmt::Delegate(stmt) => {
                let resolution = self
                    .resolutions
                    .get(&stmt.target.span)
                    .expect("checked delegate targets must be resolved");
                HirStmt::Delegate(HirDelegateStmt {
                    target: HirItemId {
                        declaration_span: resolution.declaration_span,
                    },
                    args: stmt.args.iter().map(|arg| self.lower_expr(arg)).collect(),
                    span: stmt.span,
                })
            }
            Stmt::Observe(stmt) => HirStmt::Observe(HirObserveStmt {
                left: self.lower_expr(&stmt.left),
                op: stmt.op,
                right: self.lower_expr(&stmt.right),
                else_block: self.lower_block(&stmt.else_block),
                span: stmt.span,
            }),
            Stmt::UnsafeMarker(stmt) => HirStmt::UnsafeMarker(HirUnsafeMarkerStmt {
                marker: lower_marker_family(&stmt.construction.marker.value)
                    .expect("checked marker statements must name a builtin marker"),
                args: stmt
                    .construction
                    .args
                    .iter()
                    .map(|arg| self.lower_expr(arg))
                    .collect(),
                span: stmt.span,
            }),
        }
    }

    fn lower_else_branch(&self, branch: &ElseBranch) -> HirElseBranch {
        match branch {
            ElseBranch::Block(block) => HirElseBranch::Block(self.lower_block(block)),
            ElseBranch::If(stmt) => HirElseBranch::If(Box::new(HirIfStmt {
                condition: self.lower_expr(&stmt.condition),
                then_block: self.lower_block(&stmt.then_block),
                else_branch: stmt
                    .else_branch
                    .as_ref()
                    .map(|nested| self.lower_else_branch(nested)),
                span: stmt.span,
            })),
        }
    }

    fn lower_match_body(&self, body: &MatchBody) -> HirMatchBody {
        match body {
            MatchBody::Block(block) => HirMatchBody::Block(self.lower_block(block)),
            MatchBody::Expr(expr) => HirMatchBody::Expr(self.lower_expr(expr)),
        }
    }

    fn lower_pattern(&self, pattern: &Pattern) -> HirPattern {
        HirPattern {
            kind: match &pattern.kind {
                PatternKind::Wildcard => HirPatternKind::Wildcard,
                PatternKind::Binding(name) => HirPatternKind::Binding(self.lower_named_binding(
                    name,
                    BindingKind::Local,
                    false,
                )),
                PatternKind::Int(value) => HirPatternKind::Int(*value),
                PatternKind::Bool(value) => HirPatternKind::Bool(*value),
            },
            span: pattern.span,
        }
    }

    fn lower_expr(&self, expr: &Expr) -> HirExpr {
        let ty = self.lower_expr_type(expr.span);

        match &expr.kind {
            ExprKind::Int(value) => HirExpr {
                kind: HirExprKind::Int(*value),
                ty,
                span: expr.span,
            },
            ExprKind::Bool(value) => HirExpr {
                kind: HirExprKind::Bool(*value),
                ty,
                span: expr.span,
            },
            ExprKind::Name(name) => {
                let resolution = self
                    .resolutions
                    .get(&name.span)
                    .expect("checked HIR expressions must be resolved");
                let kind = match resolution.kind {
                    BindingKind::Item | BindingKind::TaskItem => HirExprKind::Item(HirItemId {
                        declaration_span: resolution.declaration_span,
                    }),
                    BindingKind::HostBuiltin => HirExprKind::HostBuiltin(
                        HostBuiltin::from_name(&resolution.name)
                            .expect("host builtin resolution should name a host builtin"),
                    ),
                    BindingKind::Param | BindingKind::Local => HirExprKind::Binding(HirBindingId {
                        declaration_span: resolution.declaration_span,
                    }),
                };
                HirExpr {
                    kind,
                    ty,
                    span: expr.span,
                }
            }
            ExprKind::Tuple(elements) => HirExpr {
                kind: HirExprKind::Tuple(
                    elements
                        .iter()
                        .map(|element| self.lower_expr(element))
                        .collect(),
                ),
                ty,
                span: expr.span,
            },
            ExprKind::Array(elements) => HirExpr {
                kind: HirExprKind::Array(
                    elements
                        .iter()
                        .map(|element| self.lower_expr(element))
                        .collect(),
                ),
                ty,
                span: expr.span,
            },
            ExprKind::Block(block) => HirExpr {
                kind: HirExprKind::Block(self.lower_block(block)),
                ty,
                span: expr.span,
            },
            ExprKind::Unary { op, expr: operand } => HirExpr {
                kind: HirExprKind::Unary {
                    op: *op,
                    expr: Box::new(self.lower_expr(operand)),
                },
                ty,
                span: expr.span,
            },
            ExprKind::Binary { op, left, right } => HirExpr {
                kind: HirExprKind::Binary {
                    op: *op,
                    left: Box::new(self.lower_expr(left)),
                    right: Box::new(self.lower_expr(right)),
                },
                ty,
                span: expr.span,
            },
            ExprKind::Recover {
                expr: target,
                error_binding,
                fallback,
            } => HirExpr {
                kind: HirExprKind::Recover {
                    expr: Box::new(self.lower_expr(target)),
                    error_binding: error_binding.as_ref().map(|binding| {
                        self.lower_named_binding(binding, BindingKind::Local, false)
                    }),
                    fallback: Box::new(self.lower_expr(fallback)),
                },
                ty,
                span: expr.span,
            },
            ExprKind::Call { callee, args } => HirExpr {
                kind: HirExprKind::Call {
                    callee: Box::new(self.lower_expr(callee)),
                    args: args.iter().map(|arg| self.lower_expr(arg)).collect(),
                },
                ty,
                span: expr.span,
            },
            ExprKind::Index { target, index } => HirExpr {
                kind: HirExprKind::Index {
                    target: Box::new(self.lower_expr(target)),
                    index: Box::new(self.lower_expr(index)),
                },
                ty,
                span: expr.span,
            },
            ExprKind::UnsafeMarker(construction) => HirExpr {
                kind: HirExprKind::UnsafeMarker {
                    marker: lower_marker_family(&construction.marker.value)
                        .expect("checked marker expressions must name a builtin marker"),
                    args: construction
                        .args
                        .iter()
                        .map(|arg| self.lower_expr(arg))
                        .collect(),
                },
                ty,
                span: expr.span,
            },
            ExprKind::MarkerRefinement { .. } => {
                unreachable!("marker refinements are rejected before HIR lowering")
            }
            ExprKind::Grouped(inner) => {
                let mut lowered = self.lower_expr(inner);
                lowered.span = expr.span;
                lowered.ty = ty;
                lowered
            }
        }
    }

    fn lower_named_binding(
        &self,
        name: &Spanned<String>,
        kind: BindingKind,
        mutable: bool,
    ) -> HirBinding {
        self.lower_named_binding_with_type(name, kind, mutable, None)
    }

    fn lower_named_binding_with_type(
        &self,
        name: &Spanned<String>,
        kind: BindingKind,
        mutable: bool,
        ty: Option<&Type>,
    ) -> HirBinding {
        HirBinding {
            id: HirBindingId {
                declaration_span: name.span,
            },
            name: name.value.clone(),
            kind,
            mutable,
            ty: self.lower_binding_type(name.span),
            markers: ty
                .map(|ty| self.lower_marker_requirements(ty))
                .unwrap_or_default(),
            span: name.span,
        }
    }

    fn lower_expr_type(&self, span: Span) -> HirType {
        let ty = self
            .types
            .expr_type(span)
            .expect("checked HIR expressions must carry a type");
        lower_semantic_type(ty).expect("checked HIR expressions must not contain unknown types")
    }

    fn lower_binding_type(&self, span: Span) -> HirType {
        let ty = self
            .types
            .binding_type(span)
            .expect("checked HIR bindings must carry a type");
        lower_semantic_type(ty).expect("checked HIR bindings must not contain unknown types")
    }

    fn lower_marker_requirements(&self, ty: &Type) -> Vec<HirMarkerRequirement> {
        let TypeKind::With { markers, .. } = &ty.kind else {
            return Vec::new();
        };

        markers
            .iter()
            .filter_map(|marker| self.lower_marker_requirement(marker))
            .collect()
    }

    fn lower_marker_requirement(&self, marker: &MarkerAnnotation) -> Option<HirMarkerRequirement> {
        let family = lower_marker_family(&marker.name.value)?;
        let args = normalize_marker_places(
            family,
            marker
                .args
                .iter()
                .filter_map(|arg| self.lower_marker_arg(arg))
                .collect(),
        );
        Some(HirMarkerRequirement {
            family,
            args,
            span: marker.span,
        })
    }

    fn lower_marker_arg(&self, arg: &MarkerArg) -> Option<HirMarkerPlace> {
        match &arg.kind {
            MarkerArgKind::Name(name) => self.marker_binding_place(name.span),
            MarkerArgKind::PatternBinding(_) => None,
            MarkerArgKind::Field { base, field } if field.value == "length" => self
                .marker_binding_place(base.span)
                .and_then(|place| match place {
                    HirMarkerPlace::Binding(binding) => Some(HirMarkerPlace::ArrayLength(binding)),
                    _ => None,
                }),
            MarkerArgKind::Field { .. } => None,
            MarkerArgKind::Int(value) => Some(HirMarkerPlace::U32(*value)),
            MarkerArgKind::Bool(value) => Some(HirMarkerPlace::Bool(*value)),
        }
    }

    fn marker_binding_place(&self, span: Span) -> Option<HirMarkerPlace> {
        let resolution = self.resolutions.get(&span)?;
        matches!(resolution.kind, BindingKind::Param | BindingKind::Local).then_some(
            HirMarkerPlace::Binding(HirBindingId {
                declaration_span: resolution.declaration_span,
            }),
        )
    }
}

fn lower_ast_type(ty: &Type) -> HirType {
    let semantic = lower_type(ty);
    lower_semantic_type(&semantic).expect("surface types must lower into HIR types")
}

pub fn lower_marker_family(name: &str) -> Option<HirMarkerFamily> {
    match name {
        "True" => Some(HirMarkerFamily::True),
        "False" => Some(HirMarkerFamily::False),
        "Equal" => Some(HirMarkerFamily::Equal),
        "LessThan" => Some(HirMarkerFamily::LessThan),
        "GreaterThan" => Some(HirMarkerFamily::GreaterThan),
        "LessOrEqual" => Some(HirMarkerFamily::LessOrEqual),
        "GreaterOrEqual" => Some(HirMarkerFamily::GreaterOrEqual),
        "MemberOf" => Some(HirMarkerFamily::MemberOf),
        "Event" => Some(HirMarkerFamily::Event),
        _ => None,
    }
}

fn normalize_marker_places(
    family: HirMarkerFamily,
    args: Vec<HirMarkerPlace>,
) -> Vec<HirMarkerPlace> {
    match family {
        HirMarkerFamily::Event => Vec::new(),
        HirMarkerFamily::True | HirMarkerFamily::False if args.is_empty() => {
            vec![HirMarkerPlace::Subject]
        }
        HirMarkerFamily::MemberOf if args.len() == 1 => {
            vec![HirMarkerPlace::Subject, args[0].clone()]
        }
        HirMarkerFamily::Equal
        | HirMarkerFamily::LessThan
        | HirMarkerFamily::GreaterThan
        | HirMarkerFamily::LessOrEqual
        | HirMarkerFamily::GreaterOrEqual
            if args.len() == 1 =>
        {
            vec![HirMarkerPlace::Subject, args[0].clone()]
        }
        _ => args,
    }
}

fn lower_semantic_type(ty: &SemanticType) -> Option<HirType> {
    if ty.contains_unknown() {
        return None;
    }

    Some(match ty {
        SemanticType::Unit => HirType::Unit,
        SemanticType::Bool => HirType::Bool,
        SemanticType::U32 => HirType::U32,
        SemanticType::ArithmeticError => HirType::ArithmeticError,
        SemanticType::Tuple(elements) => HirType::Tuple(
            elements
                .iter()
                .map(|element| lower_semantic_type(element).expect("tuple types must be known"))
                .collect(),
        ),
        SemanticType::Array { element, length } => HirType::Array {
            element: Box::new(
                lower_semantic_type(element).expect("array element types must be known"),
            ),
            length: *length,
        },
        SemanticType::Option(inner) => HirType::Option(Box::new(
            lower_semantic_type(inner).expect("option element types must be known"),
        )),
        SemanticType::Result { ok, err } => HirType::Result {
            ok: Box::new(lower_semantic_type(ok).expect("result ok types must be known")),
            err: Box::new(lower_semantic_type(err).expect("result err types must be known")),
        },
        SemanticType::Set { element, capacity } => HirType::Set {
            element: Box::new(
                lower_semantic_type(element).expect("set element types must be known"),
            ),
            capacity: *capacity,
        },
        SemanticType::Map {
            key,
            value,
            capacity,
        } => HirType::Map {
            key: Box::new(lower_semantic_type(key).expect("map key types must be known")),
            value: Box::new(lower_semantic_type(value).expect("map value types must be known")),
            capacity: *capacity,
        },
        SemanticType::Range(inner) => HirType::Range(Box::new(
            lower_semantic_type(inner).expect("range element types must be known"),
        )),
        SemanticType::Named(name) => HirType::Named(name.clone()),
        SemanticType::Function(signature) => {
            HirType::Function(lower_function_type(signature).expect("function types must be known"))
        }
        SemanticType::Unknown => return None,
    })
}

fn lower_function_type(signature: &FunctionType) -> Option<HirFunctionType> {
    Some(HirFunctionType {
        params: signature
            .params
            .iter()
            .map(lower_semantic_type)
            .collect::<Option<Vec<_>>>()?,
        return_type: Box::new(lower_semantic_type(&signature.return_type)?),
    })
}

#[cfg(test)]
mod requirement_tests;
