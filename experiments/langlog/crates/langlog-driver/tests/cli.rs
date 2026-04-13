use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

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
        path.push(format!(
            "langlog-driver-integration-{}-{unique}.llg",
            process::id()
        ));
        fs::write(&path, contents).unwrap();
        Self { path }
    }
}

impl Drop for TempSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn run_check(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", &path.display().to_string()])
        .output()
        .unwrap()
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When `langlog check <path>` succeeds, the CLI MUST print a success summary to stdout.
#[test]
fn requirement_llg_cli_02_prints_success_summaries_to_stdout() {
    let source = TempSource::new("fn main() {}");
    let success = run_check(&source.path);

    assert!(success.status.success());
    let success_stdout = String::from_utf8(success.stdout).unwrap();
    assert!(success_stdout.contains("checked 1 item(s)"));
    assert!(success_stdout.contains(&source.path.display().to_string()));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When syntax analysis fails, the CLI MUST print diagnostics to stderr.
#[test]
fn requirement_llg_cli_02_prints_syntax_failures_to_stderr() {
    let broken = TempSource::new("fn main( {");
    let failure = run_check(&broken.path);

    assert!(!failure.status.success());
    let failure_stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(failure_stderr.contains("error: expected a parameter name"));
    assert!(failure_stderr.contains(&format!("{}:1:10", broken.path.display())));
    assert!(failure_stderr.contains("fn main( {"));
    assert!(failure_stderr.contains("^"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# Success and syntax-error reporting MUST not write to the opposite stream.
#[test]
fn requirement_llg_cli_02_does_not_write_success_and_error_output_to_the_wrong_streams() {
    let source = TempSource::new("fn main() {}");
    let broken = TempSource::new("fn main( {");
    let success = run_check(&source.path);
    let failure = run_check(&broken.path);

    let success_stderr = String::from_utf8(success.stderr).unwrap();
    let failure_stdout = String::from_utf8(failure.stdout).unwrap();

    assert!(success_stderr.is_empty());
    assert!(failure_stdout.is_empty());
}

#[test]
fn check_reports_semantic_failures_to_stderr() {
    let broken = TempSource::new(
        r#"
fn main() {
    missing;
}
"#,
    );
    let failure = run_check(&broken.path);

    assert!(!failure.status.success());
    let failure_stdout = String::from_utf8(failure.stdout).unwrap();
    let failure_stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(failure_stdout.is_empty());
    assert!(failure_stderr.contains("error: undefined binding `missing`"));
    assert!(failure_stderr.contains(&broken.path.display().to_string()));
    assert!(failure_stderr.contains("missing;"));
}
