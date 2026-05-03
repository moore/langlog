use std::collections::HashMap;

use langlog_sema::{
    CheckedProgram, HirBindingId, HirBlock, HirElseBranch, HirExpr, HirExprKind, HirForStmt,
    HirFunction, HirItemId, HirMatchBody, HirMatchStmt, HirPatternKind, HirProgram, HirStmt,
    HirType, HostBuiltin,
};
use langlog_syntax::ast::{BinaryOp, ObserveOp, UnaryOp};
use langlog_syntax::{Diagnostic, Label, Span};

const ARITHMETIC_OVERFLOW: u32 = 1;
const ARITHMETIC_UNDERFLOW: u32 = 2;
const DIVIDE_BY_ZERO: u32 = 3;
const REMAINDER_BY_ZERO: u32 = 4;
const OPTION_NONE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmModule {
    pub wat: String,
    pub wasm: Vec<u8>,
}

pub fn compile(program: &CheckedProgram) -> Result<WasmModule, Vec<Diagnostic>> {
    let Some(hir) = program.hir.as_ref() else {
        return Err(vec![Diagnostic::error(
            "cannot compile Wasm for a program with semantic errors",
        )]);
    };

    let mut compiler = Compiler::new(hir);
    let wat = compiler.compile_program();
    if !compiler.diagnostics.is_empty() {
        return Err(compiler.diagnostics);
    }

    match wat::parse_str(&wat) {
        Ok(wasm) => Ok(WasmModule { wat, wasm }),
        Err(error) => Err(vec![Diagnostic::error(format!(
            "internal Wasm generation error: {error}"
        ))]),
    }
}

struct Compiler<'a> {
    program: &'a HirProgram,
    function_indices: HashMap<HirItemId, usize>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Compiler<'a> {
    fn new(program: &'a HirProgram) -> Self {
        let function_indices = program
            .functions
            .iter()
            .enumerate()
            .map(|(index, function)| (function.id, index))
            .collect();
        Self {
            program,
            function_indices,
            diagnostics: Vec::new(),
        }
    }

    fn compile_program(&mut self) -> String {
        let mut module = String::from("(module\n");
        let Some(main) = self
            .program
            .functions
            .iter()
            .find(|function| function.name == "main")
        else {
            self.error(None, "Wasm build requires a `main` function");
            module.push_str(")\n");
            return module;
        };
        if !main.params.is_empty() || main.return_type != HirType::U32 {
            self.error(
                Some(main.span),
                "Wasm build requires `fn main() -> u32` for backend v1",
            );
        }

        for builtin in used_host_builtins(self.program) {
            module.push_str(host_builtin_import(builtin));
        }

        for function in &self.program.functions {
            let mut body = FunctionCompiler::new(self, function);
            module.push_str(&body.compile_function(function.name == "main"));
        }

        module.push_str(")\n");
        module
    }

    fn function_index(&self, id: HirItemId) -> Option<usize> {
        self.function_indices.get(&id).copied()
    }

    fn error(&mut self, span: Option<Span>, message: impl Into<String>) {
        let message = message.into();
        let diagnostic = if let Some(span) = span {
            Diagnostic::error(message).with_label(Label::primary(span, "unsupported for Wasm v1"))
        } else {
            Diagnostic::error(message)
        };
        self.diagnostics.push(diagnostic);
    }
}

struct FunctionCompiler<'a, 'b> {
    compiler: &'a mut Compiler<'b>,
    function: &'b HirFunction,
    locals: HashMap<HirBindingId, LocalValue>,
    param_count: u32,
    next_local: u32,
    code: Vec<String>,
}

#[derive(Debug, Clone)]
struct LocalValue {
    slots: Vec<u32>,
}

impl<'a, 'b> FunctionCompiler<'a, 'b> {
    fn new(compiler: &'a mut Compiler<'b>, function: &'b HirFunction) -> Self {
        let mut locals = HashMap::new();
        let mut next_param = 0;
        for param in &function.params {
            let width = value_width(&param.ty).unwrap_or(1);
            let slots = (next_param..next_param + width).collect::<Vec<_>>();
            next_param += width;
            locals.insert(param.id, LocalValue { slots });
        }
        let param_count = next_param;
        Self {
            compiler,
            function,
            locals,
            param_count,
            next_local: param_count,
            code: Vec::new(),
        }
    }

