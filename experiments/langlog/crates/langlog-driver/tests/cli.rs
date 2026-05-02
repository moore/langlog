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

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let mut root = env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        root.push(format!("langlog-driver-project-{}-{unique}", process::id()));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run_check(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", &path.display().to_string()])
        .output()
        .unwrap()
}

fn run_build_wasm(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["build", "--target", "wasm", &path.display().to_string()])
        .output()
        .unwrap()
}

fn run_build(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["build", &path.display().to_string()])
        .output()
        .unwrap()
}

fn run_check_warnings_as_errors(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args(["check", "--warnings-as-errors", &path.display().to_string()])
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

//= SPEC.md#llg-cli-01-single-file-front-end
//= type=test
//# The phase 1 front end MUST accept `langlog build --target wasm <path>`.
#[test]
fn requirement_llg_cli_01_accepts_build_target_wasm_command() {
    let project = TempProject::new();
    let source = project.write("main.llg", "fn main() -> u32 { 42 }");

    let success = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .current_dir(&project.root)
        .args(["build", "--target", "wasm", &source.display().to_string()])
        .output()
        .unwrap();

    assert!(success.status.success());
    assert!(String::from_utf8(success.stdout)
        .unwrap()
        .contains("target/langlog/main.wasm"));
}

//= SPEC.md#llg-cli-01-single-file-front-end
//= type=test
//# The phase 1 front end MUST use `.langlog-config` build settings when building source files below that config file.
#[test]
fn requirement_llg_cli_01_uses_langlog_config_build_settings() {
    let project = TempProject::new();
    project.write(
        ".langlog-config",
        r#"
[build]
target = "wasm"
out_dir = "artifacts"
"#,
    );
    let source = project.write("src/main.llg", "fn main() -> u32 { 42 }");

    let success = run_build(&source);

    assert!(success.status.success());
    assert!(project.root.join("artifacts/main.wasm").exists());
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

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When `langlog build --target wasm <path>` succeeds, the CLI MUST print the output artifact path to stdout.
#[test]
fn requirement_llg_cli_02_prints_wasm_build_artifact_path_to_stdout() {
    let project = TempProject::new();
    let source = project.write("main.llg", "fn main() -> u32 { 42 }");

    let success = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .current_dir(&project.root)
        .args(["build", "--target", "wasm", &source.display().to_string()])
        .output()
        .unwrap();

    assert!(success.status.success());
    let stdout = String::from_utf8(success.stdout).unwrap();
    assert!(stdout.contains("built "));
    assert!(stdout.contains("target/langlog/main.wasm"));
    assert!(String::from_utf8(success.stderr).unwrap().is_empty());
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# The CLI MUST reject unsupported build targets as usage errors.
#[test]
fn requirement_llg_cli_02_rejects_unsupported_build_targets() {
    let source = TempSource::new("fn main() -> u32 { 42 }");
    let failure = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args([
            "build",
            "--target",
            "native",
            &source.path.display().to_string(),
        ])
        .output()
        .unwrap();

    assert_eq!(failure.status.code(), Some(2));
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("unsupported build target `native`"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# The CLI MUST reject malformed build target flags as usage errors.
#[test]
fn requirement_llg_cli_02_rejects_malformed_build_target_flags() {
    let source = TempSource::new("fn main() -> u32 { 42 }");
    let failure = Command::new(env!("CARGO_BIN_EXE_langlog"))
        .args([
            "build",
            "--not-target",
            "wasm",
            &source.path.display().to_string(),
        ])
        .output()
        .unwrap();

    assert_eq!(failure.status.code(), Some(2));
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("usage: langlog check"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When semantic analysis fails, the CLI MUST print diagnostics to stderr.
#[test]
fn requirement_llg_cli_02_prints_semantic_failures_to_stderr() {
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

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When proof analysis fails during `langlog check`, the CLI MUST print diagnostics to stderr.
#[test]
fn requirement_llg_cli_02_prints_check_proof_failures_to_stderr() {
    let broken = TempSource::new(
        r#"
fn main(total: u32, denom: u32) {
    total / denom;
}
"#,
    );
    let failure = run_check(&broken.path);

    assert!(!failure.status.success());
    let failure_stdout = String::from_utf8(failure.stdout).unwrap();
    let failure_stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(failure_stdout.is_empty());
    assert!(failure_stderr.contains("error: possible divide-by-zero is not proven safe"));
    assert!(failure_stderr.contains(&broken.path.display().to_string()));
    assert!(failure_stderr.contains("total / denom;"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When arithmetic proof analysis fails during `langlog check`, the CLI MUST print diagnostics to stderr.
#[test]
fn requirement_llg_cli_02_prints_check_arithmetic_proof_failures_to_stderr() {
    let broken = TempSource::new(
        r#"
fn main(total: u32, step: u32) {
    total + step;
}
"#,
    );
    let failure = run_check(&broken.path);

    assert!(!failure.status.success());
    let failure_stdout = String::from_utf8(failure.stdout).unwrap();
    let failure_stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(failure_stdout.is_empty());
    assert!(failure_stderr.contains("error: possible arithmetic overflow is not proven safe"));
    assert!(failure_stderr.contains(&broken.path.display().to_string()));
    assert!(failure_stderr.contains("total + step;"));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# Wasm builds MUST stop before backend lowering when front-end or proof checks fail.
#[test]
fn requirement_llg_wasm_01_build_reports_proof_failures_to_stderr() {
    let broken = TempSource::new(
        r#"
fn main(total: u32, denom: u32) -> u32 {
    total / denom
}
"#,
    );
    let failure = run_build_wasm(&broken.path);

    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stdout).unwrap().is_empty());
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("error: possible divide-by-zero is not proven safe"));
}

//= WASM.md#llg-wasm-01-build-gate-and-entry-point
//= type=test
//# When backend lowering fails during `langlog build --target wasm`, the CLI MUST print diagnostics to stderr.
#[test]
fn requirement_llg_wasm_01_build_reports_backend_failures_to_stderr() {
    let source = TempSource::new(
        r#"
fn helper() -> [u32; 1] { [1] }
fn main() -> u32 { 1 }
"#,
    );
    let failure = run_build_wasm(&source.path);

    assert!(!failure.status.success());
    let stdout = String::from_utf8(failure.stdout).unwrap();
    let stderr = String::from_utf8(failure.stderr).unwrap();
    assert!(stdout.is_empty());
    assert!(stderr.contains("returns compile to Wasm v1"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# When `.langlog-config` cannot be read, the CLI MUST print an error to stderr.
#[test]
fn requirement_llg_cli_02_prints_unreadable_config_errors_to_stderr() {
    let project = TempProject::new();
    fs::create_dir(project.root.join(".langlog-config")).unwrap();
    let source = project.write("src/main.llg", "fn main() -> u32 { 42 }");

    let failure = run_build(&source);

    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stdout).unwrap().is_empty());
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("failed to read"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# Malformed entries in a `.langlog-config` `[build]` section MUST make build fail with a config error on stderr.
#[test]
fn requirement_llg_cli_02_rejects_malformed_build_config_entries() {
    let project = TempProject::new();
    project.write(
        ".langlog-config",
        r#"
[build]
[broken
"#,
    );
    let source = project.write("src/main.llg", "fn main() -> u32 { 42 }");

    let failure = run_build(&source);

    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8(failure.stdout).unwrap().is_empty());
    assert!(String::from_utf8(failure.stderr)
        .unwrap()
        .contains("expected `key = \"value\"`"));
}

//= SPEC.md#llg-cli-02-cli-output-behavior
//= type=test
//# `langlog check --warnings-as-errors <path>` MUST succeed when no warnings are emitted.
#[test]
fn requirement_llg_cli_02_accepts_warnings_as_errors_without_warnings() {
    let source = TempSource::new("fn main() {}");
    let success = run_check_warnings_as_errors(&source.path);

    assert!(success.status.success());
    let success_stdout = String::from_utf8(success.stdout).unwrap();
    let success_stderr = String::from_utf8(success.stderr).unwrap();
    assert!(success_stdout.contains("checked 1 item(s)"));
    assert!(success_stderr.is_empty());
}
