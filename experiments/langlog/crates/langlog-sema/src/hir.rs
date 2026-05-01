use std::collections::HashMap;

use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, Item, MatchBody, ObserveOp, Pattern,
    PatternKind, Stmt, Type, UnaryOp,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub id: HirItemId,
    pub name: String,
    pub params: Vec<HirBinding>,
    pub return_type: HirType,
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
    pub span: Span,
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
    Observe(HirObserveStmt),
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
            Self::Observe(stmt) => stmt.span,
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
pub struct HirObserveStmt {
    pub left: HirExpr,
    pub op: ObserveOp,
    pub right: HirExpr,
    pub else_block: HirBlock,
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
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    Index {
        target: Box<HirExpr>,
        index: Box<HirExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirType {
    Unit,
    Bool,
    U32,
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
            .map(|item| {
                let Item::Function(function) = item;
                lowerer.lower_function(function)
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
            id: HirItemId {
                declaration_span: function.name.span,
            },
            name: function.name.value.clone(),
            params: function
                .params
                .iter()
                .map(|param| self.lower_named_binding(&param.name, BindingKind::Param, false))
                .collect(),
            return_type: function
                .return_type
                .as_ref()
                .map(lower_ast_type)
                .unwrap_or(HirType::Unit),
            body: self.lower_block(&function.body),
            span: function.span,
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
                binding: self.lower_named_binding(&stmt.name, BindingKind::Local, stmt.mutable),
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
            Stmt::Observe(stmt) => HirStmt::Observe(HirObserveStmt {
                left: self.lower_expr(&stmt.left),
                op: stmt.op,
                right: self.lower_expr(&stmt.right),
                else_block: self.lower_block(&stmt.else_block),
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
                    BindingKind::Item => HirExprKind::Item(HirItemId {
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
        HirBinding {
            id: HirBindingId {
                declaration_span: name.span,
            },
            name: name.value.clone(),
            kind,
            mutable,
            ty: self.lower_binding_type(name.span),
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
}

fn lower_ast_type(ty: &Type) -> HirType {
    let semantic = lower_type(ty);
    lower_semantic_type(&semantic).expect("surface types must lower into HIR types")
}

fn lower_semantic_type(ty: &SemanticType) -> Option<HirType> {
    if ty.contains_unknown() {
        return None;
    }

    Some(match ty {
        SemanticType::Unit => HirType::Unit,
        SemanticType::Bool => HirType::Bool,
        SemanticType::U32 => HirType::U32,
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