    fn compile_function(&mut self, export: bool) -> String {
        let function = self.function;
        let function_index = self
            .compiler
            .function_index(function.id)
            .expect("function should have an index");
        let params = function
            .params
            .iter()
            .fold(String::new(), |mut params, param| {
                match supported_value_width(&param.ty) {
                    Some(width) => {
                        for _ in 0..width {
                            params.push_str(" (param i32)");
                        }
                    }
                    None => self.unsupported(
                        param.span,
                        "only scalar and fixed-size scalar aggregate parameters compile to Wasm v1",
                    ),
                }
                params
            });
        let result = match wasm_return_width(&function.return_type) {
            Some(width) => wasm_result_signature(width),
            None => {
                self.unsupported(
                    function.span,
                    "only `u32`, `bool`, `ArithmeticError`, scalar `Option`, scalar `Result`, and `()` returns compile to Wasm v1",
                );
                String::new()
            }
        };

        self.compile_block(&function.body, &function.return_type);
        if wasm_return_width(&function.return_type).unwrap_or(0) > 0 {
            if let Some(result) = &function.body.result {
                self.compile_expr(result);
            } else {
                self.emit_default_value(&function.return_type);
            }
        }

        let mut rendered = format!("  (func $f{function_index}{params}{result}\n");
        for local in self.param_count..self.next_local {
            let _ = local;
            rendered.push_str("    (local i32)\n");
        }
        for line in &self.code {
            rendered.push_str("    ");
            rendered.push_str(line);
            rendered.push('\n');
        }
        rendered.push_str("  )\n");
        if export {
            rendered.push_str(&format!("  (export \"main\" (func $f{function_index}))\n"));
        }
        rendered
    }

    fn compile_block(&mut self, block: &HirBlock, return_type: &HirType) {
        for statement in &block.statements {
            self.compile_stmt(statement, return_type);
        }
    }

    fn compile_stmt(&mut self, statement: &HirStmt, return_type: &HirType) {
        match statement {
            HirStmt::Let(stmt) => {
                self.ensure_wasm_type(&stmt.binding.ty, stmt.span);
                let local = self.allocate_local(stmt.binding.id, &stmt.binding.ty);
                if let Some(value) = &stmt.value {
                    self.compile_expr(value);
                    self.store_slots(&local.slots);
                }
            }
            HirStmt::Assign(stmt) => match &stmt.target.kind {
                HirExprKind::Binding(id) => {
                    self.compile_expr(&stmt.value);
                    let Some(local) = self.locals.get(id).cloned() else {
                        self.unsupported(stmt.span, "assignment target is not a Wasm local");
                        return;
                    };
                    self.store_slots(&local.slots);
                }
                _ => self.unsupported(stmt.span, "only local assignments compile to Wasm v1"),
            },
            HirStmt::Expr(stmt) => {
                self.compile_expr(&stmt.expr);
                self.drop_value(&stmt.expr.ty);
            }
            HirStmt::If(stmt) => {
                self.compile_expr(&stmt.condition);
                self.emit("if");
                self.compile_block(&stmt.then_block, return_type);
                if let Some(else_branch) = &stmt.else_branch {
                    self.emit("else");
                    self.compile_else_branch(else_branch, return_type);
                }
                self.emit("end");
            }
            HirStmt::Return(stmt) => {
                if let Some(value) = &stmt.value {
                    self.compile_expr(value);
                } else if wasm_return_width(return_type).unwrap_or(0) > 0 {
                    self.emit_default_value(return_type);
                }
                self.emit("return");
            }
            HirStmt::Match(stmt) => self.compile_match_stmt(stmt, return_type),
            HirStmt::For(stmt) => self.compile_for_stmt(stmt, return_type),
            HirStmt::Observe(stmt) => {
                self.compile_expr(&stmt.left);
                self.compile_expr(&stmt.right);
                self.emit(observe_instruction(stmt.op));
                self.emit("i32.eqz");
                self.emit("if");
                self.compile_block(&stmt.else_block, return_type);
                self.emit("end");
            }
        }
    }

    fn compile_else_branch(&mut self, branch: &HirElseBranch, return_type: &HirType) {
        match branch {
            HirElseBranch::Block(block) => self.compile_block(block, return_type),
            HirElseBranch::If(stmt) => {
                self.compile_expr(&stmt.condition);
                self.emit("if");
                self.compile_block(&stmt.then_block, return_type);
                if let Some(else_branch) = &stmt.else_branch {
                    self.emit("else");
                    self.compile_else_branch(else_branch, return_type);
                }
                self.emit("end");
            }
        }
    }

