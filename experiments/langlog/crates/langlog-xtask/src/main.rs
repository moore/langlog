use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some("check-requirements") => {
            let root = workspace_root();
            match check_requirements(&root) {
                Ok(summary) => {
                    println!(
                        "validated {} requirement tests and {} todo tests",
                        summary.requirement_tests, summary.todo_tests
                    );
                    ExitCode::SUCCESS
                }
                Err(errors) => {
                    for error in errors {
                        eprintln!("{error}");
                    }
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!("usage: cargo run -p langlog-xtask -- check-requirements");
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Summary {
    requirement_tests: usize,
    todo_tests: usize,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("xtask crate should live under crates/langlog-xtask")
        .to_path_buf()
}

fn check_requirements(root: &Path) -> Result<Summary, Vec<String>> {
    let mut errors = Vec::new();
    let mut seen_requirements: HashMap<(String, String, String), (PathBuf, usize, String)> =
        HashMap::new();
    let mut summary = Summary {
        requirement_tests: 0,
        todo_tests: 0,
    };

    for path in rust_files(&root.join("crates")) {
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) => {
                errors.push(format!(
                    "{}: failed to read file: {error}",
                    display(root, &path)
                ));
                continue;
            }
        };
        let lines: Vec<&str> = contents.lines().collect();
        let mut consumed_annotations = HashSet::new();

        for index in 0..lines.len() {
            let Some(fn_name) = function_name(lines[index]) else {
                continue;
            };

            let block = collect_annotation_block(&lines, index);
            let parsed = parse_annotation_block(&block);
            if parsed.is_empty() {
                if parsed.test_attrs > 0 {
                    errors.push(format!(
                        "{}:{}: {fn_name} is an uncited #[test]; add one //= <spec>.md#..., one //= type=..., and one //# ...",
                        display(root, &path),
                        index + 1
                    ));
                }
                continue;
            }

            for (line_number, line) in &block {
                let stripped = line.trim();
                if is_doc_ref(stripped) || is_type_ref(stripped) || is_quote(stripped) {
                    consumed_annotations.insert(*line_number);
                }
            }

            if parsed.test_attrs != 1
                || parsed.spec_refs.len() != 1
                || parsed.type_refs.len() != 1
                || parsed.quotes.len() != 1
            {
                errors.push(format!(
                    "{}:{}: {fn_name} must have exactly one #[test], one //= <spec>.md#..., one //= type=..., and one //# ... (found test={}, spec={}, type={}, quote={})",
                    display(root, &path),
                    index + 1,
                    parsed.test_attrs,
                    parsed.spec_refs.len(),
                    parsed.type_refs.len(),
                    parsed.quotes.len()
                ));
                continue;
            }

            let spec = &parsed.spec_refs[0];
            let trace_type = &parsed.type_refs[0];
            let quote = &parsed.quotes[0];
            let key = (
                spec.spec_doc.clone(),
                spec.spec_anchor.clone(),
                quote.clone(),
            );
            if let Some((prev_path, prev_line, prev_fn)) = seen_requirements.get(&key) {
                errors.push(format!(
                    "{}:{}: {fn_name} duplicates requirement {}#{} / {:?}, already used by {}:{} ({prev_fn})",
                    display(root, &path),
                    index + 1,
                    spec.spec_doc,
                    spec.spec_anchor,
                    quote,
                    display(root, prev_path),
                    prev_line
                ));
                continue;
            }
            seen_requirements.insert(key, (path.clone(), index + 1, fn_name.clone()));

            match trace_type.as_str() {
                "test" => {
                    if !fn_name.starts_with("requirement_") {
                        errors.push(format!(
                            "{}:{}: {fn_name} must use the requirement_ prefix for type=test traces",
                            display(root, &path),
                            index + 1
                        ));
                        continue;
                    }
                    summary.requirement_tests += 1;
                }
                "todo" => {
                    if !fn_name.starts_with("todo_") {
                        errors.push(format!(
                            "{}:{}: {fn_name} must use the todo_ prefix for type=todo traces",
                            display(root, &path),
                            index + 1
                        ));
                        continue;
                    }
                    summary.todo_tests += 1;
                }
                _ => errors.push(format!(
                    "{}:{}: {fn_name} uses unsupported trace type {trace_type:?}",
                    display(root, &path),
                    index + 1
                )),
            }
        }

        for (index, line) in lines.iter().enumerate() {
            let line_number = index + 1;
            if consumed_annotations.contains(&line_number) {
                continue;
            }
            let stripped = line.trim();
            if is_doc_ref(stripped) || is_type_ref(stripped) || is_quote(stripped) {
                errors.push(format!(
                    "{}:{line_number}: Duvet annotation must be attached to a test function",
                    display(root, &path)
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(summary)
    } else {
        Err(errors)
    }
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn collect_annotation_block<'a>(lines: &'a [&str], fn_line_index: usize) -> Vec<(usize, &'a str)> {
    let mut block = Vec::new();
    let mut index = fn_line_index;
    while index > 0 {
        index -= 1;
        let line = lines[index];
        let stripped = line.trim();
        if stripped.is_empty() {
            if !block.is_empty() {
                block.push((index + 1, line));
            }
            continue;
        }
        if stripped.starts_with("#[") || stripped.starts_with("//=") || stripped.starts_with("//#")
        {
            block.push((index + 1, line));
            continue;
        }
        break;
    }
    block.reverse();
    block
}

#[derive(Default)]
struct ParsedBlock {
    test_attrs: usize,
    spec_refs: Vec<SpecRef>,
    type_refs: Vec<String>,
    quotes: Vec<String>,
}

impl ParsedBlock {
    fn is_empty(&self) -> bool {
        self.spec_refs.is_empty() && self.type_refs.is_empty() && self.quotes.is_empty()
    }
}

#[derive(Clone)]
struct SpecRef {
    spec_doc: String,
    spec_anchor: String,
}

fn parse_annotation_block(block: &[(usize, &str)]) -> ParsedBlock {
    let mut parsed = ParsedBlock::default();
    for (_, line) in block {
        let stripped = line.trim();
        if stripped == "#[test]" {
            parsed.test_attrs += 1;
        }
        if let Some(spec) = parse_doc_ref(stripped) {
            parsed.spec_refs.push(spec);
        }
        if let Some(trace_type) = parse_type_ref(stripped) {
            parsed.type_refs.push(trace_type);
        }
        if let Some(quote) = parse_quote(stripped) {
            parsed.quotes.push(quote);
        }
    }
    parsed
}

fn function_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("fn ")?;
    let name_end = rest.find('(')?;
    let name = &rest[..name_end];
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(name.to_owned())
    } else {
        None
    }
}

fn is_doc_ref(line: &str) -> bool {
    parse_doc_ref(line).is_some()
}

fn parse_doc_ref(line: &str) -> Option<SpecRef> {
    let rest = line.strip_prefix("//=")?.trim_start();
    let (doc, anchor) = rest.split_once('#')?;
    if !doc.ends_with(".md") {
        return None;
    }
    Some(SpecRef {
        spec_doc: doc.to_owned(),
        spec_anchor: anchor.trim().to_owned(),
    })
}

fn is_type_ref(line: &str) -> bool {
    parse_type_ref(line).is_some()
}

fn parse_type_ref(line: &str) -> Option<String> {
    let rest = line.strip_prefix("//=")?.trim();
    let trace_type = rest.strip_prefix("type=")?;
    if matches!(trace_type, "test" | "todo") {
        Some(trace_type.to_owned())
    } else {
        None
    }
}

fn is_quote(line: &str) -> bool {
    parse_quote(line).is_some()
}

fn parse_quote(line: &str) -> Option<String> {
    Some(line.strip_prefix("//#")?.trim().to_owned())
}

fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests;
