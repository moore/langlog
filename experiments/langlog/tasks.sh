#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

run_cargo_test() {
    echo "==> cargo test"
    cargo test
}

run_cargo_fmt() {
    echo "==> cargo fmt --all"
    cargo fmt --all
}

run_cargo_clippy() {
    echo "==> cargo clippy --all-targets --all-features -- -D warnings"
    cargo clippy --all-targets --all-features -- -D warnings
}

run_markdown_format() {
    echo "==> rumdl fmt ."
    rumdl fmt . --respect-gitignore
}

run_duvet() {
    echo "==> duvet report --require-tests true"
    duvet report --require-tests true
}

refuse_cargo_mutants() {
    cat >&2 <<'EOF'
mutation testing is intentionally disabled in ./tasks.sh
run `cargo mutants` manually when you explicitly want that expensive check
EOF
    return 2
}

usage() {
    cat <<'EOF'
Usage: ./tasks.sh [task...]

Tasks:
  all      Run the default fast checks: cargo fmt, markdown formatting, cargo test, cargo clippy, duvet
  fmt      Run cargo fmt and markdown formatting
  test     Run cargo test
  clippy   Run cargo clippy
  md       Run markdown formatting
  duvet    Run duvet report with test coverage required
  mutants  Refuse to run cargo-mutants; use `cargo mutants` manually instead
EOF
}

run_task() {
    case "$1" in
        all)
            run_cargo_fmt
            run_markdown_format
            run_cargo_test
            run_cargo_clippy
            run_duvet
            ;;
        fmt)
            run_cargo_fmt
            run_markdown_format
            ;;
        test)
            run_cargo_test
            ;;
        clippy)
            run_cargo_clippy
            ;;
        md)
            run_markdown_format
            ;;
        duvet)
            run_duvet
            ;;
        mutants)
            refuse_cargo_mutants
            ;;
        -h|--help|help)
            usage
            ;;
        *)
            echo "unknown task: $1" >&2
            usage >&2
            return 2
            ;;
    esac
}

cd "$ROOT_DIR"

if [ "$#" -eq 0 ]; then
    run_task all
    exit 0
fi

for task in "$@"; do
    run_task "$task"
done