    fn compile_expr(&mut self, expr: &HirExpr) {
        match &expr.kind {
            HirExprKind::Int(value) => self.emit(format!("i32.const {value}")),
            HirExprKind::Bool(value) => self.emit(format!("i32.const {}", u8::from(*value))),
            HirExprKind::Binding(id) => {
                if let Some(local) = self.locals.get(id).cloned() {
                    for slot in local.slots {
                        self.emit(format!("local.get {slot}"));
                    }
                } else {
                    self.unsupported(expr.span, "unmapped local binding");
                }
            }
            HirExprKind::Item(_) => self.unsupported(
                expr.span,
                "function item values are not supported by Wasm v1",
            ),
            HirExprKind::HostBuiltin(_) => self.unsupported(
                expr.span,
                "host builtin values are not supported by Wasm v1",
            ),
            HirExprKind::Unary { op, expr } => match op {
                UnaryOp::Neg => {
                    self.emit("i32.const 0");
                    self.compile_expr(expr);
                    self.emit("i32.sub");
                }
                UnaryOp::Not => {
                    self.compile_expr(expr);
                    self.emit("i32.eqz");
                }
            },
            HirExprKind::Binary { op, left, right } => {
                if is_checked_arithmetic(*op) {
                    self.compile_checked_arithmetic(*op, left, right);
                    return;
                }
                let Some(instruction) = binary_instruction(*op) else {
                    self.unsupported(
                        expr.span,
                        "this binary operator is not supported by Wasm v1",
                    );
                    return;
                };
                self.compile_expr(left);
                self.compile_expr(right);
                self.emit(instruction);
            }
            HirExprKind::Call { callee, args } => {
                for arg in args {
                    if supported_value_width(&arg.ty).is_none() {
                        self.unsupported(
                            arg.span,
                            "this argument type is not supported by Wasm v1",
                        );
                        continue;
                    }
                    self.compile_expr(arg);
                }
                match &callee.kind {
                    HirExprKind::Item(id) => {
                        if let Some(index) = self.compiler.function_index(*id) {
                            self.emit(format!("call $f{index}"));
                        } else {
                            self.unsupported(callee.span, "unknown callee");
                        }
                    }
                    HirExprKind::HostBuiltin(builtin) => {
                        self.compile_builtin_call(*builtin, args, &expr.ty);
                    }
                    _ => self
                        .unsupported(callee.span, "only direct function calls compile to Wasm v1"),
                }
            }
            HirExprKind::Block(block) => {
                self.compile_block(block, &expr.ty);
                if let Some(result) = &block.result {
                    self.compile_expr(result);
                } else if expr.ty == HirType::Unit {
                } else {
                    self.emit_default_value(&expr.ty);
                }
            }
            HirExprKind::Tuple(elements) | HirExprKind::Array(elements) => {
                for element in elements {
                    self.compile_expr(element);
                }
            }
            HirExprKind::Index { target, index } => {
                self.compile_index_expr(expr.span, target, index);
            }
            HirExprKind::Recover {
                expr: target,
                error_binding,
                fallback,
            } => {
                self.compile_recovery_expr(
                    expr.span,
                    target,
                    error_binding.as_ref(),
                    fallback,
                    &expr.ty,
                );
            }
        }
    }

