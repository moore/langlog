use super::{build_wasm, check_source, render_diagnostics, CheckOptions};
use langlog_syntax::{Diagnostic, Label, SourceFile};

//= SPEC.md#llg-cli-01-single-file-front-end
//= type=test
//# The phase 1 front end MUST check in-memory source text without filesystem access.
#[test]
fn requirement_llg_cli_01_checks_in_memory_source_without_filesystem_access() {
    let outcome = check_source("memory.llg", "fn main( {", CheckOptions::new());

    assert!(outcome.has_errors());
    assert!(outcome
        .rendered_diagnostics()
        .contains("error: expected a parameter name"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# The compiler interface MUST promote warnings to failing diagnostics when requested.
#[test]
fn requirement_llg_cli_02_promotes_warnings_in_compiler_interface() {
    let diagnostics = super::promote_warnings(&[Diagnostic::warning("example warning")]);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "example warning" && diagnostic.severity == super::Severity::Error
    }));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# A successful Wasm build MUST produce WAT and non-empty Wasm bytes.
#[test]
fn requirement_llg_wasm_01_build_returns_wat_and_wasm_bytes() {
    let outcome = build_wasm("memory.llg", "fn main() -> u32 { 42 }");

    assert!(!outcome.has_errors());
    let artifact = outcome.artifact.expect("expected wasm artifact");
    assert!(artifact.wat.contains("(export \"main\""));
    assert!(!artifact.wasm.is_empty());
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm build diagnostics MUST be reported without panicking.
#[test]
fn requirement_llg_wasm_01_reports_backend_diagnostics_without_panicking() {
    let outcome = build_wasm(
        "memory.llg",
        "fn helper(values: Set<u32, 16>) -> u32 { 0 }\nfn main() -> u32 { 1 }",
    );

    assert!(outcome.has_errors());
    assert!(outcome.artifact.is_none());
    assert!(outcome
        .check
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("check/proof-only")));
}

//= SPEC.md#llg-diag-02-rendered-syntax-diagnostics
//= type=test
//# The CLI MUST render syntax errors with file path, line, column, source line text, and an underline spanning the full primary source span.
#[test]
fn requirement_llg_diag_02_renders_source_linked_syntax_errors() {
    let outcome = check_source("broken.llg", "fn main( {", CheckOptions::new());

    let rendered = outcome.rendered_diagnostics();

    assert!(rendered.contains("error: expected a parameter name"));
    assert!(rendered.contains("broken.llg:1:10"));
    assert!(rendered.contains("fn main( {"));
    assert!(rendered.contains("^"));

    let source = SourceFile::new(
        "diagnostic.llg",
        "observe count <= limit else { return; }\n",
    );
    let span = source.span(8, 13);
    let diagnostic =
        Diagnostic::error("example error").with_label(Label::primary(span, "spans the whole name"));
    let rendered = render_diagnostics(&source, &[diagnostic]);

    assert!(rendered.contains("^^^^^ spans the whole name"));
}
