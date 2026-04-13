use std::env;
use std::fs;
use std::path::PathBuf;
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

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When `langlog check <path>` succeeds, the CLI MUST print a success summary to stdout.
//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When syntax analysis fails, the CLI MUST print diagnostics to stderr.
//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# Success and syntax-error reporting MUST not write to the opposite stream.
#[test]
fn requirement_llg_cli_02_routes_success_and_error_output_to_the_correct_streams() {
    let source = TempSource::new("fn main() {}");
    let broken = TempSource::new("fn main( {");

    let success = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", &source.path.display().to_string()])
        .output()
        .unwrap();
    let failure = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", &broken.path.display().to_string()])
        .output()
        .unwrap();

    assert!(success.status.success());
    let success_stdout = String::from_utf8(success.stdout).unwrap();
    let success_stderr = String::from_utf8(success.stderr).unwrap();
    assert!(success_stdout.contains("checked 1 item(s)"));
    assert!(success_stdout.contains(&source.path.display().to_string()));
    assert!(success_stderr.is_empty());

    assert!(!failure.status.success());
    let failure_stdout = String::from_utf8(failure.stdout).unwrap();
    let failure_stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(failure_stdout.is_empty());
    assert!(failure_stderr.contains("error: expected a parameter name"));
    assert!(failure_stderr.contains(&format!("{}:1:10", broken.path.display())));
    assert!(failure_stderr.contains("fn main( {"));
    assert!(failure_stderr.contains("^"));
}