    fn compile_builtin_call(&mut self, builtin: HostBuiltin, args: &[HirExpr], ty: &HirType) {
        match builtin {
            HostBuiltin::ReadU32
            | HostBuiltin::PrintU32
            | HostBuiltin::PrintBool
            | HostBuiltin::PrintNewline => {
                self.emit(format!("call ${}", host_builtin_symbol(builtin)));
            }
            HostBuiltin::Some | HostBuiltin::Ok => {
                if args.len() != 1 {
                    self.unsupported(
                        args.first().map_or(self.function.span, |arg| arg.span),
                        "invalid builtin arity",
                    );
                    return;
                }
                self.emit("i32.const 0");
            }
            HostBuiltin::None => {
                let HirType::Option(inner) = ty else {
                    self.unsupported(self.function.span, "`none` must have an Option type");
                    return;
                };
                self.emit_default_value(inner);
                self.emit(format!("i32.const {OPTION_NONE}"));
            }
            HostBuiltin::Err => {
                let err_local = self.allocate_scratch_local();
                self.emit(format!("local.set {err_local}"));
                let HirType::Result { ok, .. } = ty else {
                    self.unsupported(self.function.span, "`err` must have a Result type");
                    return;
                };
                self.emit_default_value(ok);
                self.emit(format!("local.get {err_local}"));
            }
            HostBuiltin::ArithmeticOverflow => {
                self.emit(format!("i32.const {ARITHMETIC_OVERFLOW}"))
            }
            HostBuiltin::ArithmeticUnderflow => {
                self.emit(format!("i32.const {ARITHMETIC_UNDERFLOW}"));
            }
            HostBuiltin::DivideByZero => self.emit(format!("i32.const {DIVIDE_BY_ZERO}")),
            HostBuiltin::RemainderByZero => self.emit(format!("i32.const {REMAINDER_BY_ZERO}")),
        }
    }

    fn compile_recovery_expr(
        &mut self,
        span: Span,
        target: &HirExpr,
        error_binding: Option<&langlog_sema::HirBinding>,
        fallback: &HirExpr,
        ty: &HirType,
    ) {
        if supported_value_width(ty) != Some(1) {
            self.unsupported(
                span,
                "recovery expressions currently require scalar fallback values",
            );
            return;
        }
        let Some(target_width) = supported_value_width(&target.ty) else {
            self.unsupported(target.span, "unsupported recovery target type");
            return;
        };
        if target_width != 2 {
            self.unsupported(
                target.span,
                "recovery targets must lower to value and tag slots",
            );
            return;
        }

        let value_local = self.allocate_scratch_local();
        let tag_local = self.allocate_scratch_local();
        self.compile_expr(target);
        self.emit(format!("local.set {tag_local}"));
        self.emit(format!("local.set {value_local}"));

        if let Some(binding) = error_binding {
            let local = self.allocate_local(binding.id, &binding.ty);
            self.emit(format!("local.get {tag_local}"));
            self.store_slots(&local.slots);
        }

        self.emit(format!("local.get {tag_local}"));
        self.emit("i32.eqz");
        self.emit("if (result i32)");
        self.emit(format!("local.get {value_local}"));
        self.emit("else");
        self.compile_expr(fallback);
        self.emit("end");
    }

    fn compile_checked_arithmetic(&mut self, op: BinaryOp, left: &HirExpr, right: &HirExpr) {
        let left_value = self.allocate_scratch_local();
        let left_tag = self.allocate_scratch_local();
        self.compile_operand_value_tag(left, left_value, left_tag);

        self.emit(format!("local.get {left_tag}"));
        self.emit("i32.eqz");
        self.emit("if (result i32 i32)");
        let right_value = self.allocate_scratch_local();
        let right_tag = self.allocate_scratch_local();
        self.compile_operand_value_tag(right, right_value, right_tag);
        self.emit(format!("local.get {right_tag}"));
        self.emit("i32.eqz");
        self.emit("if (result i32 i32)");
        self.compile_checked_scalar_operation(op, left_value, right_value);
        self.emit("else");
        self.emit("i32.const 0");
        self.emit(format!("local.get {right_tag}"));
        self.emit("end");
        self.emit("else");
        self.emit("i32.const 0");
        self.emit(format!("local.get {left_tag}"));
        self.emit("end");
    }

    fn compile_operand_value_tag(&mut self, expr: &HirExpr, value_local: u32, tag_local: u32) {
        match &expr.ty {
            HirType::Result { .. } => {
                self.compile_expr(expr);
                self.emit(format!("local.set {tag_local}"));
                self.emit(format!("local.set {value_local}"));
            }
            _ => {
                self.compile_expr(expr);
                self.emit(format!("local.set {value_local}"));
                self.emit("i32.const 0");
                self.emit(format!("local.set {tag_local}"));
            }
        }
    }

