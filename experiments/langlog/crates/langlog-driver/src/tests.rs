use super::{finish_check, run};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{self, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use langlog_compiler::{CheckOutcome, Diagnostic, Label, Severity, SourceFile};

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

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When a successful check includes warnings, the CLI MUST print the warnings to stderr while keeping the success summary on stdout.
#[test]
fn requirement_llg_cli_02_keeps_success_summaries_and_warning_output_on_separate_streams() {
    let source = SourceFile::new("warning.llg", "fn main() {}\n");
    let warning_span = source.span(3, 7);
    let outcome = CheckOutcome {
        source,
        item_count: 1,
        obligations: 1,
        observations: 0,
        diagnostics: vec![Diagnostic::warning("example warning")
            .with_label(Label::primary(warning_span, "warning label"))],
    };

    let output = finish_check(outcome, PathBuf::from("warning.llg").as_path());

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
    let source = SourceFile::new("warning.llg", "fn main() {}\n");
    let warning_span = source.span(3, 7);
    let mut diagnostic = Diagnostic::warning("example warning")
        .with_label(Label::primary(warning_span, "warning label"));
    diagnostic.severity = Severity::Error;
    let outcome = CheckOutcome {
        source,
        item_count: 1,
        obligations: 0,
        observations: 0,
        diagnostics: vec![diagnostic],
    };

    let output = finish_check(outcome, PathBuf::from("warning.llg").as_path());

    assert_eq!(output.exit, ExitCode::from(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.contains("error: example warning"));
    assert!(!output.stderr.contains("warning: example warning"));
}
