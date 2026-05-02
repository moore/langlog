# Langlog Tooling Specification

Status: draft 0. This document defines traceable requirements for project
tooling that supports requirement tracking and mutation testing.

Normative terms in this document follow RFC 2119, but they apply to repository
tooling rather than to the Langlog language.

## LLG-TOOLS-01 Requirement Checker

- The requirement checker MUST accept one cited implemented test and one cited
  todo test when both use the required annotation shape.
- The requirement checker MUST ignore uncited helper functions.
- The requirement checker MUST reject cited tests that are missing the test
  attribute, spec reference, trace type, or requirement quote.
- The requirement checker MUST reject duplicate requirement traces.
- Duplicate-trace diagnostics MUST report the original traced test line.
- The requirement checker MUST reject detached Duvet annotations.
- Detached-annotation diagnostics MUST reject detached spec references, trace
  types, and requirement quotes.
- Requirement-checker diagnostics MUST report paths relative to the workspace
  root when possible.
- Requirement annotation collection MUST preserve source line numbers for
  attached annotation blocks.
- The `check-requirements` command MUST validate the current workspace and
  print a success summary.

## LLG-TOOLS-02 Mutation Testing

- The default mutation-test lane MUST run only cited implemented requirement
  tests.
- Native mutation testing MAY exclude wasm-only `JsValue` conversion shells
  when the pure adapter result model remains covered by cited tests.
- The task runner MUST validate requirement annotations before running mutation
  testing.