    fn compile_checked_scalar_operation(
        &mut self,
        op: BinaryOp,
        left_local: u32,
        right_local: u32,
    ) {
        match op {
            BinaryOp::Add => {
                self.emit(format!("local.get {left_local}"));
                self.emit("i32.const -1");
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.sub");
                self.emit("i32.gt_u");
                self.emit("if (result i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {ARITHMETIC_OVERFLOW}"));
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.add");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Sub => {
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.lt_u");
                self.emit("if (result i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {ARITHMETIC_UNDERFLOW}"));
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.sub");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Mul => {
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.eqz");
                self.emit("if (result i32)");
                self.emit("i32.const 0");
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit("i32.const -1");
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.div_u");
                self.emit("i32.gt_u");
                self.emit("end");
                self.emit("if (result i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {ARITHMETIC_OVERFLOW}"));
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.mul");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Div => {
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.eqz");
                self.emit("if (result i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {DIVIDE_BY_ZERO}"));
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.div_u");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Rem => {
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.eqz");
                self.emit("if (result i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {REMAINDER_BY_ZERO}"));
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.rem_u");
                self.emit("i32.const 0");
                self.emit("end");
            }
            _ => self.unsupported(
                self.function.span,
                "unsupported checked arithmetic operator",
            ),
        }
    }

    fn compile_for_stmt(&mut self, stmt: &HirForStmt, return_type: &HirType) {
        let HirType::Array { element, length } = &stmt.iterable.ty else {
            self.unsupported(stmt.span, "`for` currently requires an array iterable");
            return;
        };
        if !is_scalar_wasm_type(element) {
            self.unsupported(
                stmt.iterable.span,
                "`for` currently supports arrays of `u32` or `bool`",
            );
            return;
        }

        let binding = match &stmt.binding.kind {
            HirPatternKind::Binding(binding) => binding,
            HirPatternKind::Wildcard => {
                for index in 0..*length {
                    self.compile_array_element(&stmt.iterable, index);
                    self.emit("drop");
                    self.compile_block(&stmt.body, return_type);
                }
                return;
            }
            HirPatternKind::Int(_) | HirPatternKind::Bool(_) => {
                self.unsupported(
                    stmt.binding.span,
                    "`for` currently requires a binding pattern",
                );
                return;
            }
        };

        let local = self.allocate_local(binding.id, element);
        for index in 0..*length {
            self.compile_array_element(&stmt.iterable, index);
            self.store_slots(&local.slots);
            self.compile_block(&stmt.body, return_type);
        }
    }

    fn compile_match_stmt(&mut self, stmt: &HirMatchStmt, return_type: &HirType) {
        if !is_scalar_wasm_type(&stmt.expr.ty) {
            self.unsupported(
                stmt.expr.span,
                "`match` currently supports `u32` and `bool` scrutinees",
            );
            return;
        }

        let scrutinee = self.allocate_scratch_local();
        let matched = self.allocate_scratch_local();
        self.compile_expr(&stmt.expr);
        self.emit(format!("local.set {scrutinee}"));
        self.emit("i32.const 0");
        self.emit(format!("local.set {matched}"));

        for arm in &stmt.arms {
            self.emit(format!("local.get {matched}"));
            self.emit("i32.eqz");
            match &arm.pattern.kind {
                HirPatternKind::Int(value) => {
                    self.emit(format!("local.get {scrutinee}"));
                    self.emit(format!("i32.const {value}"));
                    self.emit("i32.eq");
                    self.emit("i32.and");
                }
                HirPatternKind::Bool(value) => {
                    self.emit(format!("local.get {scrutinee}"));
                    self.emit(format!("i32.const {}", u8::from(*value)));
                    self.emit("i32.eq");
                    self.emit("i32.and");
                }
                HirPatternKind::Wildcard | HirPatternKind::Binding(_) => {}
            }
            self.emit("if");
            self.emit("i32.const 1");
            self.emit(format!("local.set {matched}"));
            if let HirPatternKind::Binding(binding) = &arm.pattern.kind {
                let local = self.allocate_local(binding.id, &binding.ty);
                self.emit(format!("local.get {scrutinee}"));
                self.store_slots(&local.slots);
            }
            self.compile_match_body(&arm.body, return_type);
            self.emit("end");
        }
    }

    fn compile_match_body(&mut self, body: &HirMatchBody, return_type: &HirType) {
        match body {
            HirMatchBody::Block(block) => self.compile_block(block, return_type),
            HirMatchBody::Expr(expr) => {
                self.compile_expr(expr);
                self.drop_value(&expr.ty);
            }
        }
    }

    fn compile_index_expr(&mut self, span: Span, target: &HirExpr, index: &HirExpr) {
        let HirType::Array { element, length } = &target.ty else {
            self.unsupported(span, "only array indexing compiles to Wasm v1");
            return;
        };
        if !is_scalar_wasm_type(element) {
            self.unsupported(span, "only scalar array elements compile to Wasm v1");
            return;
        }

        if let Some(index) = constant_index(index) {
            if index >= *length {
                self.unsupported(span, "constant array index is out of bounds");
                return;
            }
            self.compile_array_element(target, index);
            return;
        }

        if *length == 0 {
            self.emit("i32.const 0");
            return;
        }

        let index_local = self.allocate_scratch_local();
        self.compile_expr(index);
        self.emit(format!("local.set {index_local}"));

        let result_local = self.allocate_scratch_local();
        self.compile_array_element(target, 0);
        self.emit(format!("local.set {result_local}"));
        for candidate in 1..*length {
            self.emit(format!("local.get {index_local}"));
            self.emit(format!("i32.const {candidate}"));
            self.emit("i32.eq");
            self.emit("if");
            self.compile_array_element(target, candidate);
            self.emit(format!("local.set {result_local}"));
            self.emit("end");
        }
        self.emit(format!("local.get {result_local}"));
    }

    fn compile_array_element(&mut self, target: &HirExpr, index: u64) {
        match &target.kind {
            HirExprKind::Binding(id) => {
                let Some(local) = self.locals.get(id).cloned() else {
                    self.unsupported(target.span, "unmapped array binding");
                    return;
                };
                let Some(slot) = local.slots.get(index as usize) else {
                    self.unsupported(target.span, "array index is out of bounds");
                    return;
                };
                self.emit(format!("local.get {slot}"));
            }
            HirExprKind::Array(elements) => {
                let Some(element) = elements.get(index as usize) else {
                    self.unsupported(target.span, "array index is out of bounds");
                    return;
                };
                self.compile_expr(element);
            }
            _ => {
                self.unsupported(
                    target.span,
                    "array indexing currently requires an array binding or literal",
                );
            }
        }
    }

    fn allocate_local(&mut self, id: HirBindingId, ty: &HirType) -> LocalValue {
        let width = value_width(ty).unwrap_or(1);
        let slots = (self.next_local..self.next_local + width).collect::<Vec<_>>();
        self.next_local += width;
        let local = LocalValue { slots };
        self.locals.insert(id, local.clone());
        local
    }

    fn allocate_scratch_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        local
    }

    fn ensure_wasm_type(&mut self, ty: &HirType, span: Span) {
        if supported_value_width(ty).is_none() {
            self.unsupported(
                span,
                "only `u32`, `bool`, `ArithmeticError`, `()`, scalar `Option`, scalar `Result`, and fixed-size scalar aggregates compile to Wasm v1",
            );
        }
    }

    fn emit_default_value(&mut self, ty: &HirType) {
        let width = supported_value_width(ty).unwrap_or(0);
        for _ in 0..width {
            self.emit("i32.const 0");
        }
    }

    fn drop_value(&mut self, ty: &HirType) {
        let width = supported_value_width(ty).unwrap_or(0);
        for _ in 0..width {
            self.emit("drop");
        }
    }

    fn store_slots(&mut self, slots: &[u32]) {
        for slot in slots.iter().rev() {
            self.emit(format!("local.set {slot}"));
        }
    }

    fn unsupported(&mut self, span: Span, message: &str) {
        self.compiler.error(Some(span), message);
    }

    fn emit(&mut self, instruction: impl Into<String>) {
        self.code.push(instruction.into());
    }
}

