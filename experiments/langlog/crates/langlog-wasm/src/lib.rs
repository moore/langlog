use std::collections::{HashMap, HashSet, VecDeque};

use langlog_sema::{
    CheckedProgram, HirBindingId, HirBlock, HirElseBranch, HirExpr, HirExprKind, HirForStmt,
    HirFunction, HirFunctionKind, HirItemId, HirMatchBody, HirMatchStmt, HirPatternKind,
    HirProgram, HirStmt, HirType, HostBuiltin,
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
            .filter(|function| function.kind == HirFunctionKind::Function)
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
        if self
            .program
            .functions
            .iter()
            .any(|function| function.kind == HirFunctionKind::Task)
        {
            return self.compile_task_program(module);
        }

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

    fn compile_task_program(&mut self, mut module: String) -> String {
        let Some(root) = self
            .program
            .functions
            .iter()
            .find(|function| function.kind == HirFunctionKind::Task && function.name == "main")
        else {
            self.error(None, "Wasm task build requires `task main() -> u32`");
            module.push_str(")\n");
            return module;
        };
        if !root.params.is_empty() || root.return_type != HirType::U32 {
            self.error(
                Some(root.span),
                "Wasm task build requires `task main() -> u32`",
            );
        }

        let Some(layout) = TaskRuntimeLayout::build(self, root) else {
            module.push_str(")\n");
            return module;
        };

        for builtin in used_host_builtins(self.program) {
            module.push_str(host_builtin_import(builtin));
        }

        for function in self
            .program
            .functions
            .iter()
            .filter(|function| function.kind == HirFunctionKind::Function)
        {
            let mut body = FunctionCompiler::new(self, function);
            module.push_str(&body.compile_function(false));
        }

        let mut body = FunctionCompiler::new_task_dispatcher(self, root, layout);
        module.push_str(&body.compile_task_dispatcher());
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

#[derive(Debug, Clone)]
struct TaskRuntimeLayout<'a> {
    states: Vec<TaskStateLayout<'a>>,
    state_by_id: HashMap<HirItemId, usize>,
    state_width: u32,
}

#[derive(Debug, Clone)]
struct TaskStateLayout<'a> {
    function: &'a HirFunction,
    tag: u32,
    bindings: HashMap<HirBindingId, StateSlotValue>,
    param_bindings: Vec<StateSlotValue>,
    variant_width: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateSlotValue {
    offsets: Vec<u32>,
}

impl<'a> TaskRuntimeLayout<'a> {
    fn build(compiler: &mut Compiler<'a>, root: &'a HirFunction) -> Option<Self> {
        let task_by_id: HashMap<_, _> = compiler
            .program
            .functions
            .iter()
            .filter(|function| function.kind == HirFunctionKind::Task)
            .map(|function| (function.id, function))
            .collect();

        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([root.id]);
        let mut reachable = Vec::new();
        while let Some(id) = queue.pop_front() {
            if !seen.insert(id) {
                continue;
            }
            let Some(task) = task_by_id.get(&id).copied() else {
                compiler.error(None, "delegate target is not a task item");
                continue;
            };
            reachable.push(task);
            for target in delegate_targets_in_block(&task.body) {
                if task_by_id.contains_key(&target) {
                    queue.push_back(target);
                }
            }
        }

        let mut states = Vec::new();
        let mut state_by_id = HashMap::new();
        let mut state_width = 0;
        for task in reachable {
            let tag = u32::try_from(states.len()).unwrap_or(u32::MAX);
            let Some(state) = TaskStateLayout::build(compiler, task, tag) else {
                continue;
            };
            state_width = state_width.max(state.variant_width);
            state_by_id.insert(task.id, states.len());
            states.push(state);
        }

        if compiler.diagnostics.is_empty() {
            Some(Self {
                states,
                state_by_id,
                state_width,
            })
        } else {
            None
        }
    }

    fn state(&self, id: HirItemId) -> Option<&TaskStateLayout<'a>> {
        self.state_by_id
            .get(&id)
            .and_then(|index| self.states.get(*index))
    }
}

impl<'a> TaskStateLayout<'a> {
    fn build(compiler: &mut Compiler<'a>, function: &'a HirFunction, tag: u32) -> Option<Self> {
        let mut bindings = HashMap::new();
        let mut param_bindings = Vec::new();
        let mut next_offset = 0_u32;
        let mut valid = true;

        for param in &function.params {
            match state_slot_value(&param.ty, &mut next_offset) {
                Some(slot) => {
                    bindings.insert(param.id, slot.clone());
                    param_bindings.push(slot);
                }
                None => {
                    compiler.error(Some(param.span), wasm_type_diagnostic(&param.ty));
                    valid = false;
                }
            }
        }

        let mut locals = Vec::new();
        collect_task_local_bindings(&function.body, &mut locals);
        for binding in locals {
            if bindings.contains_key(&binding.id) {
                continue;
            }
            match state_slot_value(&binding.ty, &mut next_offset) {
                Some(slot) => {
                    bindings.insert(binding.id, slot);
                }
                None => {
                    compiler.error(Some(binding.span), wasm_type_diagnostic(&binding.ty));
                    valid = false;
                }
            }
        }

        valid.then_some(Self {
            function,
            tag,
            bindings,
            param_bindings,
            variant_width: next_offset,
        })
    }
}

