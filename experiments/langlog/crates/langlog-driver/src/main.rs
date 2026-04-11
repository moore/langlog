use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

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
    let checked = langlog_sema::analyze(parsed);
    let proof = langlog_proof::check(&checked);

    println!(
        "bootstrap check completed for {} (obligations: {}, observations: {})",
        path.display(),
        proof.obligations,
        proof.observations
    );

    ExitCode::SUCCESS
}
