# Agent Instructions for excli

This file defines repository-wide rules for AI coding agents.

## Scope

- Applies to the entire repository.
- If another instructions file exists in a subdirectory, the more specific file takes precedence for that directory.

## Primary Goals

- Keep behavior stable unless a task explicitly requests behavior changes.
- Prefer small, focused patches over broad refactors.
- Preserve workspace crate boundaries and existing public APIs.

## Repository Structure

- excli-cli: command-line interface and command wiring.
- excli-core: workbook, sheet, cell, and core data model.
- excli-formula: parser, evaluator, and function implementations.
- excli-i18n: language and encoding utilities.
- excli-io: format readers and writers.
- excli-ops: data operations such as filter, sort, and aggregate.
- excli-style: styling and formatting features.

## Required Local Checks

Run these checks before proposing a final patch:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

If a change targets one crate only, crate-scoped tests are acceptable during iteration, but run workspace tests before finalizing.

## Change Rules

- Do not edit lockfiles, generated outputs, or release artifacts unless explicitly requested.
- Do not commit build artifacts under target/, temporary outputs, or logs.
- Do not modify CI, release workflows, or security policy files unless the task requires it.
- Add or update tests for behavior changes.
- Keep CLI help text and docs in sync when flags or command semantics change.

## Rust Style Expectations

- Use idiomatic Rust and explicit error handling.
- Prefer existing error/result types in each crate.
- Avoid introducing unwrap/expect in production paths.
- Keep comments concise and only where intent is not obvious.

## Documentation Expectations

- Update README and/or crate docs when user-facing behavior changes.
- Include concrete command examples for new CLI functionality.

## PR Readiness Checklist

- Scope is minimal and aligned with the request.
- Formatting, linting, and tests pass.
- Docs and tests are updated where needed.
- No unrelated files were modified.