fn state_slot_value(ty: &HirType, next_offset: &mut u32) -> Option<StateSlotValue> {
    let width = supported_value_width(ty)?;
    let end = next_offset.checked_add(width)?;
    let offsets = (*next_offset..end).collect();
    *next_offset = end;
    Some(StateSlotValue { offsets })
}

struct FunctionCompiler<'a, 'b> {
    compiler: &'a mut Compiler<'b>,
    function: &'b HirFunction,
    locals: HashMap<HirBindingId, LocalValue>,
    param_count: u32,
    next_local: u32,
    task_layout: Option<TaskRuntimeLayout<'b>>,
    task_state_start: u32,
    task_tag_local: u32,
    next_label: u32,
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
            task_layout: None,
            task_state_start: 0,
            task_tag_local: 0,
            next_label: 0,
            code: Vec::new(),
        }
    }

    fn new_task_dispatcher(
        compiler: &'a mut Compiler<'b>,
        root: &'b HirFunction,
        layout: TaskRuntimeLayout<'b>,
    ) -> Self {
        let task_tag_local = 0;
        let task_state_start = 1;
        let next_local = task_state_start + layout.state_width;
        Self {
            compiler,
            function: root,
            locals: HashMap::new(),
            param_count: 0,
            next_local,
            task_layout: Some(layout),
            task_state_start,
            task_tag_local,
            next_label: 0,
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
                    None => self.unsupported(param.span, wasm_type_diagnostic(&param.ty)),
                }
                params
            });
        let result = match wasm_return_width(&function.return_type) {
            Some(width) => wasm_result_signature(width),
            None => {
                self.unsupported(
                    function.span,
                    "only flattened non-collection returns compile to Wasm v1",
                );
                String::new()
            }
        };

        self.compile_block(&function.body, &function.return_type);
        match wasm_return_width(&function.return_type) {
            Some(0) | None => {}
            Some(_) => {
                if let Some(result) = &function.body.result {
                    self.compile_expr(result);
                } else {
                    self.emit_default_value(&function.return_type);
                }
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

    fn compile_task_dispatcher(&mut self) -> String {
        // The task dispatcher is the executable form of the task memory model:
        // one tag local selects the active task-state variant, and one shared
        // bank of locals holds the flattened slots for the largest reachable
        // variant. Each loop iteration runs the task matching the tag.
        // `delegate` updates those shared slots, changes the tag, and branches
        // back to `$task_dispatch` instead of emitting a Wasm task call.
        let layout = self
            .task_layout
            .as_ref()
            .expect("task dispatcher requires a task layout");
        let root_tag = layout
            .state(self.function.id)
            .expect("root task should be reachable")
            .tag;
        let states = layout.states.clone();

        self.emit(format!("i32.const {root_tag}"));
        self.emit(format!("local.set {}", self.task_tag_local));
        self.emit("loop $task_dispatch");
        for state in states {
            self.emit(format!("local.get {}", self.task_tag_local));
            self.emit(format!("i32.const {}", state.tag));
            self.emit("i32.eq");
            self.emit("if");
            self.locals = self.task_locals_for_state(&state);
            self.compile_block(&state.function.body, &state.function.return_type);
            self.emit("end");
        }
        self.emit("unreachable");
        self.emit("end");
        self.emit("i32.const 0");

        let mut rendered = String::from("  (func $task_main (result i32)\n");
        for local in 0..self.next_local {
            let _ = local;
            rendered.push_str("    (local i32)\n");
        }
        for line in &self.code {
            rendered.push_str("    ");
            rendered.push_str(line);
            rendered.push('\n');
        }
        rendered.push_str("  )\n");
        rendered.push_str("  (export \"main\" (func $task_main))\n");
        rendered
    }

    fn task_locals_for_state(
        &self,
        state: &TaskStateLayout<'_>,
    ) -> HashMap<HirBindingId, LocalValue> {
        state
            .bindings
            .iter()
            .map(|(id, slots)| (*id, self.task_local_value(slots)))
            .collect()
    }

    fn task_local_value(&self, slots: &StateSlotValue) -> LocalValue {
        LocalValue {
            slots: slots
                .offsets
                .iter()
                .map(|offset| self.task_state_start + offset)
                .collect(),
        }
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
                }
                self.emit("return");
            }
            HirStmt::Forever(stmt) => {
                if self.task_layout.is_some() {
                    self.compile_task_forever(&stmt.body);
                } else {
                    self.unsupported(stmt.span, "`forever` is not supported by Wasm v1");
                }
            }
            HirStmt::Exit(stmt) => {
                if self.task_layout.is_some() {
                    self.compile_expr(&stmt.value);
                    self.emit("return");
                } else {
                    self.unsupported(stmt.span, "`exit` is not supported by Wasm v1");
                }
            }
            HirStmt::Delegate(stmt) => {
                if self.task_layout.is_some() {
                    self.compile_task_delegate(stmt);
                } else {
                    self.unsupported(stmt.span, "`delegate` is not supported by Wasm v1");
                }
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
            HirStmt::UnsafeMarker(stmt) => {
                for arg in &stmt.args {
                    self.compile_expr(arg);
                    self.drop_value(&arg.ty);
                }
            }
        }
    }

    fn compile_task_forever(&mut self, body: &HirBlock) {
        let label = self.fresh_label("task_forever");
        self.emit(format!("loop ${label}"));
        self.compile_block(body, &self.function.return_type);
        self.emit(format!("br ${label}"));
        self.emit("end");
    }

    fn compile_task_delegate(&mut self, stmt: &langlog_sema::HirDelegateStmt) {
        let Some(target) = self
            .task_layout
            .as_ref()
            .and_then(|layout| layout.state(stmt.target))
            .cloned()
        else {
            self.unsupported(stmt.span, "delegate target is not reachable from task main");
            return;
        };
        if stmt.args.len() != target.param_bindings.len() {
            self.unsupported(
                stmt.span,
                "delegate argument count does not match target task",
            );
            return;
        }

        let mut evaluated_args = Vec::new();
        for arg in &stmt.args {
            let Some(width) = supported_value_width(&arg.ty) else {
                self.unsupported(arg.span, wasm_type_diagnostic(&arg.ty));
                return;
            };
            let slots = self.allocate_scratch_locals(width);
            self.compile_expr(arg);
            self.store_slots(&slots);
            evaluated_args.push(slots);
        }

        self.clear_task_state_slots(&target);
        for (slots, target_param) in evaluated_args.iter().zip(target.param_bindings.iter()) {
            let target_local = self.task_local_value(target_param);
            self.emit_get_slots(slots);
            self.store_slots(&target_local.slots);
        }

        self.emit(format!("i32.const {}", target.tag));
        self.emit(format!("local.set {}", self.task_tag_local));
        self.emit("br $task_dispatch");
    }

    fn clear_task_state_slots(&mut self, state: &TaskStateLayout<'_>) {
        for offset in 0..state.variant_width {
            self.emit("i32.const 0");
            self.emit(format!("local.set {}", self.task_state_start + offset));
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
                if *op == BinaryOp::Range {
                    self.compile_expr(left);
                    self.compile_expr(right);
                    return;
                }
                if matches!(op, BinaryOp::EqEq | BinaryOp::NotEq) {
                    self.compile_structural_equality(*op, left, right, expr.span);
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
            HirExprKind::UnsafeMarker { args, .. } => {
                if let Some((first, rest)) = args.split_first() {
                    self.compile_expr(first);
                    for arg in rest {
                        self.compile_expr(arg);
                        self.drop_value(&arg.ty);
                    }
                } else {
                    self.emit_default_value(&expr.ty);
                }
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
            HostBuiltin::Some => {
                if args.len() != 1 {
                    self.unsupported(
                        args.first().map_or(self.function.span, |arg| arg.span),
                        "invalid builtin arity",
                    );
                    return;
                }
                self.emit("i32.const 0");
            }
            HostBuiltin::Ok => {
                if args.len() != 1 {
                    self.unsupported(
                        args.first().map_or(self.function.span, |arg| arg.span),
                        "invalid builtin arity",
                    );
                    return;
                }
                let HirType::Result { err, .. } = ty else {
                    self.unsupported(self.function.span, "`ok` must have a Result type");
                    return;
                };
                self.emit_default_value(err);
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
                let HirType::Result { ok, .. } = ty else {
                    self.unsupported(self.function.span, "`err` must have a Result type");
                    return;
                };
                let Some(err_width) = args.first().and_then(|arg| supported_value_width(&arg.ty))
                else {
                    self.unsupported(
                        self.function.span,
                        "`err` payload is not supported by Wasm v1",
                    );
                    return;
                };
                let err_locals = self.store_to_scratch_locals(err_width);
                self.emit_default_value(ok);
                self.emit_get_slots(&err_locals);
                self.emit("i32.const 1");
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
        let Some(result_width) = supported_value_width(ty) else {
            self.unsupported(span, "unsupported recovery result type");
            return;
        };

        match &target.ty {
            HirType::Option(inner) => {
                if error_binding.is_some() {
                    self.unsupported(span, "`Option` recovery cannot bind an error value");
                    return;
                }
                let Some(payload_width) = supported_value_width(inner) else {
                    self.unsupported(target.span, "unsupported Option payload type");
                    return;
                };
                let tag_local = self.allocate_scratch_local();
                let payload_locals = self.allocate_scratch_locals(payload_width);
                self.compile_expr(target);
                self.emit(format!("local.set {tag_local}"));
                self.store_slots(&payload_locals);

                self.emit(format!("local.get {tag_local}"));
                self.emit("i32.eqz");
                self.emit(format!("if{}", wasm_result_signature(result_width)));
                self.emit_get_slots(&payload_locals);
                self.emit("else");
                self.compile_expr(fallback);
                self.emit("end");
            }
            HirType::Result { ok, err } => {
                let Some(ok_width) = supported_value_width(ok) else {
                    self.unsupported(target.span, "unsupported Result ok type");
                    return;
                };
                let Some(err_width) = supported_value_width(err) else {
                    self.unsupported(target.span, "unsupported Result error type");
                    return;
                };
                let status_local = self.allocate_scratch_local();
                let err_locals = self.allocate_scratch_locals(err_width);
                let ok_locals = self.allocate_scratch_locals(ok_width);
                self.compile_expr(target);
                self.emit(format!("local.set {status_local}"));
                self.store_slots(&err_locals);
                self.store_slots(&ok_locals);

                if let Some(binding) = error_binding {
                    let local = self.allocate_local(binding.id, &binding.ty);
                    self.emit_get_slots(&err_locals);
                    self.store_slots(&local.slots);
                }

                self.emit(format!("local.get {status_local}"));
                self.emit("i32.eqz");
                self.emit(format!("if{}", wasm_result_signature(result_width)));
                self.emit_get_slots(&ok_locals);
                self.emit("else");
                self.compile_expr(fallback);
                self.emit("end");
            }
            _ => self.unsupported(
                target.span,
                "recovery targets must be Option or Result values",
            ),
        }
    }

    fn compile_checked_arithmetic(&mut self, op: BinaryOp, left: &HirExpr, right: &HirExpr) {
        let left_value = self.allocate_scratch_local();
        let left_error = self.allocate_scratch_local();
        let left_status = self.allocate_scratch_local();
        self.compile_operand_value_error_status(left, left_value, left_error, left_status);

        self.emit(format!("local.get {left_status}"));
        self.emit("i32.eqz");
        self.emit("if (result i32 i32 i32)");
        let right_value = self.allocate_scratch_local();
        let right_error = self.allocate_scratch_local();
        let right_status = self.allocate_scratch_local();
        self.compile_operand_value_error_status(right, right_value, right_error, right_status);
        self.emit(format!("local.get {right_status}"));
        self.emit("i32.eqz");
        self.emit("if (result i32 i32 i32)");
        self.compile_checked_scalar_operation(op, left_value, right_value);
        self.emit("else");
        self.emit("i32.const 0");
        self.emit(format!("local.get {right_error}"));
        self.emit("i32.const 1");
        self.emit("end");
        self.emit("else");
        self.emit("i32.const 0");
        self.emit(format!("local.get {left_error}"));
        self.emit("i32.const 1");
        self.emit("end");
    }

    fn compile_operand_value_error_status(
        &mut self,
        expr: &HirExpr,
        value_local: u32,
        error_local: u32,
        status_local: u32,
    ) {
        match &expr.ty {
            HirType::Result { ok, err }
                if **ok == HirType::U32 && **err == HirType::ArithmeticError =>
            {
                self.compile_expr(expr);
                self.emit(format!("local.set {status_local}"));
                self.emit(format!("local.set {error_local}"));
                self.emit(format!("local.set {value_local}"));
            }
            _ => {
                self.compile_expr(expr);
                self.emit(format!("local.set {value_local}"));
                self.emit("i32.const 0");
                self.emit(format!("local.set {error_local}"));
                self.emit("i32.const 0");
                self.emit(format!("local.set {status_local}"));
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
                self.emit("if (result i32 i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {ARITHMETIC_OVERFLOW}"));
                self.emit("i32.const 1");
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.add");
                self.emit("i32.const 0");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Sub => {
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.lt_u");
                self.emit("if (result i32 i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {ARITHMETIC_UNDERFLOW}"));
                self.emit("i32.const 1");
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.sub");
                self.emit("i32.const 0");
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
                self.emit("if (result i32 i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {ARITHMETIC_OVERFLOW}"));
                self.emit("i32.const 1");
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.mul");
                self.emit("i32.const 0");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Div => {
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.eqz");
                self.emit("if (result i32 i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {DIVIDE_BY_ZERO}"));
                self.emit("i32.const 1");
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.div_u");
                self.emit("i32.const 0");
                self.emit("i32.const 0");
                self.emit("end");
            }
            BinaryOp::Rem => {
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.eqz");
                self.emit("if (result i32 i32 i32)");
                self.emit("i32.const 0");
                self.emit(format!("i32.const {REMAINDER_BY_ZERO}"));
                self.emit("i32.const 1");
                self.emit("else");
                self.emit(format!("local.get {left_local}"));
                self.emit(format!("local.get {right_local}"));
                self.emit("i32.rem_u");
                self.emit("i32.const 0");
                self.emit("i32.const 0");
                self.emit("end");
            }
            _ => self.unsupported(
                self.function.span,
                "unsupported checked arithmetic operator",
            ),
        }
    }

    fn compile_structural_equality(
        &mut self,
        op: BinaryOp,
        left: &HirExpr,
        right: &HirExpr,
        span: Span,
    ) {
        let Some(width) = supported_value_width(&left.ty) else {
            self.unsupported(span, "equality over this type is not supported by Wasm v1");
            return;
        };
        if supported_value_width(&right.ty) != Some(width) {
            self.unsupported(span, "equality operands have incompatible Wasm layouts");
            return;
        }

        let left_locals = self.compile_expr_to_scratch_locals(left);
        let right_locals = self.compile_expr_to_scratch_locals(right);
        self.emit("i32.const 1");
        for (left_slot, right_slot) in left_locals.iter().zip(right_locals.iter()) {
            self.emit(format!("local.get {left_slot}"));
            self.emit(format!("local.get {right_slot}"));
            self.emit("i32.eq");
            self.emit("i32.and");
        }
        if op == BinaryOp::NotEq {
            self.emit("i32.eqz");
        }
    }

    fn compile_for_stmt(&mut self, stmt: &HirForStmt, return_type: &HirType) {
        match &stmt.iterable.ty {
            HirType::Array { element, length } => {
                let Some(element_width) = supported_value_width(element) else {
                    self.unsupported(
                        stmt.iterable.span,
                        "array element type is not executable in Wasm v1",
                    );
                    return;
                };
                let binding = match &stmt.binding.kind {
                    HirPatternKind::Binding(binding) => Some(binding),
                    HirPatternKind::Wildcard => None,
                    HirPatternKind::Int(_) | HirPatternKind::Bool(_) => {
                        self.unsupported(
                            stmt.binding.span,
                            "`for` currently requires a binding or wildcard pattern",
                        );
                        return;
                    }
                };
                let local = binding.map(|binding| self.allocate_local(binding.id, element));
                let iterable_locals = match &stmt.iterable.kind {
                    HirExprKind::Binding(_) | HirExprKind::Array(_) => None,
                    _ => Some(self.compile_expr_to_scratch_locals(&stmt.iterable)),
                };
                for index in 0..*length {
                    if let Some(iterable_locals) = &iterable_locals {
                        self.emit_array_element_from_slots(iterable_locals, element_width, index);
                    } else {
                        self.compile_array_element(&stmt.iterable, index);
                    }
                    if let Some(local) = &local {
                        self.store_slots(&local.slots);
                    } else {
                        self.drop_slots(element_width);
                    }
                    self.compile_block(&stmt.body, return_type);
                }
            }
            HirType::Range(inner) if **inner == HirType::U32 => {
                let current = self.allocate_scratch_local();
                let end = self.allocate_scratch_local();
                self.compile_expr(&stmt.iterable);
                self.emit(format!("local.set {end}"));
                self.emit(format!("local.set {current}"));
                let local = match &stmt.binding.kind {
                    HirPatternKind::Binding(binding) => {
                        Some(self.allocate_local(binding.id, inner))
                    }
                    HirPatternKind::Wildcard => None,
                    HirPatternKind::Int(_) | HirPatternKind::Bool(_) => {
                        self.unsupported(
                            stmt.binding.span,
                            "`for` over a range requires a binding or wildcard pattern",
                        );
                        return;
                    }
                };
                self.emit("block");
                self.emit("loop");
                self.emit(format!("local.get {current}"));
                self.emit(format!("local.get {end}"));
                self.emit("i32.ge_u");
                self.emit("br_if 1");
                if let Some(local) = &local {
                    self.emit(format!("local.get {current}"));
                    self.store_slots(&local.slots);
                }
                self.compile_block(&stmt.body, return_type);
                self.emit(format!("local.get {current}"));
                self.emit("i32.const 1");
                self.emit("i32.add");
                self.emit(format!("local.set {current}"));
                self.emit("br 0");
                self.emit("end");
                self.emit("end");
            }
            HirType::Set { .. } | HirType::Map { .. } => {
                self.unsupported(
                    stmt.iterable.span,
                    "Set and Map values are check/proof-only in Wasm v1",
                );
            }
            _ => {
                self.unsupported(
                    stmt.span,
                    "`for` currently requires an array or u32 range iterable",
                );
            }
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
            if matches!(target.ty, HirType::Set { .. } | HirType::Map { .. }) {
                self.unsupported(span, "Set and Map indexing is check/proof-only in Wasm v1");
            } else {
                self.unsupported(span, "only array indexing compiles to Wasm v1");
            }
            return;
        };
        let Some(element_width) = supported_value_width(element) else {
            self.unsupported(span, "array element type is not executable in Wasm v1");
            return;
        };

        if let Some(index) = constant_index(index) {
            if index >= *length {
                self.unsupported(span, "constant array index is out of bounds");
                return;
            }
            self.compile_array_element(target, index);
            return;
        }

        if *length == 0 {
            self.emit_default_value(element);
            return;
        }

        let index_local = self.allocate_scratch_local();
        self.compile_expr(index);
        self.emit(format!("local.set {index_local}"));

        let result_locals = self.allocate_scratch_locals(element_width);
        let target_locals = match &target.kind {
            HirExprKind::Binding(_) | HirExprKind::Array(_) => None,
            _ => Some(self.compile_expr_to_scratch_locals(target)),
        };
        if let Some(target_locals) = &target_locals {
            self.emit_array_element_from_slots(target_locals, element_width, 0);
        } else {
            self.compile_array_element(target, 0);
        }
        self.store_slots(&result_locals);
        for candidate in 1..*length {
            self.emit(format!("local.get {index_local}"));
            self.emit(format!("i32.const {candidate}"));
            self.emit("i32.eq");
            self.emit("if");
            if let Some(target_locals) = &target_locals {
                self.emit_array_element_from_slots(target_locals, element_width, candidate);
            } else {
                self.compile_array_element(target, candidate);
            }
            self.store_slots(&result_locals);
            self.emit("end");
        }
        self.emit_get_slots(&result_locals);
    }

    fn compile_array_element(&mut self, target: &HirExpr, index: u64) {
        let HirType::Array { element, length } = &target.ty else {
            self.unsupported(target.span, "array element access requires an array target");
            return;
        };
        let Some(element_width) = supported_value_width(element) else {
            self.unsupported(
                target.span,
                "array element type is not executable in Wasm v1",
            );
            return;
        };
        if index >= *length {
            self.unsupported(target.span, "array index is out of bounds");
            return;
        }
        match &target.kind {
            HirExprKind::Binding(id) => {
                let Some(local) = self.locals.get(id).cloned() else {
                    self.unsupported(target.span, "unmapped array binding");
                    return;
                };
                let start = index as usize * element_width as usize;
                let end = start + element_width as usize;
                let Some(slots) = local.slots.get(start..end) else {
                    self.unsupported(target.span, "array index is out of bounds");
                    return;
                };
                self.emit_get_slots(slots);
            }
            HirExprKind::Array(elements) => {
                let Some(element) = elements.get(index as usize) else {
                    self.unsupported(target.span, "array index is out of bounds");
                    return;
                };
                self.compile_expr(element);
            }
            _ => {
                let array_locals = self.compile_expr_to_scratch_locals(target);
                let start = index as usize * element_width as usize;
                let end = start + element_width as usize;
                self.emit_get_slots(&array_locals[start..end]);
            }
        }
    }

    fn emit_array_element_from_slots(&mut self, slots: &[u32], element_width: u32, index: u64) {
        let start = index as usize * element_width as usize;
        let end = start + element_width as usize;
        self.emit_get_slots(&slots[start..end]);
    }

    fn allocate_local(&mut self, id: HirBindingId, ty: &HirType) -> LocalValue {
        if self.task_layout.is_some() {
            if let Some(local) = self.locals.get(&id).cloned() {
                return local;
            }
            self.compiler
                .error(None, "task local is missing from task state layout");
            return LocalValue { slots: Vec::new() };
        }

        let width = value_width(ty).unwrap_or(1);
        let slots = (self.next_local..self.next_local + width).collect::<Vec<_>>();
        self.next_local += width;
        let local = LocalValue { slots };
        self.locals.insert(id, local.clone());
        local
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!("{prefix}_{}", self.next_label);
        self.next_label += 1;
        label
    }

    fn allocate_scratch_local(&mut self) -> u32 {
        let local = self.next_local;
        self.next_local += 1;
        local
    }

    fn allocate_scratch_locals(&mut self, width: u32) -> Vec<u32> {
        (0..width).map(|_| self.allocate_scratch_local()).collect()
    }

    fn compile_expr_to_scratch_locals(&mut self, expr: &HirExpr) -> Vec<u32> {
        let width = supported_value_width(&expr.ty).unwrap_or(0);
        let locals = self.allocate_scratch_locals(width);
        self.compile_expr(expr);
        self.store_slots(&locals);
        locals
    }

    fn store_to_scratch_locals(&mut self, width: u32) -> Vec<u32> {
        let locals = self.allocate_scratch_locals(width);
        self.store_slots(&locals);
        locals
    }

    fn ensure_wasm_type(&mut self, ty: &HirType, span: Span) {
        if supported_value_width(ty).is_none() {
            self.unsupported(span, wasm_type_diagnostic(ty));
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
        self.drop_slots(width);
    }

    fn drop_slots(&mut self, width: u32) {
        for _ in 0..width {
            self.emit("drop");
        }
    }

    fn store_slots(&mut self, slots: &[u32]) {
        for slot in slots.iter().rev() {
            self.emit(format!("local.set {slot}"));
        }
    }

    fn emit_get_slots(&mut self, slots: &[u32]) {
        for slot in slots {
            self.emit(format!("local.get {slot}"));
        }
    }

    fn unsupported(&mut self, span: Span, message: &str) {
        self.compiler.error(Some(span), message);
    }

    fn emit(&mut self, instruction: impl Into<String>) {
        self.code.push(instruction.into());
    }
}

fn delegate_targets_in_block(block: &HirBlock) -> Vec<HirItemId> {
    let mut targets = Vec::new();
    collect_delegate_targets_block(block, &mut targets);
    targets
}

fn collect_delegate_targets_block(block: &HirBlock, targets: &mut Vec<HirItemId>) {
    for statement in &block.statements {
        collect_delegate_targets_stmt(statement, targets);
    }
    if let Some(result) = &block.result {
        collect_delegate_targets_expr(result, targets);
    }
}

fn collect_delegate_targets_stmt(statement: &HirStmt, targets: &mut Vec<HirItemId>) {
    match statement {
        HirStmt::Let(stmt) => {
            if let Some(value) = &stmt.value {
                collect_delegate_targets_expr(value, targets);
            }
        }
        HirStmt::Assign(stmt) => {
            collect_delegate_targets_expr(&stmt.target, targets);
            collect_delegate_targets_expr(&stmt.value, targets);
        }
        HirStmt::Expr(stmt) => collect_delegate_targets_expr(&stmt.expr, targets),
        HirStmt::If(stmt) => {
            collect_delegate_targets_expr(&stmt.condition, targets);
            collect_delegate_targets_block(&stmt.then_block, targets);
            if let Some(branch) = &stmt.else_branch {
                collect_delegate_targets_else(branch, targets);
            }
        }
        HirStmt::Match(stmt) => {
            collect_delegate_targets_expr(&stmt.expr, targets);
            for arm in &stmt.arms {
                match &arm.body {
                    HirMatchBody::Block(block) => collect_delegate_targets_block(block, targets),
                    HirMatchBody::Expr(expr) => collect_delegate_targets_expr(expr, targets),
                }
            }
        }
        HirStmt::For(stmt) => {
            collect_delegate_targets_expr(&stmt.iterable, targets);
            collect_delegate_targets_block(&stmt.body, targets);
        }
        HirStmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                collect_delegate_targets_expr(value, targets);
            }
        }
        HirStmt::Forever(stmt) => collect_delegate_targets_block(&stmt.body, targets),
        HirStmt::Exit(stmt) => collect_delegate_targets_expr(&stmt.value, targets),
        HirStmt::Delegate(stmt) => {
            targets.push(stmt.target);
            for arg in &stmt.args {
                collect_delegate_targets_expr(arg, targets);
            }
        }
        HirStmt::Observe(stmt) => {
            collect_delegate_targets_expr(&stmt.left, targets);
            collect_delegate_targets_expr(&stmt.right, targets);
            collect_delegate_targets_block(&stmt.else_block, targets);
        }
        HirStmt::UnsafeMarker(stmt) => {
            for arg in &stmt.args {
                collect_delegate_targets_expr(arg, targets);
            }
        }
    }
}

