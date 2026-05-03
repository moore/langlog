use std::fmt::Write as _;
use std::path::PathBuf;

pub use langlog_syntax::{Diagnostic, Label, Severity, SourceFile};
use langlog_syntax::{LabelStyle, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckOptions {
    pub warnings_as_errors: bool,
}

impl CheckOptions {
    pub const fn new() -> Self {
        Self {
            warnings_as_errors: false,
        }
    }

    pub const fn warnings_as_errors() -> Self {
        Self {
            warnings_as_errors: true,
        }
    }
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub source: SourceFile,
    pub item_count: usize,
    pub obligations: usize,
    pub observations: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl CheckOutcome {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.severity, Severity::Error))
    }

    pub fn rendered_diagnostics(&self) -> String {
        render_diagnostics(&self.source, &self.diagnostics)
    }
}

#[derive(Debug, Clone)]
pub struct WasmArtifact {
    pub wat: String,
    pub wasm: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct BuildOutcome {
    pub check: CheckOutcome,
    pub artifact: Option<WasmArtifact>,
}

impl BuildOutcome {
    pub fn has_errors(&self) -> bool {
        self.check.has_errors()
    }
}

pub fn check_source(
    path: impl Into<PathBuf>,
    contents: impl Into<String>,
    options: CheckOptions,
) -> CheckOutcome {
    let parsed = langlog_syntax::parse(path, contents);
    let source = parsed.source.clone();
    if parsed.has_errors() {
        return CheckOutcome {
            source,
            item_count: parsed.module.items.len(),
            obligations: 0,
            observations: 0,
            diagnostics: parsed.diagnostics,
        };
    }

    let checked = langlog_sema::analyze(parsed);
    let source = checked.parsed.source.clone();
    if checked.has_errors() {
        return CheckOutcome {
            source,
            item_count: checked.parsed.module.items.len(),
            obligations: 0,
            observations: 0,
            diagnostics: checked.diagnostics,
        };
    }

    let proof = langlog_proof::check(&checked);
    let diagnostics = if options.warnings_as_errors {
        promote_warnings(&proof.diagnostics)
    } else {
        proof.diagnostics
    };

    CheckOutcome {
        source,
        item_count: checked.parsed.module.items.len(),
        obligations: proof.obligations,
        observations: proof.observations,
        diagnostics,
    }
}

pub fn build_wasm(path: impl Into<PathBuf>, contents: impl Into<String>) -> BuildOutcome {
    let path = path.into();
    let contents = contents.into();
    let check = check_source(path.clone(), contents.clone(), CheckOptions::new());
    if check.has_errors() {
        return BuildOutcome {
            check,
            artifact: None,
        };
    }

    let parsed = langlog_syntax::parse(path, contents);
    let checked = langlog_sema::analyze(parsed);
    let module = match langlog_wasm::compile(&checked) {
        Ok(module) => module,
        Err(diagnostics) => {
            let mut check = check;
            check.diagnostics = diagnostics;
            return BuildOutcome {
                check,
                artifact: None,
            };
        }
    };

    BuildOutcome {
        check,
        artifact: Some(WasmArtifact {
            wat: module.wat,
            wasm: module.wasm,
        }),
    }
}

pub fn render_diagnostics(source: &SourceFile, diagnostics: &[Diagnostic]) -> String {
    let mut rendered = String::new();
    for diagnostic in diagnostics {
        render_diagnostic(source, diagnostic, &mut rendered);
    }
    rendered
}

fn promote_warnings(diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .cloned()
        .map(|mut diagnostic| {
            if matches!(diagnostic.severity, Severity::Warning) {
                diagnostic.severity = Severity::Error;
            }
            diagnostic
        })
        .collect()
}

fn render_diagnostic(source: &SourceFile, diagnostic: &Diagnostic, rendered: &mut String) {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };

    let _ = writeln!(rendered, "{severity}: {}", diagnostic.message);
    for label in &diagnostic.labels {
        render_label(source, label, rendered);
    }

    for note in &diagnostic.notes {
        let _ = writeln!(rendered, "note: {note}");
    }

    rendered.push('\n');
}

fn render_label(source: &SourceFile, label: &Label, rendered: &mut String) {
    let Some(location) = source.location(label.span.start()) else {
        let _ = writeln!(rendered, " --> {}", source.path().display());
        return;
    };

    let line = location.line;
    let line_number = line.to_string();
    let gutter_width = line_number.len();
    let underline_len = underline_width(source, label.span, line);
    let marker = match label.style {
        LabelStyle::Primary => '^',
        LabelStyle::Secondary => '-',
    }
    .to_string()
    .repeat(underline_len.max(1));
    let padding = " ".repeat(location.column.saturating_sub(1));

    let _ = writeln!(
        rendered,
        "{:>width$} --> {}:{}:{}",
        "",
        source.path().display(),
        line,
        location.column,
        width = gutter_width
    );
    let _ = writeln!(rendered, "{:>width$} |", "", width = gutter_width);
    if let Some(text) = source.line_text(line) {
        let _ = writeln!(
            rendered,
            "{line_number:>width$} | {text}",
            width = gutter_width
        );
        match &label.message {
            Some(message) => {
                let _ = writeln!(
                    rendered,
                    "{:>width$} | {padding}{marker} {message}",
                    "",
                    width = gutter_width
                );
            }
            None => {
                let _ = writeln!(
                    rendered,
                    "{:>width$} | {padding}{marker}",
                    "",
                    width = gutter_width
                );
            }
        }
    }
}

fn underline_width(source: &SourceFile, span: Span, line: usize) -> usize {
    let Some(line_span) = source.line_span(line) else {
        return 1;
    };

    let line_end = span.end().as_usize().min(line_span.end().as_usize());
    let line_start = span.start().as_usize().min(line_end);

    source.contents()[line_start..line_end]
        .chars()
        .count()
        .max(1)
}

#[cfg(test)]
mod tests;
