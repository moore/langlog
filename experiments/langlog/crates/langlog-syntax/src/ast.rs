use crate::span::{Span, Spanned};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Function(Function),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Self::Function(function) => function.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub span: Span,
    pub name: Spanned<String>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub span: Span,
    pub name: Spanned<String>,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub span: Span,
    pub statements: Vec<Stmt>,
    pub trailing_expr: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let(LetStmt),
    Assign(AssignStmt),
    Expr(ExprStmt),
    If(IfStmt),
    Match(MatchStmt),
    For(ForStmt),
    Return(ReturnStmt),
    Observe(ObserveStmt),
}

impl Stmt {
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
pub struct LetStmt {
    pub span: Span,
    pub mutable: bool,
    pub name: Spanned<String>,
    pub ty: Option<Type>,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignStmt {
    pub span: Span,
    pub target: Expr,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprStmt {
    pub span: Span,
    pub expr: Expr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfStmt {
    pub span: Span,
    pub condition: Expr,
    pub then_block: Block,
    pub else_branch: Option<ElseBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElseBranch {
    Block(Block),
    If(Box<IfStmt>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStmt {
    pub span: Span,
    pub expr: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub span: Span,
    pub pattern: Pattern,
    pub body: MatchBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchBody {
    Block(Block),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForStmt {
    pub span: Span,
    pub binding: Pattern,
    pub iterable: Expr,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnStmt {
    pub span: Span,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveStmt {
    pub span: Span,
    pub subject: Spanned<String>,
    pub op: ObserveOp,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserveOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub span: Span,
    pub kind: PatternKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Binding(Spanned<String>),
    Int(u64),
    Bool(bool),
}

impl Pattern {
    pub fn new(span: Span, kind: PatternKind) -> Self {
        Self { span, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

impl Expr {
    pub fn new(span: Span, kind: ExprKind) -> Self {
        Self { span, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    Int(u64),
    Bool(bool),
    Name(Spanned<String>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Block(Block),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Grouped(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Range,
    Or,
    And,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    pub span: Span,
    pub kind: TypeKind,
}

impl Type {
    pub fn new(span: Span, kind: TypeKind) -> Self {
        Self { span, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    Unit,
    Named(Spanned<String>),
    Tuple(Vec<Type>),
    Array {
        element: Box<Type>,
        length: Spanned<u64>,
    },
    Applied {
        base: Spanned<String>,
        args: Vec<GenericArg>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericArg {
    Type(Type),
    Const(Spanned<u64>),
}
