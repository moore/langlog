use super::{compile, Compiler, TaskRuntimeLayout};
use langlog_sema::HirTask;
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

fn checked_with_errors(source: &str) -> langlog_sema::CheckedProgram {
    let parsed = langlog_syntax::parse("wasm-test.llg", source);
    langlog_sema::analyze(parsed)
}

fn task_runtime_layout<'a>(
    checked: &'a langlog_sema::CheckedProgram,
) -> (TaskRuntimeLayout<'a>, &'a HirTask) {
    let hir = checked.hir.as_ref().expect("expected checked HIR");
    let root = hir
        .tasks
        .iter()
        .find(|task| task.name == "main")
        .expect("expected task main");
    let mut compiler = Compiler::new(hir);
    let layout = TaskRuntimeLayout::build(&mut compiler, root).expect("expected task layout");
    assert!(
        compiler.diagnostics.is_empty(),
        "{:#?}",
        compiler.diagnostics
    );
    (layout, root)
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# The Wasm compiler MUST reject programs that do not have checked HIR.
#[test]
fn requirement_llg_wasm_01_rejects_programs_without_checked_hir() {
    let checked = checked_with_errors("fn main() -> u32 { missing }");
    let diagnostics = compile(&checked).expect_err("expected backend error");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("semantic errors")));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm builds MUST export `fn main() -> u32` as `main`.
#[test]
fn requirement_llg_wasm_01_emits_exported_main_wat() {
    let checked = checked("fn main() -> u32 { 42 }");
    let module = compile(&checked).expect("expected Wasm module");

    assert!(module.wat.contains("(export \"main\""));
    assert!(module.wat.contains("i32.const 42"));
    assert!(!module.wasm.is_empty());
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST reject `main` forms other than `fn main() -> u32`.
#[test]
fn requirement_llg_wasm_01_rejects_unsupported_main_shapes() {
    let checked = checked("fn main(value: u32) -> u32 { value }");
    let diagnostics = compile(&checked).expect_err("expected backend error");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("requires `fn main() -> u32`")));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm builds MUST export `task main() -> u32` as `main`.
#[test]
fn requirement_llg_wasm_01_executes_task_main_exit() {
    assert_eq!(
        run_main(
            r#"
task main() -> u32 {
    state start() {
        exit 42;
    }
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST reject task-mode roots other than `task main() -> u32`.
#[test]
fn requirement_llg_wasm_01_rejects_unsupported_task_roots() {
    let no_root = checked(
        r#"
task worker() -> u32 {
    state start() {
        exit 0;
    }
}
"#,
    );
    let diagnostics = compile(&no_root).expect_err("expected backend error");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("requires `task main() -> u32`")));

    let param_root = checked(
        r#"
task main(value: u32) -> u32 {
    state start(value: u32) {
        exit value;
    }
}
"#,
    );
    let diagnostics = compile(&param_root).expect_err("expected backend error");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("requires `task main() -> u32`")));

    let bool_root = checked(
        r#"
task main() -> bool {
    state start() {
        exit true;
    }
}
"#,
    );
    let diagnostics = compile(&bool_root).expect_err("expected backend error");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("requires `task main() -> u32`")));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 task-state layout MUST include exactly the states reachable from `task main` through `go` statements and MUST size the shared state slots to the largest reachable task-state variant.