fn used_host_builtins(program: &HirProgram) -> Vec<HostBuiltin> {
    let mut builtins = Vec::new();
    for function in &program.functions {
        collect_host_builtins_block(&function.body, &mut builtins);
    }
    builtins
}

fn collect_host_builtins_block(block: &HirBlock, builtins: &mut Vec<HostBuiltin>) {
    for statement in &block.statements {
        collect_host_builtins_stmt(statement, builtins);
    }
    if let Some(result) = &block.result {
        collect_host_builtins_expr(result, builtins);
    }
}

fn collect_host_builtins_stmt(statement: &HirStmt, builtins: &mut Vec<HostBuiltin>) {
    match statement {
        HirStmt::Let(stmt) => {
            if let Some(value) = &stmt.value {
                collect_host_builtins_expr(value, builtins);
            }
        }
        HirStmt::Assign(stmt) => {
            collect_host_builtins_expr(&stmt.target, builtins);
            collect_host_builtins_expr(&stmt.value, builtins);
        }
        HirStmt::Expr(stmt) => collect_host_builtins_expr(&stmt.expr, builtins),
        HirStmt::If(stmt) => {
            collect_host_builtins_expr(&stmt.condition, builtins);
            collect_host_builtins_block(&stmt.then_block, builtins);
            if let Some(branch) = &stmt.else_branch {
                collect_host_builtins_else(branch, builtins);
            }
        }
        HirStmt::Match(stmt) => {
            collect_host_builtins_expr(&stmt.expr, builtins);
            for arm in &stmt.arms {
                match &arm.body {
                    langlog_sema::HirMatchBody::Block(block) => {
                        collect_host_builtins_block(block, builtins);
                    }
                    langlog_sema::HirMatchBody::Expr(expr) => {
                        collect_host_builtins_expr(expr, builtins);
                    }
                }
            }
        }
        HirStmt::For(stmt) => {
            collect_host_builtins_expr(&stmt.iterable, builtins);
            collect_host_builtins_block(&stmt.body, builtins);
        }
        HirStmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                collect_host_builtins_expr(value, builtins);
            }
        }
        HirStmt::Observe(stmt) => {
            collect_host_builtins_expr(&stmt.left, builtins);
            collect_host_builtins_expr(&stmt.right, builtins);
            collect_host_builtins_block(&stmt.else_block, builtins);
        }
    }
}

