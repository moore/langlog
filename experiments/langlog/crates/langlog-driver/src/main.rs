use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use langlog_compiler::{build_wasm, check_source, CheckOptions, CheckOutcome};

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
    let source = match read_source(&path) {
        Ok(source) => source,
        Err(exit) => return exit,
    };
    let output = finish_check(
        check_source(path.clone(), source, CheckOptions { warnings_as_errors }),
        &path,
    );
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    if !output.stdout.is_empty() {
        print!("{}", output.stdout);
    }
    output.exit
}

fn read_source(path: &Path) -> Result<String, ExitCode> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents),
        Err(error) => {
            eprintln!("failed to read {}: {error}", path.display());
            Err(ExitCode::from(1))
        }
    }
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

    let source = match read_source(&path) {
        Ok(source) => source,
        Err(exit) => return exit,
    };
    let outcome = build_wasm(path.clone(), source);
    if outcome.has_errors() {
        eprint!("{}", outcome.check.rendered_diagnostics());
        return ExitCode::from(1);
    }
    let module = outcome
        .artifact
        .expect("successful Wasm build has an artifact");

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

fn finish_check(outcome: CheckOutcome, path: &Path) -> CheckOutput {
    if outcome.has_errors() {
        return CheckOutput {
            stdout: String::new(),
            stderr: outcome.rendered_diagnostics(),
            exit: ExitCode::from(1),
        };
    }

    let stderr = if outcome.diagnostics.is_empty() {
        String::new()
    } else {
        outcome.rendered_diagnostics()
    };

    CheckOutput {
        stdout: format!(
            "checked {} item(s) in {} (obligations: {}, observations: {})\n",
            outcome.item_count,
            path.display(),
            outcome.obligations,
            outcome.observations
        ),
        stderr,
        exit: ExitCode::SUCCESS,
    }
}

#[cfg(test)]
mod tests {
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
}
