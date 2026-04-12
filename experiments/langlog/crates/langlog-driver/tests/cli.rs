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

#[test]
fn check_command_prints_success_summary_for_valid_program() {
    let source = TempSource::new("fn main() {}");

    let output = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", &source.path.display().to_string()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("checked 1 item(s)"));
    assert!(stdout.contains(&source.path.display().to_string()));
    assert!(stderr.is_empty());
}

#[test]
fn check_command_prints_syntax_errors_to_stderr() {
    let source = TempSource::new("fn main( {");

    let output = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", &source.path.display().to_string()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.is_empty());
    assert!(stderr.contains("error: expected a parameter name"));
    assert!(stderr.contains(&format!("{}:1:10", source.path.display())));
    assert!(stderr.contains("fn main( {"));
    assert!(stderr.contains("^"));
}