fn collect_host_builtins_else(branch: &HirElseBranch, builtins: &mut Vec<HostBuiltin>) {
    match branch {
        HirElseBranch::Block(block) => collect_host_builtins_block(block, builtins),
        HirElseBranch::If(stmt) => {
            collect_host_builtins_expr(&stmt.condition, builtins);
            collect_host_builtins_block(&stmt.then_block, builtins);
            if let Some(branch) = &stmt.else_branch {
                collect_host_builtins_else(branch, builtins);
            }
        }
    }
}

fn collect_host_builtins_expr(expr: &HirExpr, builtins: &mut Vec<HostBuiltin>) {
    match &expr.kind {
        HirExprKind::HostBuiltin(builtin) => {
            if builtin.is_host_import() && !builtins.contains(builtin) {
                builtins.push(*builtin);
            }
        }
        HirExprKind::Tuple(elements) | HirExprKind::Array(elements) => {
            for element in elements {
                collect_host_builtins_expr(element, builtins);
            }
        }
        HirExprKind::Block(block) => collect_host_builtins_block(block, builtins),
        HirExprKind::Unary { expr, .. } => collect_host_builtins_expr(expr, builtins),
        HirExprKind::Binary { left, right, .. } => {
            collect_host_builtins_expr(left, builtins);
            collect_host_builtins_expr(right, builtins);
        }
        HirExprKind::Recover { expr, fallback, .. } => {
            collect_host_builtins_expr(expr, builtins);
            collect_host_builtins_expr(fallback, builtins);
        }
        HirExprKind::Call { callee, args } => {
            collect_host_builtins_expr(callee, builtins);
            for arg in args {
                collect_host_builtins_expr(arg, builtins);
            }
        }
        HirExprKind::Index { target, index } => {
            collect_host_builtins_expr(target, builtins);
            collect_host_builtins_expr(index, builtins);
        }
        HirExprKind::Binding(_)
        | HirExprKind::Item(_)
        | HirExprKind::Int(_)
        | HirExprKind::Bool(_) => {}
    }
}

fn host_builtin_import(builtin: HostBuiltin) -> &'static str {
    match builtin {
        HostBuiltin::ReadU32 => {
            "  (import \"langlog_host\" \"read_u32\" (func $host_read_u32 (result i32)))\n"
        }
        HostBuiltin::PrintU32 => {
            "  (import \"langlog_host\" \"print_u32\" (func $host_print_u32 (param i32)))\n"
        }
        HostBuiltin::PrintBool => {
            "  (import \"langlog_host\" \"print_bool\" (func $host_print_bool (param i32)))\n"
        }
        HostBuiltin::PrintNewline => {
            "  (import \"langlog_host\" \"print_newline\" (func $host_print_newline))\n"
        }
        HostBuiltin::Some
        | HostBuiltin::None
        | HostBuiltin::Ok
        | HostBuiltin::Err
        | HostBuiltin::ArithmeticOverflow
        | HostBuiltin::ArithmeticUnderflow
        | HostBuiltin::DivideByZero
        | HostBuiltin::RemainderByZero => "",
    }
}

