use std::collections::HashMap;

use langlog_sema::{
    CheckedProgram, HirBindingId, HirBlock, HirElseBranch, HirExpr, HirExprKind, HirFunction,
    HirItemId, HirProgram, HirStmt, HirType, HostBuiltin,
};
use langlog_syntax::ast::{BinaryOp, UnaryOp};
use langlog_syntax::{Diagnostic, Label, Span};

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
    locals: HashMap<HirBindingId, u32>,
    param_count: u32,
    next_local: u32,
    code: Vec<String>,
}

impl<'a, 'b> FunctionCompiler<'a, 'b> {
    fn new(compiler: &'a mut Compiler<'b>, function: &'b HirFunction) -> Self {
        let mut locals = HashMap::new();
        for (index, param) in function.params.iter().enumerate() {
            locals.insert(param.id, index as u32);
        }
        let param_count = function.params.len() as u32;
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
            .map(|_| " (param i32)")
            .collect::<String>();
        let result = match function.return_type {
            HirType::Unit => "",
            HirType::U32 | HirType::Bool => " (result i32)",
            _ => {
                self.unsupported(
                    function.span,
                    "only `u32`, `bool`, and `()` returns compile to Wasm v1",
                );
                ""
            }
        };

        self.compile_block(&function.body, &function.return_type);
        if matches!(function.return_type, HirType::U32 | HirType::Bool) {
            if let Some(result) = &function.body.result {
                self.compile_expr(result);
            } else {
                self.emit("i32.const 0");
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
                let local = self.allocate_local(stmt.binding.id);
                if let Some(value) = &stmt.value {
                    self.compile_expr(value);
                    self.emit(format!("local.set {local}"));
                }
            }
            HirStmt::Assign(stmt) => match &stmt.target.kind {
                HirExprKind::Binding(id) => {
                    self.compile_expr(&stmt.value);
                    let Some(local) = self.locals.get(id).copied() else {
                        self.unsupported(stmt.span, "assignment target is not a Wasm local");
                        return;
                    };
                    self.emit(format!("local.set {local}"));
                }
                _ => self.unsupported(stmt.span, "only local assignments compile to Wasm v1"),
            },
            HirStmt::Expr(stmt) => {
                self.compile_expr(&stmt.expr);
                if stmt.expr.ty != HirType::Unit {
                    self.emit("drop");
                }
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
                } else if matches!(return_type, HirType::U32 | HirType::Bool) {
                    self.emit("i32.const 0");
                }
                self.emit("return");
            }
            HirStmt::Match(stmt) => {
                self.unsupported(stmt.span, "`match` is not supported by Wasm v1")
            }
            HirStmt::For(stmt) => self.unsupported(stmt.span, "`for` is not supported by Wasm v1"),
            HirStmt::Observe(stmt) => {
                self.unsupported(stmt.span, "`observe` lowering is not supported by Wasm v1")
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
                if let Some(local) = self.locals.get(id) {
                    self.emit(format!("local.get {local}"));
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
                        self.emit(format!("call ${}", host_builtin_symbol(*builtin)));
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
                    self.emit("i32.const 0");
                }
            }
            HirExprKind::Tuple(_) | HirExprKind::Array(_) | HirExprKind::Index { .. } => {
                self.unsupported(expr.span, "this expression is not supported by Wasm v1")
            }
        }
    }

    fn allocate_local(&mut self, id: HirBindingId) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        self.locals.insert(id, local);
        local
    }

    fn ensure_wasm_type(&mut self, ty: &HirType, span: Span) {
        if !matches!(ty, HirType::Unit | HirType::U32 | HirType::Bool) {
            self.unsupported(
                span,
                "only `u32`, `bool`, and `()` values compile to Wasm v1",
            );
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
            if !builtins.contains(builtin) {
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
    }
}

fn host_builtin_symbol(builtin: HostBuiltin) -> &'static str {
    match builtin {
        HostBuiltin::ReadU32 => "host_read_u32",
        HostBuiltin::PrintU32 => "host_print_u32",
        HostBuiltin::PrintBool => "host_print_bool",
        HostBuiltin::PrintNewline => "host_print_newline",
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

#[cfg(test)]
mod tests {
    use super::compile;
    use wasmtime::{Caller, Engine, Instance, Linker, Module, Store};

    fn checked(source: &str) -> langlog_sema::CheckedProgram {
        let parsed = langlog_syntax::parse("wasm-test.llg", source);
        assert!(!parsed.has_errors(), "{:#?}", parsed.diagnostics);
        let checked = langlog_sema::analyze(parsed);
        assert!(!checked.has_errors(), "{:#?}", checked.diagnostics);
        checked
    }

    fn run_main(source: &str) -> i32 {
        let checked = checked(source);
        let module = compile(&checked).expect("expected Wasm module");
        let engine = Engine::default();
        let module = Module::new(&engine, &module.wasm).expect("expected valid module");
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[]).expect("expected instance");
        let main = instance
            .get_typed_func::<(), i32>(&mut store, "main")
            .expect("expected exported main");

        main.call(&mut store, ()).expect("expected main result")
    }

    fn run_main_with_host(source: &str, input: i32) -> (i32, Vec<i32>) {
        let checked = checked(source);
        let module = compile(&checked).expect("expected Wasm module");
        let engine = Engine::default();
        let module = Module::new(&engine, &module.wasm).expect("expected valid module");
        let mut store = Store::new(&engine, Vec::<i32>::new());
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap("langlog_host", "read_u32", move || -> i32 { input })
            .expect("expected read_u32 import");
        linker
            .func_wrap(
                "langlog_host",
                "print_u32",
                |mut caller: Caller<'_, Vec<i32>>, value: i32| {
                    caller.data_mut().push(value);
                },
            )
            .expect("expected print_u32 import");
        linker
            .func_wrap("langlog_host", "print_bool", |_: i32| {})
            .expect("expected print_bool import");
        linker
            .func_wrap("langlog_host", "print_newline", || {})
            .expect("expected print_newline import");
        let instance = linker
            .instantiate(&mut store, &module)
            .expect("expected instance");
        let main = instance
            .get_typed_func::<(), i32>(&mut store, "main")
            .expect("expected exported main");

        let result = main.call(&mut store, ()).expect("expected main result");
        (result, store.into_data())
    }

    #[test]
    fn emits_exported_main_wat() {
        let checked = checked("fn main() -> u32 { 42 }");
        let module = compile(&checked).expect("expected Wasm module");

        assert!(module.wat.contains("(export \"main\""));
        assert!(module.wat.contains("i32.const 42"));
        assert!(!module.wasm.is_empty());
    }

    #[test]
    fn rejects_unsupported_arrays() {
        let checked = checked("fn main() -> u32 { let values: [u32; 1] = [1]; 1 }");
        let diagnostics = compile(&checked).expect_err("expected backend error");

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("not supported by Wasm v1")));
    }

    #[test]
    fn executes_constant_main() {
        assert_eq!(run_main("fn main() -> u32 { 42 }"), 42);
    }

    #[test]
    fn executes_arithmetic_expression() {
        assert_eq!(run_main("fn main() -> u32 { 6 * 7 }"), 42);
    }

    #[test]
    fn executes_direct_function_call() {
        assert_eq!(
            run_main(
                r#"
fn helper() -> u32 { 42 }
fn main() -> u32 { helper() }
"#
            ),
            42
        );
    }

    #[test]
    fn executes_if_with_comparison_and_returns() {
        assert_eq!(
            run_main(
                r#"
fn main() -> u32 {
    if 1 < 2 {
        return 7;
    } else {
        return 9;
    }
}
"#
            ),
            7
        );
    }

    #[test]
    fn executes_mutable_assignment() {
        assert_eq!(
            run_main(
                r#"
fn main() -> u32 {
    let mut value: u32 = 1;
    value = 42;
    value
}
"#
            ),
            42
        );
    }

    #[test]
    fn emits_host_builtin_imports() {
        let checked = checked(
            r#"
fn main() -> u32 {
    print_u32(read_u32());
    0
}
"#,
        );
        let module = compile(&checked).expect("expected Wasm module");

        assert!(module.wat.contains("(import \"langlog_host\" \"read_u32\""));
        assert!(module
            .wat
            .contains("(import \"langlog_host\" \"print_u32\""));
    }

    #[test]
    fn executes_host_builtin_imports() {
        let (result, output) = run_main_with_host(
            r#"
fn main() -> u32 {
    let value: u32 = read_u32();
    print_u32(value);
    value
}
"#,
            41,
        );

        assert_eq!(result, 41);
        assert_eq!(output, vec![41]);
    }
}
