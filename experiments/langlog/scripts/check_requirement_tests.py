#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[1]
RUST_FILES = sorted((ROOT_DIR / "crates").glob("**/*.rs"))

FUNCTION_RE = re.compile(r"^\s*fn\s+([A-Za-z0-9_]+)\s*\(")
SPEC_RE = re.compile(r"^\s*//=\s+SPEC\.md#(.+?)\s*$")
TYPE_RE = re.compile(r"^\s*//=\s+type=(test|todo)\s*$")
QUOTE_RE = re.compile(r"^\s*//#\s+(.+?)\s*$")


def collect_annotation_block(lines: list[str], fn_line_index: int) -> list[tuple[int, str]]:
    block: list[tuple[int, str]] = []
    index = fn_line_index - 1
    while index >= 0:
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            if block:
                block.append((index + 1, line))
            index -= 1
            continue
        if stripped.startswith("#[") or stripped.startswith("//=") or stripped.startswith("//#"):
            block.append((index + 1, line))
            index -= 1
            continue
        break
    block.reverse()
    return block


def main() -> int:
    errors: list[str] = []
    seen_requirements: dict[tuple[str, str], tuple[Path, int, str]] = {}
    consumed_annotations: set[tuple[Path, int]] = set()
    validated_requirement_tests = 0
    validated_todo_tests = 0

    for path in RUST_FILES:
        lines = path.read_text().splitlines()
        for index, line in enumerate(lines):
            function_match = FUNCTION_RE.match(line)
            if not function_match:
                continue

            fn_name = function_match.group(1)
            block = collect_annotation_block(lines, index)

            test_attrs = [entry for entry in block if entry[1].strip() == "#[test]"]
            spec_refs = [SPEC_RE.match(entry[1]).group(1) for entry in block if SPEC_RE.match(entry[1])]
            type_refs = [TYPE_RE.match(entry[1]).group(1) for entry in block if TYPE_RE.match(entry[1])]
            quotes = [QUOTE_RE.match(entry[1]).group(1) for entry in block if QUOTE_RE.match(entry[1])]

            if not spec_refs and not type_refs and not quotes:
                continue

            for line_number, text in block:
                stripped = text.strip()
                if SPEC_RE.match(stripped) or TYPE_RE.match(stripped) or QUOTE_RE.match(stripped):
                    consumed_annotations.add((path, line_number))

            if len(test_attrs) != 1 or len(spec_refs) != 1 or len(type_refs) != 1 or len(quotes) != 1:
                errors.append(
                    f"{path.relative_to(ROOT_DIR)}:{index + 1}: {fn_name} must have exactly one "
                    f"#[test], one //= SPEC.md#..., one //= type=..., and one //# ... "
                    f"(found test={len(test_attrs)}, spec={len(spec_refs)}, type={len(type_refs)}, quote={len(quotes)})"
                )
                continue

            key = (spec_refs[0], quotes[0])
            previous = seen_requirements.get(key)
            if previous is not None:
                prev_path, prev_line, prev_fn = previous
                errors.append(
                    f"{path.relative_to(ROOT_DIR)}:{index + 1}: {fn_name} duplicates requirement "
                    f"{spec_refs[0]!r} / {quotes[0]!r}, already used by "
                    f"{prev_path.relative_to(ROOT_DIR)}:{prev_line} ({prev_fn})"
                )
                continue

            seen_requirements[key] = (path, index + 1, fn_name)
            trace_type = type_refs[0]

            if trace_type == "test":
                if not fn_name.startswith("requirement_"):
                    errors.append(
                        f"{path.relative_to(ROOT_DIR)}:{index + 1}: {fn_name} must use the "
                        "requirement_ prefix for type=test traces"
                    )
                    continue
                validated_requirement_tests += 1
            elif trace_type == "todo":
                if not fn_name.startswith("todo_"):
                    errors.append(
                        f"{path.relative_to(ROOT_DIR)}:{index + 1}: {fn_name} must use the "
                        "todo_ prefix for type=todo traces"
                    )
                    continue
                validated_todo_tests += 1
            else:
                errors.append(
                    f"{path.relative_to(ROOT_DIR)}:{index + 1}: {fn_name} uses unsupported trace type "
                    f"{trace_type!r}"
                )
                continue

    for path in RUST_FILES:
        lines = path.read_text().splitlines()
        for index, line in enumerate(lines, start=1):
            if (path, index) in consumed_annotations:
                continue
            if SPEC_RE.match(line) or TYPE_RE.match(line) or QUOTE_RE.match(line):
                errors.append(
                    f"{path.relative_to(ROOT_DIR)}:{index}: Duvet annotation must be attached to a test function"
                )

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print(
        "validated "
        f"{validated_requirement_tests} requirement tests and {validated_todo_tests} todo tests"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
