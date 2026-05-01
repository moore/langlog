use js_sys::{Array, Object, Reflect, Uint8Array};
use langlog_compiler::{build_wasm, check_source, CheckOptions, Severity};
use wasm_bindgen::prelude::*;

const PLAYGROUND_PATH: &str = "playground.llg";

#[wasm_bindgen]
pub fn check(source: &str) -> JsValue {
    let outcome = check_source(PLAYGROUND_PATH, source, CheckOptions::new());
    let result = Object::new();
    set_bool(&result, "success", !outcome.has_errors());
    set_string(&result, "diagnostics", &outcome.rendered_diagnostics());
    set_usize(&result, "itemCount", outcome.item_count);
    set_usize(&result, "obligations", outcome.obligations);
    set_usize(&result, "observations", outcome.observations);
    set_string_array(
        &result,
        "errors",
        diagnostic_messages(&outcome.diagnostics, true),
    );
    set_string_array(
        &result,
        "warnings",
        diagnostic_messages(&outcome.diagnostics, false),
    );
    result.into()
}

#[wasm_bindgen]
pub fn build(source: &str) -> JsValue {
    build_result(source, false)
}

#[wasm_bindgen(js_name = buildAndRunReady)]
pub fn build_and_run_ready(source: &str) -> JsValue {
    build_result(source, true)
}

fn build_result(source: &str, run_ready: bool) -> JsValue {
    let outcome = build_wasm(PLAYGROUND_PATH, source);
    let result = Object::new();
    set_bool(&result, "success", !outcome.has_errors());
    set_bool(
        &result,
        "canRun",
        run_ready && !outcome.has_errors() && outcome.artifact.is_some(),
    );
    set_string(
        &result,
        "diagnostics",
        &outcome.check.rendered_diagnostics(),
    );
    set_usize(&result, "itemCount", outcome.check.item_count);
    set_usize(&result, "obligations", outcome.check.obligations);
    set_usize(&result, "observations", outcome.check.observations);
    set_string_array(
        &result,
        "errors",
        diagnostic_messages(&outcome.check.diagnostics, true),
    );
    set_string_array(
        &result,
        "warnings",
        diagnostic_messages(&outcome.check.diagnostics, false),
    );

    if let Some(artifact) = outcome.artifact {
        set_string(&result, "wat", &artifact.wat);
        let bytes = Uint8Array::from(artifact.wasm.as_slice());
        set_value(&result, "wasm", bytes.as_ref());
        set_usize(&result, "wasmByteLength", artifact.wasm.len());
    } else {
        set_string(&result, "wat", "");
        set_value(&result, "wasm", &Uint8Array::new_with_length(0).into());
        set_usize(&result, "wasmByteLength", 0);
    }

    result.into()
}

fn diagnostic_messages(diagnostics: &[langlog_compiler::Diagnostic], errors: bool) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                (errors, diagnostic.severity),
                (true, Severity::Error) | (false, Severity::Warning)
            )
        })
        .map(|diagnostic| diagnostic.message.clone())
        .collect()
}

fn set_bool(object: &Object, key: &str, value: bool) {
    set_value(object, key, &JsValue::from_bool(value));
}

fn set_usize(object: &Object, key: &str, value: usize) {
    set_value(object, key, &JsValue::from_f64(value as f64));
}

fn set_string(object: &Object, key: &str, value: &str) {
    set_value(object, key, &JsValue::from_str(value));
}

fn set_string_array(object: &Object, key: &str, values: Vec<String>) {
    let array = Array::new();
    for value in values {
        array.push(&JsValue::from_str(&value));
    }
    set_value(object, key, array.as_ref());
}

fn set_value(object: &Object, key: &str, value: &JsValue) {
    Reflect::set(object, &JsValue::from_str(key), value)
        .expect("setting result object properties should not fail");
}