#[test]
fn requirement_llg_wasm_01_layout_includes_reachable_tasks_and_largest_variant() {
    let checked = checked(
        r#"
task main() -> u32 {
    state start() {
        go worker(1, 2, 3);
    }

    state worker(a: u32, b: u32, c: u32) {
        let d = c;
        exit a;
    }

    state unused(a: u32, b: u32, c: u32, d: u32, e: u32) {
        exit a;
    }
}
"#,
    );
    let (layout, root) = task_runtime_layout(&checked);

    assert_eq!(layout.states.len(), 2);
    assert_eq!(layout.state(root.states[0].id).unwrap().variant_width, 0);
    assert_eq!(layout.state_width, 4);
    assert!(layout
        .states
        .iter()
        .all(|state| state.state.name != "unused"));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 task-state layout MUST collect cyclic `go` graphs without recursive stack growth.
#[test]
fn requirement_llg_wasm_01_collects_cyclic_task_go_layout() {
    let checked = checked(
        r#"
task main() -> u32 {
    state start() {
        go left(2);
    }

    state left(value: u32) {
        go right(value);
    }

    state right(value: u32) {
        go left(value);
    }
}
"#,
    );
    let (layout, _) = task_runtime_layout(&checked);

    assert_eq!(layout.states.len(), 3);
    assert_eq!(layout.state_width, 1);
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 task-state layout MUST expose `go` target parameter offsets for state transitions.
#[test]
fn requirement_llg_wasm_01_records_go_target_parameter_offsets() {
    let checked = checked(
        r#"
task main() -> u32 {
    state start() {
        go worker(7, (8, true));
    }

    state worker(count: u32, pair: (u32, bool)) {
        exit count;
    }
}
"#,
    );
    let (layout, _) = task_runtime_layout(&checked);
    let worker = layout
        .states
        .iter()
        .find(|state| state.state.name == "worker")
        .expect("expected worker state");

    assert_eq!(worker.param_bindings.len(), 2);
    assert_eq!(worker.param_bindings[0].offsets, vec![0]);
    assert_eq!(worker.param_bindings[1].offsets, vec![1, 2]);
    assert_eq!(worker.variant_width, 3);
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST lower `go` statements as task-state transitions without direct Wasm calls to task states.
#[test]
fn requirement_llg_wasm_01_executes_task_go_as_state_transition() {
    let checked = checked(
        r#"
task main() -> u32 {
    state start() {
        go worker(42);
    }

    state worker(value: u32) {
        exit value;
    }
}
"#,
    );
    let module = compile(&checked).expect("expected Wasm module");
    assert!(module.wat.contains("br $task_dispatch"));
    assert!(!module.wat.contains("call $task"));

    assert_eq!(
        run_main(
            r#"
task main() -> u32 {
    state start() {
        go worker(42);
    }

    state worker(value: u32) {
        exit value;
    }
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST execute cyclic `go` graphs as bounded task-state transitions.
#[test]
fn requirement_llg_wasm_01_executes_cyclic_task_go() {
    assert_eq!(
        run_main(
            r#"
task main() -> u32 {
    state start() {
        go count(3);
    }

    state count(value: u32) {
        if value == 0 {
            exit 42;
        } else {
            go count(value - 1 or(err) 0);
        }
    }
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST evaluate `go` arguments before replacing active state arguments.
#[test]
fn requirement_llg_wasm_01_evaluates_go_args_before_state_replacement() {
    assert_eq!(
        run_main(
            r#"
task main() -> u32 {
    state start() {
        go swap(7, 42);
    }

    state swap(first: u32, second: u32) {
        if first == 42 {
            exit second;
        } else {
            go swap(second, first);
        }
    }
}
"#
        ),
        7
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST discard source state-local values before entering the target state.
#[test]
fn requirement_llg_wasm_01_discards_source_state_locals_on_go() {
    assert_eq!(
        run_main(
            r#"
task main() -> u32 {
    state start() {
        go source(99);
    }

    state source(value: u32) {
        let hidden = value;
        go target();
    }

    state target() {
        let leaked: u32;
        exit leaked;
    }
}
"#
        ),
        0
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST preserve task fields across `go` transitions.
#[test]
fn requirement_llg_wasm_01_preserves_task_fields_across_go() {
    assert_eq!(
        run_main(
            r#"
task main() -> u32 {
    let mut saved: u32 = 0;

    state start() {
        saved = 42;
        go done();
    }

    state done() {
        exit saved;
    }
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST emit imports for host builtins used inside reachable task bodies.
#[test]
fn requirement_llg_wasm_01_compiles_task_host_builtin_imports() {
    let (result, output) = run_main_with_host(
        r#"
task main() -> u32 {
    state start() {
        print_u32(7);
        exit 0;
    }
}
"#,
        0,
    );

    assert_eq!(result, 0);
    assert_eq!(output, vec![7]);
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST reject task-state values that are not representable as flattened Wasm values.
#[test]
fn requirement_llg_wasm_01_rejects_unsupported_task_state_values() {
    let checked = checked(
        r#"
task main() -> u32 {
    state start() {
        let values: Set<u32, 16>;
        exit 0;
    }
}
"#,
    );
    let diagnostics = compile(&checked).expect_err("expected backend error");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("check/proof-only")));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST compile helper functions returning flattened aggregate values.
#[test]
fn requirement_llg_wasm_01_compiles_flattened_aggregate_returns() {
    assert_eq!(
        run_main(
            r#"
fn helper() -> [u32; 3] { [10, 20, 30] }
fn main() -> u32 { helper()[2] }
"#
        ),
        30
    );
    assert_eq!(
        run_main(
            r#"
fn helper() -> Option<(u32, u32)> { some((1, 2)) }
fn main() -> u32 {
    if helper() == some((1, 2)) {
        return 7;
    }
    9
}
"#,
        ),
        7
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST compile generic flattened `Result<T, E>` values.
#[test]
fn requirement_llg_wasm_01_compiles_generic_result_values() {
    assert_eq!(
        run_main(
            r#"
fn helper(value: Result<u32, bool>) -> Result<u32, bool> { value }
fn main() -> u32 {
    let value: Result<u32, bool> = err(true);
    helper(value) or(err) {
        if err {
            return 7;
        }
        9
    }
}
"#,
        ),
        7
    );
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST compile helper functions returning `()` without Wasm result values.
#[test]
fn requirement_llg_wasm_01_compiles_unit_returning_helpers_without_results() {
    let checked = checked(
        r#"
fn helper() {
    print_newline();
}

fn main() -> u32 {
    helper();
    7
}
"#,
    );
    let module = compile(&checked).expect("expected Wasm module");

    assert!(module.wat.contains("(func $f0\n"));
    assert!(!module.wat.contains("(func $f0 (result i32)"));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST reject Set and Map values as check/proof-only runtime values.
#[test]
fn requirement_llg_wasm_01_rejects_set_and_map_values() {
    let set_param = checked("fn helper(values: Set<u32, 16>) -> u32 { 0 }\nfn main() -> u32 { 1 }");
    let diagnostics = compile(&set_param).expect_err("expected backend error");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("check/proof-only")));

    let map_index = checked(
        r#"
fn helper(table: Map<u32, bool, 16>) -> bool { table[1] }
fn main() -> u32 { 1 }
"#,
    );
    let diagnostics = compile(&map_index).expect_err("expected backend error");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Map indexing")));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST reject first-class function values and indirect calls.
#[test]
fn requirement_llg_wasm_01_rejects_first_class_function_values() {
    let checked = checked(
        r#"
fn helper() -> u32 { 7 }
fn main() -> u32 {
    let f = helper;
    f()
}
"#,
    );
    let diagnostics = compile(&checked).expect_err("expected backend error");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("first-class function values")
            || diagnostic.message.contains("only direct function calls")
    }));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm V1 MUST reject assignment targets other than local bindings.
#[test]
fn requirement_llg_wasm_01_rejects_non_local_assignment_targets() {
    let checked = checked(
        r#"
fn main() -> u32 {
    let mut values: [u32; 2] = [1, 2];
    values[0] = 3;
    values[0]
}
"#,
    );
    let diagnostics = compile(&checked).expect_err("expected backend error");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("only local assignments")));
}

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST lower `u32` and `bool` values as Wasm `i32` values.
#[test]
fn requirement_llg_wasm_02_executes_constant_main() {
    assert_eq!(run_main("fn main() -> u32 { 42 }"), 42);
}

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST execute checked arithmetic expressions over `u32` values when their `Result` is recovered.
#[test]
fn requirement_llg_wasm_02_executes_arithmetic_expression() {
    assert_eq!(run_main("fn main() -> u32 { 6 * 7 or(err) 0 }"), 42);
}

//= SEMANTICS.md#llg-sem-02-recovery-expressions
//= type=test
//# Recovery expressions MUST evaluate the fallback expression only for `None` or `Err` values.
#[test]
fn requirement_llg_sem_02_evaluates_recovery_fallback_only_on_failure() {
    let (_, output) = run_main_with_host(
        r#"
fn main() -> u32 {
    let maybe: Option<u32> = some(7);
    let value = maybe or {
        print_u32(99);
        0
    };
    print_u32(value);
    0
}
"#,
        0,
    );

    assert_eq!(output, vec![7]);
}

//= SEMANTICS.md#llg-sem-03-checked-arithmetic
//= type=test
//# Successful checked arithmetic MUST produce an `Ok` result containing the computed `u32` value.
#[test]
fn requirement_llg_sem_03_returns_ok_for_successful_checked_arithmetic() {
    assert_eq!(run_main("fn main() -> u32 { 40 + 2 or(err) 0 }"), 42);
}

//= SEMANTICS.md#llg-sem-03-checked-arithmetic
//= type=test
//# Checked addition and multiplication overflow MUST produce an `ArithmeticError` instead of wrapping.
#[test]
fn requirement_llg_sem_03_reports_addition_and_multiplication_overflow() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    4294967295 + 1 or(err) {
        let mut code: u32 = 9;
        if err == arithmetic_overflow() {
            code = 7;
        }
        code
    }
}
"#
        ),
        7
    );
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    4294967295 * 2 or(err) {
        let mut code: u32 = 9;
        if err == arithmetic_overflow() {
            code = 7;
        }
        code
    }
}
"#
        ),
        7
    );
}

//= SEMANTICS.md#llg-sem-03-checked-arithmetic
//= type=test
//# Checked subtraction underflow MUST produce an `ArithmeticError` instead of wrapping.
#[test]
fn requirement_llg_sem_03_reports_subtraction_underflow() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    0 - 1 or(err) {
        let mut code: u32 = 9;
        if err == arithmetic_underflow() {
            code = 7;
        }
        code
    }
}
"#
        ),
        7
    );
}

//= SEMANTICS.md#llg-sem-03-checked-arithmetic
//= type=test
//# Checked division and remainder by zero MUST produce an `ArithmeticError`.
#[test]
fn requirement_llg_sem_03_reports_division_and_remainder_by_zero() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    1 / 0 or(err) {
        let mut code: u32 = 9;
        if err == divide_by_zero() {
            code = 7;
        }
        code
    }
}
"#
        ),
        7
    );
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    1 % 0 or(err) {
        let mut code: u32 = 9;
        if err == remainder_by_zero() {
            code = 7;
        }
        code
    }
}
"#
        ),
        7
    );
}

