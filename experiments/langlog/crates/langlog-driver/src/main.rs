use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use langlog_syntax::{Diagnostic, Label, LabelStyle, Severity, SourceFile, Span};

fn main() -> ExitCode {
    run(env::args().skip(1))
}

fn run(mut args: impl Iterator<Item = String>) -> ExitCode {
    match (
        args.next().as_deref(),
        args.next(),
        args.next(),
        args.next(),
    ) {
        (Some("check"), Some(path), None, None) => run_check(PathBuf::from(path), false),
        (Some("check"), Some(flag), Some(path), None) if flag == "--warnings-as-errors" => {
            run_check(PathBuf::from(path), true)
        }
        (Some("build"), Some(path), None, None) => run_build(PathBuf::from(path), None),
        (Some("build"), Some(flag), Some(target), Some(path)) if flag == "--target" => {
            run_build(PathBuf::from(path), Some(target))
        }
        _ => {
            eprintln!(
                "usage: langlog check [--warnings-as-errors] <path>\n       langlog build [--target wasm] <path>"
            );
            ExitCode::from(2)
        }
    }
}

struct CheckOutput {
    stdout: String,
    stderr: String,
    exit: ExitCode,
}

fn run_check(path: PathBuf, warnings_as_errors: bool) -> ExitCode {
    let checked = match check_source(&path) {
        Ok(checked) => checked,
        Err(exit) => return exit,
    };

    let proof = langlog_proof::check(&checked);
    let output = finish_proof_check(&checked, &path, &proof, warnings_as_errors);
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    if !output.stdout.is_empty() {
        print!("{}", output.stdout);
    }
    output.exit
}

fn check_source(path: &Path) -> Result<langlog_sema::CheckedProgram, ExitCode> {
    let source = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("failed to read {}: {error}", path.display());
            return Err(ExitCode::from(1));
        }
    };

    let parsed = langlog_syntax::parse(path, source);
    if parsed.has_errors() {
        emit_diagnostics(&parsed.source, &parsed.diagnostics);
        return Err(ExitCode::from(1));
    }

    let checked = langlog_sema::analyze(parsed);
    if checked.has_errors() {
        emit_diagnostics(&checked.parsed.source, &checked.diagnostics);
        return Err(ExitCode::from(1));
    }

    Ok(checked)
}

fn run_build(path: PathBuf, target_arg: Option<String>) -> ExitCode {
    let config = match BuildConfig::load_for_source(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let target = target_arg
        .as_deref()
        .or(config.target.as_deref())
        .unwrap_or("wasm");
    if target != "wasm" {
        eprintln!("unsupported build target `{target}`");
        return ExitCode::from(2);
    }

    let checked = match check_source(&path) {
        Ok(checked) => checked,
        Err(exit) => return exit,
    };
    let proof = langlog_proof::check(&checked);
    let output = finish_proof_check(&checked, &path, &proof, false);
    if output.exit != ExitCode::SUCCESS {
        if !output.stderr.is_empty() {
            eprint!("{}", output.stderr);
        }
        return output.exit;
    }

    let module = match langlog_wasm::compile(&checked) {
        Ok(module) => module,
        Err(diagnostics) => {
            emit_diagnostics(&checked.parsed.source, &diagnostics);
            return ExitCode::from(1);
        }
    };

    let output_path = config.output_path(&path);
    if let Some(parent) = output_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            eprintln!("failed to create {}: {error}", parent.display());
            return ExitCode::from(1);
        }
    }
    if let Err(error) = fs::write(&output_path, module.wasm) {
        eprintln!("failed to write {}: {error}", output_path.display());
        return ExitCode::from(1);
    }

    println!("built {}", output_path.display());
    ExitCode::SUCCESS
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildConfig {
    target: Option<String>,
    out_dir: PathBuf,
}

impl BuildConfig {
    fn load_for_source(source_path: &Path) -> Result<Self, String> {
        let Some((config_path, contents)) = find_config(source_path)? else {
            return Ok(Self {
                target: None,
                out_dir: PathBuf::from("target/langlog"),
            });
        };
        let base_dir = config_path.parent().unwrap_or(Path::new("."));
        let mut target = None;
        let mut out_dir = PathBuf::from("target/langlog");
        let mut in_build = false;

        for (index, raw_line) in contents.lines().enumerate() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                in_build = line == "[build]";
                continue;
            }
            if !in_build {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!(
                    "{}:{}: expected `key = \"value\"`",
                    config_path.display(),
                    index + 1
                ));
            };
            let key = key.trim();
            let value = parse_config_string(value.trim()).ok_or_else(|| {
                format!(
                    "{}:{}: expected quoted string value",
                    config_path.display(),
                    index + 1
                )
            })?;
            match key {
                "target" => target = Some(value),
                "out_dir" => out_dir = PathBuf::from(value),
                _ => {
                    return Err(format!(
                        "{}:{}: unknown build config key `{key}`",
                        config_path.display(),
                        index + 1
                    ));
                }
            }
        }

        if out_dir.is_relative() {
            out_dir = base_dir.join(out_dir);
        }
        Ok(Self { target, out_dir })
    }

    fn output_path(&self, source_path: &Path) -> PathBuf {
        let stem = source_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("main");
        self.out_dir.join(format!("{stem}.wasm"))
    }
}