fn host_builtin_symbol(builtin: HostBuiltin) -> &'static str {
    match builtin {
        HostBuiltin::ReadU32 => "host_read_u32",
        HostBuiltin::PrintU32 => "host_print_u32",
        HostBuiltin::PrintBool => "host_print_bool",
        HostBuiltin::PrintNewline => "host_print_newline",
        HostBuiltin::Some
        | HostBuiltin::None
        | HostBuiltin::Ok
        | HostBuiltin::Err
        | HostBuiltin::ArithmeticOverflow
        | HostBuiltin::ArithmeticUnderflow
        | HostBuiltin::DivideByZero
        | HostBuiltin::RemainderByZero => unreachable!("pure compiler builtin has no host symbol"),
    }
}

fn binary_instruction(op: BinaryOp) -> Option<&'static str> {
    match op {
        BinaryOp::Add => Some("i32.add"),
        BinaryOp::Sub => Some("i32.sub"),
        BinaryOp::Mul => Some("i32.mul"),
        BinaryOp::Div => Some("i32.div_u"),
        BinaryOp::Rem => Some("i32.rem_u"),
        BinaryOp::EqEq => Some("i32.eq"),
        BinaryOp::NotEq => Some("i32.ne"),
        BinaryOp::Lt => Some("i32.lt_u"),
        BinaryOp::LtEq => Some("i32.le_u"),
        BinaryOp::Gt => Some("i32.gt_u"),
        BinaryOp::GtEq => Some("i32.ge_u"),
        BinaryOp::And => Some("i32.and"),
        BinaryOp::Or => Some("i32.or"),
        BinaryOp::Range => None,
    }
}

fn is_checked_arithmetic(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
    )
}

fn observe_instruction(op: ObserveOp) -> &'static str {
    match op {
        ObserveOp::Lt => "i32.lt_u",
        ObserveOp::LtEq => "i32.le_u",
        ObserveOp::Gt => "i32.gt_u",
        ObserveOp::GtEq => "i32.ge_u",
        ObserveOp::Eq => "i32.eq",
        ObserveOp::NotEq => "i32.ne",
    }
}

fn value_width(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::Unit => Some(0),
        HirType::U32 | HirType::Bool | HirType::ArithmeticError => Some(1),
        HirType::Tuple(elements) => elements
            .iter()
            .try_fold(0_u32, |width, element| Some(width + value_width(element)?)),
        HirType::Array { element, length } => {
            let element_width = value_width(element)?;
            let length = u32::try_from(*length).ok()?;
            element_width.checked_mul(length)
        }
        HirType::Option(inner) if is_scalar_wasm_type(inner) => Some(2),
        HirType::Result { ok, err }
            if is_scalar_wasm_type(ok) && matches!(**err, HirType::ArithmeticError) =>
        {
            Some(2)
        }
        HirType::Option(_)
        | HirType::Result { .. }
        | HirType::Set { .. }
        | HirType::Map { .. }
        | HirType::Range(_)
        | HirType::Named(_)
        | HirType::Function(_) => None,
    }
}

fn supported_value_width(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::Array { element, length } if is_scalar_wasm_type(element) => {
            let length = u32::try_from(*length).ok()?;
            length.checked_mul(1)
        }
        HirType::Array { .. } => None,
        _ => value_width(ty),
    }
}

fn is_scalar_wasm_type(ty: &HirType) -> bool {
    matches!(ty, HirType::U32 | HirType::Bool | HirType::ArithmeticError)
}

fn wasm_return_width(ty: &HirType) -> Option<u32> {
    match ty {
        HirType::Tuple(_) | HirType::Array { .. } => None,
        _ => supported_value_width(ty),
    }
}

fn wasm_result_signature(width: u32) -> String {
    let mut result = String::new();
    for _ in 0..width {
        result.push_str(" (result i32)");
    }
    result
}

fn constant_index(expr: &HirExpr) -> Option<u64> {
    match &expr.kind {
        HirExprKind::Int(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