//= SEMANTICS.md#llg-sem-04-result-lifting
//= type=test
//# Result-lifted arithmetic MUST propagate the first arithmetic error in left-to-right evaluation order.
#[test]
fn requirement_llg_sem_04_propagates_first_arithmetic_error_left_to_right() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let left: Result<u32, ArithmeticError> = err(arithmetic_underflow());
    left + (1 / 0) or(err) {
        let mut code: u32 = 9;
        if err == arithmetic_underflow() {
            code = 7;
        }
        code
    }
}
"#
        ),
        7
    );
}

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST execute direct function calls.
#[test]
fn requirement_llg_wasm_02_executes_direct_function_call() {
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

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST pass fixed-size scalar tuple parameters to direct function calls.
#[test]
fn requirement_llg_wasm_02_passes_scalar_tuple_parameters_to_calls() {
    assert_eq!(
        run_main(
            r#"
fn helper(pair: (u32, u32)) -> u32 {
    42
}

fn main() -> u32 {
    helper((1, 2))
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST execute `if` statements using scalar conditions.
#[test]
fn requirement_llg_wasm_02_executes_if_with_comparison_and_returns() {
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

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST execute `else` branches when scalar `if` conditions are false.
#[test]
fn requirement_llg_wasm_02_executes_else_branch_when_condition_is_false() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let mut value: u32 = 1;
    if false {
        value = 2;
    } else {
        value = 42;
    }
    value
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST compile unit-valued block expressions without leaving stack values.
#[test]
fn requirement_llg_wasm_02_compiles_unit_block_expressions_without_stack_values() {
    let source = r#"
fn main() -> u32 {
    {};
    42
}
"#;

    assert_eq!(run_main(source), 42);
    let module = compile(&checked(source)).expect("expected Wasm module");
    assert!(!module.wat.contains("i32.const 0"));
}

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST execute mutable local assignment.
#[test]
fn requirement_llg_wasm_02_executes_mutable_assignment() {
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

//= WASM.md#llg-wasm-02-scalar-execution
//= type=test
//# Wasm V1 MUST execute structural equality and inequality for flattened values.
#[test]
fn requirement_llg_wasm_02_executes_structural_equality() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let left: Result<Option<u32>, bool> = ok(some(7));
    let right: Result<Option<u32>, bool> = ok(some(7));
    if left == right && [1, 2] != [1, 3] {
        return 42;
    }
    0
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute fixed-size scalar array literals and constant indexing.
#[test]
fn requirement_llg_wasm_03_executes_array_literal_and_constant_index() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let values: [u32; 3] = [10, 20, 30];
    values[1]
}
"#
        ),
        20
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute constant indexing directly on fixed-size scalar array literals.
#[test]
fn requirement_llg_wasm_03_executes_direct_array_literal_constant_index() {
    assert_eq!(run_main("fn main() -> u32 { [10, 20, 30][1] }"), 20);
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST lower constant array indices directly rather than through dynamic index dispatch.
#[test]
fn requirement_llg_wasm_03_lowers_constant_array_indices_without_dynamic_dispatch() {
    let checked = checked(
        r#"
fn main() -> u32 {
    let values: [u32; 3] = [10, 20, 30];
    values[1]
}
"#,
    );
    let module = compile(&checked).expect("expected Wasm module");

    assert!(!module.wat.contains("i32.eq\n    if"));
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute dynamic indexing into fixed-size scalar arrays.
#[test]
fn requirement_llg_wasm_03_executes_array_dynamic_index() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let values: [u32; 3] = [10, 20, 30];
    let index: u32 = 2;
    values[index]
}
"#
        ),
        30
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute `for` loops over fixed-size scalar arrays.
#[test]
fn requirement_llg_wasm_03_executes_for_over_array() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let values: [u32; 4] = [1, 2, 3, 4];
    let mut total: u32 = 0;
    for value in values {
        total = total + value or(err) 0;
    }
    total
}
"#
        ),
        10
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST pass fixed-size scalar arrays to direct function calls.
#[test]
fn requirement_llg_wasm_03_executes_array_parameter_call() {
    assert_eq!(
        run_main(
            r#"
fn sum(values: [u32; 4]) -> u32 {
    let mut total: u32 = 0;
    for value in values {
        total = total + value or(err) 0;
    }
    total
}

fn main() -> u32 {
    sum([5, 6, 7, 8])
}
"#
        ),
        26
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute `for` loops over u32 ranges and range bindings.
#[test]
fn requirement_llg_wasm_03_executes_for_over_ranges() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let range = 0..4;
    let mut total: u32 = 0;
    for value in range {
        total = total + value or(err) 0;
    }
    total
}
"#
        ),
        6
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute fixed-size arrays with flattened aggregate elements.
#[test]
fn requirement_llg_wasm_03_executes_arrays_with_aggregate_elements() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let values: [(u32, bool); 2] = [(1, false), (2, true)];
    let index: u32 = 1;
    if values[index] == (2, true) {
        return 7;
    }
    9
}
"#
        ),
        7
    );
    assert_eq!(
        run_main(
            r#"
fn helper() -> [(u32, bool); 2] { [(1, false), (2, true)] }
fn main() -> u32 {
    let index: u32 = 1;
    if helper()[index] == (2, true) {
        return 7;
    }
    9
}
"#
        ),
        7
    );
}