fn collect_delegate_targets_else(branch: &HirElseBranch, targets: &mut Vec<HirItemId>) {
    match branch {
        HirElseBranch::Block(block) => collect_delegate_targets_block(block, targets),
        HirElseBranch::If(stmt) => {
            collect_delegate_targets_expr(&stmt.condition, targets);
            collect_delegate_targets_block(&stmt.then_block, targets);
            if let Some(branch) = &stmt.else_branch {
                collect_delegate_targets_else(branch, targets);
            }
        }
    }
}

fn collect_delegate_targets_expr(expr: &HirExpr, targets: &mut Vec<HirItemId>) {
    match &expr.kind {
        HirExprKind::Tuple(elements) | HirExprKind::Array(elements) => {
            for element in elements {
                collect_delegate_targets_expr(element, targets);
            }
        }
        HirExprKind::Block(block) => collect_delegate_targets_block(block, targets),
        HirExprKind::Unary { expr, .. } => collect_delegate_targets_expr(expr, targets),
        HirExprKind::Binary { left, right, .. } => {
            collect_delegate_targets_expr(left, targets);
            collect_delegate_targets_expr(right, targets);
        }
        HirExprKind::Recover { expr, fallback, .. } => {
            collect_delegate_targets_expr(expr, targets);
            collect_delegate_targets_expr(fallback, targets);
        }
        HirExprKind::Call { callee, args } => {
            collect_delegate_targets_expr(callee, targets);
            for arg in args {
                collect_delegate_targets_expr(arg, targets);
            }
        }
        HirExprKind::Index { target, index } => {
            collect_delegate_targets_expr(target, targets);
            collect_delegate_targets_expr(index, targets);
        }
        HirExprKind::UnsafeMarker { args, .. } => {
            for arg in args {
                collect_delegate_targets_expr(arg, targets);
            }
        }
        HirExprKind::Binding(_)
        | HirExprKind::Item(_)
        | HirExprKind::HostBuiltin(_)
        | HirExprKind::Int(_)
        | HirExprKind::Bool(_) => {}
    }
}

