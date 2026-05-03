use langlog_compiler::{build_wasm, check_source, CheckOptions, Severity};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const PLAYGROUND_PATH: &str = "playground.llg";

#[wasm_bindgen]
#[cfg(target_arch = "wasm32")]
pub fn check(source: &str) -> JsValue {
    check_result(source).into_js()
}

#[wasm_bindgen]
#[cfg(target_arch = "wasm32")]
pub fn build(source: &str) -> JsValue {
    build_result(source, false).into_js()
}

#[wasm_bindgen(js_name = buildAndRunReady)]
#[cfg(target_arch = "wasm32")]
pub fn build_and_run_ready(source: &str) -> JsValue {
    build_result(source, true).into_js()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn check(source: &str) -> String {
    check_result(source).native_summary()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn build(source: &str) -> String {
    build_result(source, false).native_summary()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn build_and_run_ready(source: &str) -> String {
    build_result(source, true).native_summary()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaygroundResult {
    success: bool,
    can_run: bool,
    diagnostics: String,
    item_count: usize,
    obligations: usize,
    observations: usize,
    errors: Vec<String>,
    warnings: Vec<String>,
    wat: String,
    wasm: Vec<u8>,
    wasm_byte_length: usize,
}

impl PlaygroundResult {
    #[cfg(target_arch = "wasm32")]
    fn into_js(self) -> JsValue {
        use js_sys::{Array, Object, Reflect, Uint8Array};

        let result = Object::new();
        set_bool(&result, "success", self.success);
        set_bool(&result, "canRun", self.can_run);
        set_string(&result, "diagnostics", &self.diagnostics);
        set_usize(&result, "itemCount", self.item_count);
        set_usize(&result, "obligations", self.obligations);
        set_usize(&result, "observations", self.observations);
        set_string_array(&result, "errors", self.errors);
        set_string_array(&result, "warnings", self.warnings);
        set_string(&result, "wat", &self.wat);
        let bytes = Uint8Array::from(self.wasm.as_slice());
        set_value(&result, "wasm", bytes.as_ref());
        set_usize(&result, "wasmByteLength", self.wasm_byte_length);

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

        result.into()
    }

    fn native_summary(&self) -> String {
        format!(
            "success={};canRun={};itemCount={};obligations={};observations={};errors={:?};warnings={:?};watLength={};wasmByteLength={}",
            self.success,
            self.can_run,
            self.item_count,
            self.obligations,
            self.observations,
            self.errors,
            self.warnings,
            self.wat.len(),
            self.wasm_byte_length
        )
    }
}

fn check_result(source: &str) -> PlaygroundResult {
    let outcome = check_source(PLAYGROUND_PATH, source, CheckOptions::new());
    PlaygroundResult {
        success: !outcome.has_errors(),
        can_run: false,
        diagnostics: outcome.rendered_diagnostics(),
        item_count: outcome.item_count,
        obligations: outcome.obligations,
        observations: outcome.observations,
        errors: diagnostic_messages(&outcome.diagnostics, true),
        warnings: diagnostic_messages(&outcome.diagnostics, false),
        wat: String::new(),
        wasm: Vec::new(),
        wasm_byte_length: 0,
    }
}

fn build_result(source: &str, run_ready: bool) -> PlaygroundResult {
    let outcome = build_wasm(PLAYGROUND_PATH, source);

    let success = !outcome.has_errors();
    let can_run = run_ready && success && outcome.artifact.is_some();
    let diagnostics = outcome.check.rendered_diagnostics();
    let item_count = outcome.check.item_count;
    let obligations = outcome.check.obligations;
    let observations = outcome.check.observations;
    let errors = diagnostic_messages(&outcome.check.diagnostics, true);
    let warnings = diagnostic_messages(&outcome.check.diagnostics, false);
    let (wat, wasm, wasm_byte_length) = if let Some(artifact) = outcome.artifact {
        let wasm_byte_length = artifact.wasm.len();
        (artifact.wat, artifact.wasm, wasm_byte_length)
    } else {
        (String::new(), Vec::new(), 0)
    };

    PlaygroundResult {
        success,
        can_run,
        diagnostics,
        item_count,
        obligations,
        observations,
        errors,
        warnings,
        wat,
        wasm,
        wasm_byte_length,
    }
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

#[cfg(test)]
mod tests;
