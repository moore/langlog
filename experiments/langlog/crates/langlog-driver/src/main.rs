use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use langlog_syntax::{Diagnostic, Label, LabelStyle, Severity, SourceFile, Span};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);

    match (args.next().as_deref(), args.next(), args.next()) {
        (Some("check"), Some(path), None) => run_check(PathBuf::from(path)),
        _ => {
            eprintln!("usage: langlog check <path>");
            ExitCode::from(2)
        }
    }
}

fn run_check(path: PathBuf) -> ExitCode {
    let source = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("failed to read {}: {error}", path.display());
            return ExitCode::from(1);
        }
    };

    let parsed = langlog_syntax::parse(path.clone(), source);
    if parsed.has_errors() {
        emit_diagnostics(&parsed.source, &parsed.diagnostics);
        return ExitCode::from(1);
    }

    let checked = langlog_sema::analyze(parsed);
    let proof = langlog_proof::check(&checked);

    println!(
        "checked {} item(s) in {} (obligations: {}, observations: {})",
        checked.parsed.module.items.len(),
        path.display(),
        proof.obligations,
        proof.observations
    );

    ExitCode::SUCCESS
}

fn emit_diagnostics(source: &SourceFile, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        emit_diagnostic(source, diagnostic);
    }
}

fn emit_diagnostic(source: &SourceFile, diagnostic: &Diagnostic) {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };

    eprintln!("{severity}: {}", diagnostic.message);
    for label in &diagnostic.labels {
        emit_label(source, label);
    }

    for note in &diagnostic.notes {
        eprintln!("note: {note}");
    }

    eprintln!();
}

fn emit_label(source: &SourceFile, label: &Label) {
    let Some(location) = source.location(label.span.start()) else {
        eprintln!(" --> {}", source.path().display());
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

    eprintln!(
        "{:>width$} --> {}:{}:{}",
        "",
        source.path().display(),
        line,
        location.column,
        width = gutter_width
    );
    eprintln!("{:>width$} |", "", width = gutter_width);
    if let Some(text) = source.line_text(line) {
        eprintln!("{line_number:>width$} | {text}", width = gutter_width);
        match &label.message {
            Some(message) => eprintln!(
                "{:>width$} | {padding}{marker} {message}",
                "",
                width = gutter_width
            ),
            None => eprintln!("{:>width$} | {padding}{marker}", "", width = gutter_width),
        }
    }
}

fn underline_width(source: &SourceFile, span: Span, line: usize) -> usize {
    let Some(line_span) = source.line_span(line) else {
        return 1;
    };

    let line_end = span.end().as_usize().min(line_span.end().as_usize());
    let line_start = span.start().as_usize().min(line_end);

    source.contents()[line_start..line_end].chars().count().max(1)
}