fn collect_task_local_bindings(block: &HirBlock, bindings: &mut Vec<langlog_sema::HirBinding>) {
    for statement in &block.statements {
        collect_task_local_bindings_stmt(statement, bindings);
    }
    if let Some(result) = &block.result {
        collect_task_local_bindings_expr(result, bindings);
    }
}

fn collect_task_local_bindings_stmt(
    statement: &HirStmt,
    bindings: &mut Vec<langlog_sema::HirBinding>,
) {
    match statement {
        HirStmt::Let(stmt) => {
            bindings.push(stmt.binding.clone());
            if let Some(value) = &stmt.value {
                collect_task_local_bindings_expr(value, bindings);
            }
        }
        HirStmt::Assign(stmt) => {
            collect_task_local_bindings_expr(&stmt.target, bindings);
            collect_task_local_bindings_expr(&stmt.value, bindings);
        }
        HirStmt::Expr(stmt) => collect_task_local_bindings_expr(&stmt.expr, bindings),
        HirStmt::If(stmt) => {
            collect_task_local_bindings_expr(&stmt.condition, bindings);
            collect_task_local_bindings(&stmt.then_block, bindings);
            if let Some(branch) = &stmt.else_branch {
                collect_task_local_bindings_else(branch, bindings);
            }
        }
        HirStmt::Match(stmt) => {
            collect_task_local_bindings_expr(&stmt.expr, bindings);
            for arm in &stmt.arms {
                if let HirPatternKind::Binding(binding) = &arm.pattern.kind {
                    bindings.push(binding.clone());
                }
                match &arm.body {
                    HirMatchBody::Block(block) => collect_task_local_bindings(block, bindings),
                    HirMatchBody::Expr(expr) => collect_task_local_bindings_expr(expr, bindings),
                }
            }
        }
        HirStmt::For(stmt) => {
            collect_task_local_bindings_expr(&stmt.iterable, bindings);
            if let HirPatternKind::Binding(binding) = &stmt.binding.kind {
                bindings.push(binding.clone());
            }
            collect_task_local_bindings(&stmt.body, bindings);
        }
        HirStmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                collect_task_local_bindings_expr(value, bindings);
            }
        }
        HirStmt::Forever(stmt) => collect_task_local_bindings(&stmt.body, bindings),
        HirStmt::Exit(stmt) => collect_task_local_bindings_expr(&stmt.value, bindings),
        HirStmt::Delegate(stmt) => {
            for arg in &stmt.args {
                collect_task_local_bindings_expr(arg, bindings);
            }
        }
        HirStmt::Observe(stmt) => {
            collect_task_local_bindings_expr(&stmt.left, bindings);
            collect_task_local_bindings_expr(&stmt.right, bindings);
            collect_task_local_bindings(&stmt.else_block, bindings);
        }
        HirStmt::UnsafeMarker(stmt) => {
            for arg in &stmt.args {
                collect_task_local_bindings_expr(arg, bindings);
            }
        }
    }
}

