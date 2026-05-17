use super::{build, build_and_run_ready, build_result, check, check_result};

const PLAYGROUND_EXAMPLES_JSON: &str = include_str!("../../../playground/examples.json");

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground `check` API MUST report success and frontend counts without producing Wasm bytes.
#[test]
fn requirement_llg_wasm_06_check_reports_success_and_counts_without_wasm() {
    let result = check_result("fn main() -> u32 { 42 }");

    assert!(result.success);
    assert!(!result.can_run);
    assert_eq!(result.item_count, 1);
    assert_eq!(result.wasm_byte_length, 0);
    assert!(result.wasm.is_empty());
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground `build` API MUST report Wasm text and bytes but not mark the module runnable.
#[test]
fn requirement_llg_wasm_06_build_reports_wasm_without_run_ready() {
    let result = build_result("fn main() -> u32 { 42 }", false);

    assert!(result.success);
    assert!(!result.can_run);
    assert!(result.wat.contains("(export \"main\""));
    assert!(result.wasm_byte_length > 0);
    assert_eq!(result.wasm_byte_length, result.wasm.len());
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground `buildAndRunReady` API MUST mark successful Wasm builds runnable.
#[test]
fn requirement_llg_wasm_06_build_and_run_ready_marks_successful_builds_runnable() {
    let result = build_result("fn main() -> u32 { 42 }", true);

    assert!(result.success);
    assert!(result.can_run);
    assert!(result.wasm_byte_length > 0);
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground APIs MUST report rendered diagnostics and separated error messages for invalid source.
#[test]
fn requirement_llg_wasm_06_reports_diagnostics_for_invalid_source() {
    let result = build_result("fn main() -> u32 { missing }", true);

    assert!(!result.success);
    assert!(!result.can_run);
    assert!(result.wat.is_empty());
    assert_eq!(result.wasm_byte_length, 0);
    assert!(result.diagnostics.contains("undefined binding `missing`"));
    assert!(result
        .errors
        .iter()
        .any(|error| error.contains("undefined binding `missing`")));
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# Native playground adapter tests MUST expose the wasm-bindgen APIs as inspectable string summaries.
#[test]
fn requirement_llg_wasm_06_native_exports_return_inspectable_summaries() {
    let checked = check("fn main() -> u32 { 42 }");
    let built = build("fn main() -> u32 { 42 }");
    let runnable = build_and_run_ready("fn main() -> u32 { 42 }");

    assert!(checked.contains("success=true"));
    assert!(checked.contains("wasmByteLength=0"));
    assert!(built.contains("canRun=false"));
    assert!(runnable.contains("canRun=true"));
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground example programs MUST build successfully and be marked runnable by the playground adapter.
#[test]
fn requirement_llg_wasm_06_playground_examples_build_and_run_ready() {
    let examples: serde_json::Value =
        serde_json::from_str(PLAYGROUND_EXAMPLES_JSON).expect("examples JSON should parse");
    let examples = examples.as_array().expect("examples should be an array");

    assert_eq!(examples.len(), 19);

    for example in examples {
        let name = example
            .get("name")
            .and_then(serde_json::Value::as_str)
            .expect("example should have a string name");
        let source = example
            .get("source")
            .and_then(serde_json::Value::as_str)
            .expect("example should have a string source");
        let result = build_result(source, true);

        assert!(
            result.success,
            "{name} should build: {}",
            result.diagnostics
        );
        assert!(result.can_run, "{name} should be runnable");
        assert!(result.wasm_byte_length > 0, "{name} should produce Wasm");
        assert!(
            result.wat.contains("(export \"main\""),
            "{name} should export main"
        );
    }
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground example programs MUST use task-mode roots with `task main() -> u32`.
#[test]
fn requirement_llg_wasm_06_playground_examples_use_task_roots() {
    let examples: serde_json::Value =
        serde_json::from_str(PLAYGROUND_EXAMPLES_JSON).expect("examples JSON should parse");
    let examples = examples.as_array().expect("examples should be an array");
    let first = examples
        .first()
        .and_then(|example| example.get("source"))
        .and_then(serde_json::Value::as_str)
        .expect("first example should have a source");

    assert!(first.contains("exit 42"));
    for example in examples {
        let source = example
            .get("source")
            .and_then(serde_json::Value::as_str)
            .expect("example should have a string source");

        assert!(source.contains("task main() -> u32"));
        assert!(!source.contains("fn main"));
    }
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground example programs MUST include a runnable finite `forever` task example.
#[test]
fn requirement_llg_wasm_06_playground_examples_include_finite_forever_task() {
    let examples: serde_json::Value =
        serde_json::from_str(PLAYGROUND_EXAMPLES_JSON).expect("examples JSON should parse");
    let examples = examples.as_array().expect("examples should be an array");
    let forever = examples
        .iter()
        .find(|example| {
            example
                .get("source")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|source| source.contains("forever"))
        })
        .expect("examples should include a forever task");
    let source = forever
        .get("source")
        .and_then(serde_json::Value::as_str)
        .expect("forever example should have a source");
    let result = build_result(source, true);

    assert!(source.contains("exit"));
    assert!(result.success, "forever example should build");
    assert!(result.can_run, "forever example should be runnable");
}

//= WASM.md#llg-wasm-06-playground-adapter
//= type=test
//# The playground example programs MUST include a task delegation example.
#[test]
fn requirement_llg_wasm_06_playground_examples_include_task_delegation() {
    let examples: serde_json::Value =
        serde_json::from_str(PLAYGROUND_EXAMPLES_JSON).expect("examples JSON should parse");
    let examples = examples.as_array().expect("examples should be an array");

    assert!(examples.iter().any(|example| {
        example
            .get("source")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|source| source.contains("delegate"))
    }));
}