//= WASM.md#llg-wasm-03-arrays-and-loops
//= type=test
//# Wasm V1 MUST execute `for` loops over fixed-size arrays with aggregate elements.
#[test]
fn requirement_llg_wasm_03_executes_for_over_aggregate_array() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let values: [(u32, u32); 2] = [(1, 2), (3, 4)];
    let mut found: u32 = 0;
    for value in values {
        if value == (3, 4) {
            found = 7;
        }
    }
    found
}
"#
        ),
        7
    );
}

//= WASM.md#llg-wasm-04-match-and-observe
//= type=test
//# Wasm V1 MUST execute `match` statements over scalar boolean patterns.
#[test]
fn requirement_llg_wasm_04_executes_match_statement_with_bool_patterns() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let mut value: u32 = 0;
    match true {
        false => { value = 1; },
        true => { value = 42; }
    }
    value
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-04-match-and-observe
//= type=test
//# Wasm V1 MUST execute `match` statements with scalar binding patterns.
#[test]
fn requirement_llg_wasm_04_executes_match_statement_with_binding_pattern() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    let mut value: u32 = 0;
    match 7 {
        captured => { value = captured; }
    }
    value
}
"#
        ),
        7
    );
}

//= WASM.md#llg-wasm-04-match-and-observe
//= type=test
//# Wasm V1 MUST discard non-unit expression match-arm bodies used in statement position.
#[test]
fn requirement_llg_wasm_04_discards_match_arm_expression_statement_results() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    match true {
        true => 1,
        false => 2
    }
    42
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-04-match-and-observe
//= type=test
//# Wasm V1 MUST execute an `observe` else block when the observed relation is false at runtime.
#[test]
fn requirement_llg_wasm_04_executes_observe_runtime_else_block() {
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    observe 2 < 1 else {
        return 42;
    }
    0
}
"#
        ),
        42
    );
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# Wasm V1 MUST emit imports for used host builtins.
#[test]
fn requirement_llg_wasm_05_emits_host_builtin_imports() {
    let checked = checked(
        r#"
fn main() -> u32 {
    let maybe: Option<u32> = some(1);
    let input: relevant u32 = read_u32();
    unsafe { Structural::use(input); }
    print_u32(input);
    maybe or 0
}
"#,
    );
    let module = compile(&checked).expect("expected Wasm module");

    assert!(module.wat.contains("(import \"langlog_host\" \"read_u32\""));
    assert!(module
        .wat
        .contains("(import \"langlog_host\" \"print_u32\""));
    assert!(!module.wat.contains("(import \"langlog_host\" \"some\""));
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# Wasm V1 MUST emit imports for host builtins used inside nested `else` branches.
#[test]
fn requirement_llg_wasm_05_emits_host_builtin_imports_from_nested_else_branches() {
    let checked = checked(
        r#"
fn main() -> u32 {
    if false {
        let _ = 0;
    } else if false {
        let _ = 1;
    } else {
        print_u32(42);
    }
    0
}
"#,
    );
    let module = compile(&checked).expect("expected Wasm module");

    assert!(module
        .wat
        .contains("(import \"langlog_host\" \"print_u32\""));
}

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# Wasm V1 MUST execute host builtin imports through the `langlog_host` module.
#[test]
fn requirement_llg_wasm_05_executes_host_builtin_imports() {
    let (result, output) = run_main_with_host(
        r#"
fn main() -> u32 {
    let value: relevant u32 = read_u32();
    unsafe { Structural::use(value); }
    print_u32(value);
    value
}
"#,
        41,
    );

    assert_eq!(result, 41);
    assert_eq!(output, vec![41]);
}