fn collect_task_local_bindings_else(
    branch: &HirElseBranch,
    bindings: &mut Vec<langlog_sema::HirBinding>,
) {
    match branch {
        HirElseBranch::Block(block) => collect_task_local_bindings(block, bindings),
        HirElseBranch::If(stmt) => {
            collect_task_local_bindings_expr(&stmt.condition, bindings);
            collect_task_local_bindings(&stmt.then_block, bindings);
            if let Some(branch) = &stmt.else_branch {
                collect_task_local_bindings_else(branch, bindings);
            }
        }
    }
}

fn collect_task_local_bindings_expr(expr: &HirExpr, bindings: &mut Vec<langlog_sema::HirBinding>) {
    match &expr.kind {
        HirExprKind::Tuple(elements) | HirExprKind::Array(elements) => {
            for element in elements {
                collect_task_local_bindings_expr(element, bindings);
            }
        }
        HirExprKind::Block(block) => collect_task_local_bindings(block, bindings),
        HirExprKind::Unary { expr, .. } => collect_task_local_bindings_expr(expr, bindings),
        HirExprKind::Binary { left, right, .. } => {
            collect_task_local_bindings_expr(left, bindings);
            collect_task_local_bindings_expr(right, bindings);
        }
        HirExprKind::Recover {
            expr,
            error_binding,
            fallback,
        } => {
            collect_task_local_bindings_expr(expr, bindings);
            if let Some(binding) = error_binding {
                bindings.push(binding.clone());
            }
            collect_task_local_bindings_expr(fallback, bindings);
        }
        HirExprKind::Call { callee, args } => {
            collect_task_local_bindings_expr(callee, bindings);
            for arg in args {
                collect_task_local_bindings_expr(arg, bindings);
            }
        }
        HirExprKind::Index { target, index } => {
            collect_task_local_bindings_expr(target, bindings);
            collect_task_local_bindings_expr(index, bindings);
        }
        HirExprKind::UnsafeMarker { args, .. } => {
            for arg in args {
                collect_task_local_bindings_expr(arg, bindings);
            }
        }
        HirExprKind::Binding(_)
        | HirExprKind::Item(_)
        | HirExprKind::HostBuiltin(_)
        | HirExprKind::Int(_)
        | HirExprKind::Bool(_) => {}
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
        HirStmt::Forever(stmt) => collect_host_builtins_block(&stmt.body, builtins),
        HirStmt::Exit(stmt) => collect_host_builtins_expr(&stmt.value, builtins),
        HirStmt::Delegate(stmt) => {
            for arg in &stmt.args {
                collect_host_builtins_expr(arg, builtins);
            }
        }
        HirStmt::Observe(stmt) => {
            collect_host_builtins_expr(&stmt.left, builtins);
            collect_host_builtins_expr(&stmt.right, builtins);
            collect_host_builtins_block(&stmt.else_block, builtins);
        }
        HirStmt::UnsafeMarker(stmt) => {
            for arg in &stmt.args {
                collect_host_builtins_expr(arg, builtins);
            }
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
        HirExprKind::HostBuiltin(
            builtin @ (HostBuiltin::ReadU32
            | HostBuiltin::PrintU32
            | HostBuiltin::PrintBool
            | HostBuiltin::PrintNewline),
        ) => {
            if !builtins.contains(builtin) {
                builtins.push(*builtin);
            }
        }
        HirExprKind::HostBuiltin(_) => {}
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
        HirExprKind::UnsafeMarker { args, .. } => {
            for arg in args {
                collect_host_builtins_expr(arg, builtins);
            }
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
        HirType::Option(inner) => Some(value_width(inner)?.checked_add(1)?),
        HirType::Result { ok, err } => Some(
            value_width(ok)?
                .checked_add(value_width(err)?)?
                .checked_add(1)?,
        ),
        HirType::Range(inner) if **inner == HirType::U32 => Some(2),
        HirType::Range(_)
        | HirType::Set { .. }
        | HirType::Map { .. }
        | HirType::Named(_)
        | HirType::Function(_) => None,
    }
}

fn supported_value_width(ty: &HirType) -> Option<u32> {
    value_width(ty)
}

fn is_scalar_wasm_type(ty: &HirType) -> bool {
    matches!(ty, HirType::U32 | HirType::Bool | HirType::ArithmeticError)
}

fn wasm_return_width(ty: &HirType) -> Option<u32> {
    supported_value_width(ty)
}

fn wasm_type_diagnostic(ty: &HirType) -> &'static str {
    match ty {
        HirType::Set { .. } | HirType::Map { .. } => {
            "Set and Map values are check/proof-only in Wasm v1"
        }
        HirType::Function(_) => "first-class function values are not supported by Wasm v1",
        _ => "only flattened non-collection values compile to Wasm v1",
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
