# Langlog Tooling Specification

Status: draft 0. This document defines traceable requirements for project
tooling that supports requirement tracking and mutation testing.

Normative terms in this document follow RFC 2119, but they apply to repository
tooling rather than to the Langlog language.

## LLG-TOOLS-01 Requirement Checker

- The requirement checker MUST accept one cited implemented test and one cited
  todo test when both use the required annotation shape.
- The requirement checker MUST reject duplicate requirement traces.
- The requirement checker MUST reject detached Duvet annotations.

## LLG-TOOLS-02 Mutation Testing

- The default mutation-test lane MUST run only cited implemented requirement
  tests.
- The task runner MUST validate requirement annotations before running mutation
  testing.
