use langlog_sema::CheckedProgram;
use langlog_syntax::ast::{
    BinaryOp, Block, ElseBranch, Expr, ExprKind, Function, Item, MatchBody, ObserveOp, Stmt,
};
use langlog_syntax::{Diagnostic, Severity, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactSource {
    Observe,
    ControlFlow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofFact {
    pub source: FactSource,
    pub origin_span: Span,
    pub subject_name: String,
    pub subject_span: Span,
    pub op: ObserveOp,
    pub value_span: Span,
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
    let (diagnostics, facts) = {
        let mut checker = Checker::new(program);
        checker.check_module();
        (checker.diagnostics, checker.facts)
    };

    CheckedProof {
        obligations: 0,
        observations: facts.len(),
        diagnostics,
        facts,
    }
}

struct Checker<'a> {
    program: &'a CheckedProgram,
    diagnostics: Vec<Diagnostic>,
    facts: Vec<ProofFact>,
}

impl<'a> Checker<'a> {
    fn new(program: &'a CheckedProgram) -> Self {
        Self {
            program,
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
        self.check_block(&function.body);
    }

    fn check_block(&mut self, block: &Block) {
        for statement in &block.statements {
            self.check_statement(statement);
        }
        if let Some(expr) = &block.trailing_expr {
            self.check_expr(expr);
        }
    }

    fn check_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let(stmt) => {
                if let Some(value) = &stmt.value {
                    self.check_expr(value);
                }
            }
            Stmt::Assign(stmt) => {
                self.check_expr(&stmt.target);
                self.check_expr(&stmt.value);
            }
            Stmt::Expr(stmt) => {
                self.check_expr(&stmt.expr);
            }
            Stmt::If(stmt) => {
                self.record_control_flow_facts(&stmt.condition);
                self.check_expr(&stmt.condition);
                self.check_block(&stmt.then_block);
                if let Some(else_branch) = &stmt.else_branch {
                    match else_branch {
                        ElseBranch::Block(block) => self.check_block(block),
                        ElseBranch::If(stmt) => self.check_statement(&Stmt::If(*stmt.clone())),
                    }
                }
            }
            Stmt::Match(stmt) => {
                self.check_expr(&stmt.expr);
                for arm in &stmt.arms {
                    match &arm.body {
                        MatchBody::Block(block) => self.check_block(block),
                        MatchBody::Expr(expr) => self.check_expr(expr),
                    }
                }
            }
            Stmt::For(stmt) => {
                self.check_expr(&stmt.iterable);
                self.check_block(&stmt.body);
            }
            Stmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.check_expr(value);
                }
            }
            Stmt::Observe(stmt) => {
                self.facts.push(ProofFact {
                    source: FactSource::Observe,
                    origin_span: stmt.span,
                    subject_name: stmt.subject.value.clone(),
                    subject_span: stmt.subject.span,
                    op: stmt.op,
                    value_span: stmt.value.span,
                });
                self.check_expr(&stmt.value);
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Name(_) => {}
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.check_expr(element);
                }
            }
            ExprKind::Block(block) => {
                self.check_block(block);
            }
            ExprKind::Unary { expr, .. } | ExprKind::Grouped(expr) => {
                self.check_expr(expr);
            }
            ExprKind::Binary { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            ExprKind::Call { callee, args } => {
                self.check_expr(callee);
                for arg in args {
                    self.check_expr(arg);
                }
            }
            ExprKind::Index { target, index } => {
                self.check_expr(target);
                self.check_expr(index);
            }
        }
    }

    fn record_control_flow_facts(&mut self, condition: &Expr) {
        match &condition.kind {
            ExprKind::Grouped(expr) => self.record_control_flow_facts(expr),
            ExprKind::Binary {
                op: BinaryOp::And,
                left,
                right,
            } => {
                self.record_control_flow_facts(left);
                self.record_control_flow_facts(right);
            }
            ExprKind::Binary { op, left, right } => {
                let Some(op) = comparison_to_observe_op(*op) else {
                    return;
                };
                let Some(subject) = bare_name(left) else {
                    return;
                };
                self.facts.push(ProofFact {
                    source: FactSource::ControlFlow,
                    origin_span: condition.span,
                    subject_name: subject.value.clone(),
                    subject_span: subject.span,
                    op,
                    value_span: right.span,
                });
            }
            _ => {}
        }
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

fn bare_name(expr: &Expr) -> Option<&langlog_syntax::Spanned<String>> {
    match &expr.kind {
        ExprKind::Name(name) => Some(name),
        ExprKind::Grouped(expr) => bare_name(expr),
        _ => None,
    }
}
