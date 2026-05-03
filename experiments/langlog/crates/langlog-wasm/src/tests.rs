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

fn checked_with_errors(source: &str) -> langlog_sema::CheckedProgram {
    let parsed = langlog_syntax::parse("wasm-test.llg", source);
    langlog_sema::analyze(parsed)
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
//# Wasm V1 MUST reject aggregate return values.
#[test]
fn requirement_llg_wasm_01_rejects_aggregate_return_values() {
    let checked = checked("fn helper() -> [u32; 1] { [1] }\nfn main() -> u32 { 1 }");
    let diagnostics = compile(&checked).expect_err("expected backend error");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("returns compile to Wasm v1")));
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
    assert_eq!(
        run_main(
            r#"
fn main() -> u32 {
    {};
    42
}
"#
        ),
        42
    );
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
//# Wasm V1 MUST reject non-scalar array elements.
#[test]
fn requirement_llg_wasm_03_rejects_non_scalar_array_elements() {
    let checked = checked("fn main() -> u32 { let values: [(u32, u32); 1] = [(1, 2)]; 0 }");
    let diagnostics = compile(&checked).expect_err("expected backend error");

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("scalar aggregates")));
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

//= WASM.md#llg-wasm-05-host-builtins
//= type=test
//# Wasm V1 MUST emit imports for host builtins used inside nested `else` branches.
#[test]
fn requirement_llg_wasm_05_emits_host_builtin_imports_from_nested_else_branches() {
    let checked = checked(
        r#"
fn main() -> u32 {
    if false {
        0;
    } else if false {
        1;
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
