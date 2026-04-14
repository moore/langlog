use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use langlog_syntax::{Diagnostic, Label, LabelStyle, Severity, SourceFile, Span};

fn main() -> ExitCode {
    run(env::args().skip(1))
}

fn run(mut args: impl Iterator<Item = String>) -> ExitCode {
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
    if checked.has_errors() {
        emit_diagnostics(&checked.parsed.source, &checked.diagnostics);
        return ExitCode::from(1);
    }

    let proof = langlog_proof::check(&checked);
    if proof.has_errors() {
        emit_diagnostics(&checked.parsed.source, &proof.diagnostics);
        return ExitCode::from(1);
    }

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
    eprint!("{}", render_diagnostics(source, diagnostics));
}

fn render_diagnostics(source: &SourceFile, diagnostics: &[Diagnostic]) -> String {
    let mut rendered = String::new();
    for diagnostic in diagnostics {
        render_diagnostic(source, diagnostic, &mut rendered);
    }
    rendered
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
mod tests {
    use super::{render_diagnostics, run};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use langlog_syntax::{Diagnostic, Label, SourceFile};

    struct TempSource {
        path: PathBuf,
    }

    impl TempSource {
        fn new(contents: &str) -> Self {
            let mut path = env::temp_dir();
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("langlog-driver-{}-{unique}.llg", process::id()));
            fs::write(&path, contents).unwrap();
            Self { path }
        }
    }

    impl Drop for TempSource {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    //= SPEC.md#llg-cli-01-single-file-front-end
    //= type=test
    //# The phase 1 front end MUST accept `langlog check <path>`.
    #[test]
    fn requirement_llg_cli_01_accepts_check_path_command() {
        let source = TempSource::new("fn main() {}");

        let success = run(["check".to_string(), source.path.display().to_string()].into_iter());

        assert_eq!(success, std::process::ExitCode::SUCCESS);
    }

    //= SPEC.md#llg-cli-01-single-file-front-end
    //= type=test
    //# The phase 1 front end MUST treat `<path>` as a single source file.
    #[test]
    fn requirement_llg_cli_01_treats_path_as_a_single_source_file() {
        let source = TempSource::new("fn main() {}");
        let second = TempSource::new("fn helper() {}");

        let extra_path = run([
            "check".to_string(),
            source.path.display().to_string(),
            second.path.display().to_string(),
        ]
        .into_iter());

        assert_eq!(extra_path, std::process::ExitCode::from(2));
    }

    //= SPEC.md#llg-diag-02-rendered-syntax-diagnostics
    //= type=test
    //# The CLI MUST render syntax errors with file path, line, column, source line text, and an underline spanning the full primary source span.
    #[test]
    fn requirement_llg_diag_02_renders_source_linked_syntax_errors() {
        let parsed = langlog_syntax::parse("broken.llg", "fn main( {");
        assert!(parsed.has_errors());

        let rendered = render_diagnostics(&parsed.source, &parsed.diagnostics);

        assert!(rendered.contains("error: expected a parameter name"));
        assert!(rendered.contains("broken.llg:1:10"));
        assert!(rendered.contains("fn main( {"));
        assert!(rendered.contains("^"));

        let source = SourceFile::new(
            "diagnostic.llg",
            "observe count <= limit else { return; }\n",
        );
        let span = source.span(8, 13);
        let diagnostic = Diagnostic::error("example error")
            .with_label(Label::primary(span, "spans the whole name"));
        let rendered = render_diagnostics(&source, &[diagnostic]);

        assert!(rendered.contains("^^^^^ spans the whole name"));
    }
}
