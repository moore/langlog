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
    echo "==> cargo run -p langlog-xtask -- check-requirements"
    cargo run -p langlog-xtask -- check-requirements
    echo "==> duvet report --require-tests true"
    duvet report --require-tests true
}

run_playground() {
    echo "==> validate playground static files"
    test -f playground/index.html
    test -f playground/app.js
    test -f playground/style.css
    test -f REFERENCE.md
    test -f TUTORIAL.md
    test -f SPEC.md
    echo "==> cargo build -p langlog-playground-wasm --target wasm32-unknown-unknown"
    cargo build -p langlog-playground-wasm --target wasm32-unknown-unknown
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
        cat >&2 <<'EOF'
wasm-bindgen CLI is required to generate playground/pkg
install it with `cargo install wasm-bindgen-cli`
EOF
        return 1
    fi
    echo "==> assemble target/playground-site"
    rm -rf target/playground-site
    mkdir -p target/playground-site/pkg
    cp playground/index.html playground/app.js playground/style.css target/playground-site/
    cp REFERENCE.md TUTORIAL.md SPEC.md target/playground-site/
    cp -R examples target/playground-site/
    echo "==> wasm-bindgen --target web"
    wasm-bindgen \
        --target web \
        --out-dir target/playground-site/pkg \
        target/wasm32-unknown-unknown/debug/langlog_playground_wasm.wasm
}

run_playground_serve() {
    run_playground
    local port="${PORT:-8000}"
    echo "==> serving playground at http://127.0.0.1:${port}/"
    python3 -m http.server "$port" --directory target/playground-site
}

run_cargo_mutants() {
    echo "==> cargo run -p langlog-xtask -- check-requirements"
    cargo run -p langlog-xtask -- check-requirements
    echo "==> cargo mutants"
    cargo mutants
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
  playground Build the browser playground Wasm package
  playground-serve Build and serve the browser playground on PORT or 8000
  mutants  Run requirement-only cargo-mutants after validating requirement annotations
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
        playground)
            run_playground
            ;;
        playground-serve)
            run_playground_serve
            ;;
        mutants)
            run_cargo_mutants
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
