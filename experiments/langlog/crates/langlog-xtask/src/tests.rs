use super::{check_requirements, collect_annotation_block, workspace_root, Summary};
use std::fs;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(file_contents: &str) -> Self {
        Self::with_file("crates/example/src/lib.rs", file_contents)
    }

    fn with_file(relative_path: &str, file_contents: &str) -> Self {
        let mut root = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        root.push(format!("langlog-xtask-{}-{unique}", process::id()));
        let file_path = root.join(relative_path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, file_contents).unwrap();
        Self { root }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The requirement checker MUST accept one cited implemented test and one cited todo test when both use the required annotation shape.
#[test]
fn requirement_llg_tools_01_accepts_single_requirement_and_todo_tests() {
    let workspace = TempWorkspace::new(&format!(
            "\n{eq} SPEC.md#anchor\n{eq} type=test\n{quote} The parser MUST parse things.\n#[test]\nfn requirement_parses_things() {{}}\n\n{eq} SPEC.md#later\n{eq} type=todo\n{quote} The compiler MUST do later work.\n#[test]\nfn todo_later_work() {{}}\n",
            eq = "//=",
            quote = "//#"
        ));

    assert_eq!(
        check_requirements(&workspace.root).unwrap(),
        Summary {
            requirement_tests: 1,
            todo_tests: 1
        }
    );
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The requirement checker MUST ignore uncited helper functions.
#[test]
fn requirement_llg_tools_01_ignores_uncited_helper_functions() {
    let workspace = TempWorkspace::new("fn helper() {}\n");

    assert_eq!(
        check_requirements(&workspace.root).unwrap(),
        Summary {
            requirement_tests: 0,
            todo_tests: 0
        }
    );
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The requirement checker MUST reject uncited test functions.
#[test]
fn requirement_llg_tools_01_rejects_uncited_test_functions() {
    let workspace = TempWorkspace::new("#[test]\nfn plain_regression() {}\n\nfn helper() {}\n");

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors.iter().any(|error| error.contains("uncited #[test]")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The requirement checker MUST reject cited tests that are missing the test attribute, spec reference, trace type, or requirement quote.
#[test]
fn requirement_llg_tools_01_rejects_each_missing_required_annotation_part() {
    let workspace = TempWorkspace::new(&format!(
            "\n{eq} type=test\n{quote} Missing spec.\n#[test]\nfn requirement_missing_spec() {{}}\n\n{eq} SPEC.md#anchor\n{quote} Missing type.\n#[test]\nfn requirement_missing_type() {{}}\n\n{eq} SPEC.md#anchor\n{eq} type=test\n#[test]\nfn requirement_missing_quote() {{}}\n\n{eq} SPEC.md#anchor\n{eq} type=test\n{quote} Missing test attribute.\nfn requirement_missing_test_attr() {{}}\n",
            eq = "//=",
            quote = "//#"
        ));

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert_eq!(errors.len(), 4, "{errors:#?}");
    assert!(errors.iter().any(|error| error.contains("spec=0")));
    assert!(errors.iter().any(|error| error.contains("type=0")));
    assert!(errors.iter().any(|error| error.contains("quote=0")));
    assert!(errors.iter().any(|error| error.contains("test=0")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The requirement checker MUST reject duplicate requirement traces.
#[test]
fn requirement_llg_tools_01_rejects_duplicate_requirement_traces() {
    let workspace = TempWorkspace::new(&format!(
            "\n{eq} SPEC.md#anchor\n{eq} type=test\n{quote} The parser MUST parse things.\n#[test]\nfn requirement_one() {{}}\n\n{eq} SPEC.md#anchor\n{eq} type=test\n{quote} The parser MUST parse things.\n#[test]\nfn requirement_two() {{}}\n",
            eq = "//=",
            quote = "//#"
        ));

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.contains("duplicates requirement")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# Duplicate-trace diagnostics MUST report the original traced test line.
#[test]
fn requirement_llg_tools_01_reports_original_line_for_duplicate_traces() {
    let workspace = TempWorkspace::new(&format!(
            "\n{eq} SPEC.md#anchor\n{eq} type=test\n{quote} The parser MUST parse things.\n#[test]\nfn requirement_one() {{}}\n\n{eq} SPEC.md#anchor\n{eq} type=test\n{quote} The parser MUST parse things.\n#[test]\nfn requirement_two() {{}}\n",
            eq = "//=",
            quote = "//#"
        ));

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert!(errors.iter().any(|error| error.contains("lib.rs:6")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# The requirement checker MUST reject detached Duvet annotations.
#[test]
fn requirement_llg_tools_01_rejects_detached_annotations() {
    let workspace = TempWorkspace::new(&format!(
        "\n{eq} SPEC.md#anchor\nfn helper() {{}}\n",
        eq = "//="
    ));

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.contains("must have exactly one #[test]")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# Detached-annotation diagnostics MUST reject detached spec references, trace types, and requirement quotes.
#[test]
fn requirement_llg_tools_01_rejects_each_detached_annotation_kind() {
    let workspace = TempWorkspace::new(&format!(
        "\n{eq} SPEC.md#anchor\n\n{eq} type=test\n\n{quote} Detached quote.\n",
        eq = "//=",
        quote = "//#"
    ));

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert_eq!(errors.len(), 3, "{errors:#?}");
    assert!(errors
        .iter()
        .all(|error| error.contains("Duvet annotation must be attached")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# Requirement-checker diagnostics MUST report paths relative to the workspace root when possible.
#[test]
fn requirement_llg_tools_01_reports_relative_paths_in_diagnostics() {
    let workspace = TempWorkspace::new(&format!(
        "\n{eq} SPEC.md#anchor\nfn helper() {{}}\n",
        eq = "//="
    ));

    let errors = check_requirements(&workspace.root).unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.starts_with("crates/example/src/lib.rs:")));
}

//= TOOLS.md#llg-tools-01-requirement-checker
//= type=test
//# Requirement annotation collection MUST preserve source line numbers for attached annotation blocks.
#[test]
fn requirement_llg_tools_01_preserves_line_numbers_for_attached_annotation_blocks() {
    let lines = [
        "",
        "//= SPEC.md#anchor",
        "",
        "//= type=test",
        "//# Requirement text.",
        "#[test]",
        "fn requirement_example() {}",
    ];

    let block = collect_annotation_block(&lines, 6);

    assert_eq!(block[0].0, 1);
    assert_eq!(block[1].0, 2);
    assert_eq!(block[2].0, 3);
    assert_eq!(block[3].0, 4);
    assert_eq!(block[4].0, 5);
    assert_eq!(block[5].0, 6);
}

//= TOOLS.md#llg-tools-02-mutation-testing
//= type=test
//# The default mutation-test lane MUST run only cited implemented requirement tests.
#[test]
fn requirement_llg_tools_02_default_mutation_lane_filters_to_requirement_tests() {
    let config = fs::read_to_string(workspace_root().join(".cargo/mutants.toml")).unwrap();

    assert!(config.contains("additional_cargo_test_args = [\"requirement_\"]"));
}

//= TOOLS.md#llg-tools-02-mutation-testing
//= type=test
//# Native mutation testing MAY exclude wasm-only `JsValue` conversion shells when the pure adapter result model remains covered by cited tests.
#[test]
fn requirement_llg_tools_02_native_mutation_lane_excludes_wasm_only_jsvalue_shells() {
    let config = fs::read_to_string(workspace_root().join(".cargo/mutants.toml")).unwrap();

    assert!(config.contains("replace (check|build|build_and_run_ready) -> JsValue"));
    assert!(config.contains("replace PlaygroundResult::into_js"));
}

//= TOOLS.md#llg-tools-02-mutation-testing
//= type=test
//# The task runner MUST validate requirement annotations before running mutation testing.
#[test]
fn requirement_llg_tools_02_mutants_task_validates_requirements_first() {
    let tasks = fs::read_to_string(workspace_root().join("tasks.sh")).unwrap();
    let check = tasks
        .find("cargo run -p langlog-xtask -- check-requirements")
        .expect("expected requirements check in tasks.sh");
    let mutants = tasks
        .find("cargo mutants")
        .expect("expected cargo mutants in tasks.sh");

    assert!(check < mutants);
}
