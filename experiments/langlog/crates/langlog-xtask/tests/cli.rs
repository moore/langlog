use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("xtask crate should live under crates/langlog-xtask")
        .to_path_buf()
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The `check-requirements` command MUST validate the current workspace and print a success summary.
#[test]
fn requirement_llg_tools_01_check_requirements_command_validates_workspace() {
    let output = Command::new(env!("CARGO_BIN_EXE_langlog-xtask"))
        .arg("check-requirements")
        .current_dir(workspace_root())
        .output()
        .expect("expected check-requirements command to run");

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("validated "),
        "stdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