fn find_config(source_path: &Path) -> Result<Option<(PathBuf, String)>, String> {
    let mut current = source_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    loop {
        let candidate = current.join(".langlog-config");
        match fs::read_to_string(&candidate) {
            Ok(contents) => return Ok(Some((candidate, contents))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to read {}: {error}", candidate.display())),
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

fn parse_config_string(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    Some(value.to_owned())
}

fn emit_diagnostics(source: &SourceFile, diagnostics: &[Diagnostic]) {
    eprint!("{}", render_diagnostics(source, diagnostics));
}

fn finish_proof_check(
    checked: &langlog_sema::CheckedProgram,
    path: &Path,
    proof: &langlog_proof::CheckedProof,
    warnings_as_errors: bool,
) -> CheckOutput {
    let diagnostics = if warnings_as_errors {
        promote_warnings(&proof.diagnostics)
    } else {
        proof.diagnostics.clone()
    };

    if diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.severity, Severity::Error))
    {
        return CheckOutput {
            stdout: String::new(),
            stderr: render_diagnostics(&checked.parsed.source, &diagnostics),
            exit: ExitCode::from(1),
        };
    }

    let stderr = if diagnostics.is_empty() {
        String::new()
    } else {
        render_diagnostics(&checked.parsed.source, &diagnostics)
    };

    CheckOutput {
        stdout: format!(
            "checked {} item(s) in {} (obligations: {}, observations: {})\n",
            checked.parsed.module.items.len(),
            path.display(),
            proof.obligations,
            proof.observations
        ),
        stderr,
        exit: ExitCode::SUCCESS,
    }
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
    use super::{finish_proof_check, render_diagnostics, run};
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{self, ExitCode};
    use std::time::{SystemTime, UNIX_EPOCH};

    use langlog_proof::CheckedProof;
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
    //# The phase 1 front end MUST accept `langlog check --warnings-as-errors <path>`.
    #[test]
    fn requirement_llg_cli_01_accepts_check_warnings_as_errors_command() {
        let source = TempSource::new("fn main() {}");

        let success = run([
            "check".to_string(),
            "--warnings-as-errors".to_string(),
            source.path.display().to_string(),
        ]
        .into_iter());

        assert_eq!(success, ExitCode::SUCCESS);
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

    //= SPEC.md#llg-cli-02-cli-output-behavior
    //= type=test
    //# When a successful check includes warnings, the CLI MUST print the warnings to stderr while keeping the success summary on stdout.
    #[test]
    fn requirement_llg_cli_02_keeps_success_summaries_and_warning_output_on_separate_streams() {
        let parsed = langlog_syntax::parse("warning.llg", "fn main() {}\n");
        let checked = langlog_sema::analyze(parsed);
        let warning_span = checked.parsed.source.span(3, 7);
        let proof = CheckedProof {
            obligations: 1,
            observations: 0,
            diagnostics: vec![Diagnostic::warning("example warning")
                .with_label(Label::primary(warning_span, "warning label"))],
            facts: Vec::new(),
        };

        let output = finish_proof_check(&checked, checked.parsed.source.path(), &proof, false);

        assert_eq!(output.exit, ExitCode::SUCCESS);
        assert!(output.stdout.contains("checked 1 item(s)"));
        assert!(output.stderr.contains("warning: example warning"));
        assert!(output.stderr.contains("warning.llg:1:4"));
    }

    //= SPEC.md#llg-cli-02-cli-output-behavior
    //= type=test
    //# `langlog check --warnings-as-errors <path>` MUST promote warnings to failing diagnostics.
    #[test]
    fn requirement_llg_cli_02_promotes_warnings_to_errors_when_requested() {
        let parsed = langlog_syntax::parse("warning.llg", "fn main() {}\n");
        let checked = langlog_sema::analyze(parsed);
        let warning_span = checked.parsed.source.span(3, 7);
        let proof = CheckedProof {
            obligations: 0,
            observations: 0,
            diagnostics: vec![Diagnostic::warning("example warning")
                .with_label(Label::primary(warning_span, "warning label"))],
            facts: Vec::new(),
        };

        let output = finish_proof_check(&checked, checked.parsed.source.path(), &proof, true);

        assert_eq!(output.exit, ExitCode::from(1));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.contains("error: example warning"));
        assert!(!output.stderr.contains("warning: example warning"));
    }
}
