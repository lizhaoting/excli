---
name: excli-workbooks
description: Inspect, edit, and convert spreadsheet files with the excli command-line tool. Use when a user asks to work on an Excel workbook, preserve an existing XLSX/XLSM package during a supported edit, extract a sheet, or convert spreadsheet data with excli.
---

# excli workbooks

Use `excli` for the workbook operation the user requested. This skill describes the current CLI, not a promise of complete Excel fidelity. The executable is a separate prerequisite and is not bundled with this skill.

## Get the skill and CLI

- Skill package: https://raw.githubusercontent.com/lizhaoting/excli/main/skills/excli-workbooks.zip
- excli source repository: https://github.com/lizhaoting/excli

The skill zip contains instructions only. The repository currently has no verified prebuilt release for this version. To build the CLI from source when the user wants it installed, clone the repository and run `cargo build --release -p excli-cli`; the executable is `target/release/excli-cli`. Keep the user-provided CLI if one is already available. Do not fetch and execute remote install scripts.

## Locate the CLI

Find `excli` or `excli-cli` on `PATH`. If working in the excli source repository, `cargo run -p excli-cli -- <arguments>` runs that checkout; `cargo build -p excli-cli` creates `target/debug/excli-cli`. Check the selected executable's `--help` and the relevant subcommand's `--help` before use. Do not install software merely because this skill was invoked.

## Choose the operation

1. Inspect the input format, requested output, sheet names, and operation. Use `info`, `sheet list`, or `read` when the request needs discovery.
2. For an existing `.xlsx` or `.xlsm`, use the package-preserving commands for supported changes: `replace`, `write --append`, `formula set`, or `sheet add|copy|rename|delete|move|hide|unhide`. They edit the package in place. Preserve the input separately when the requested result should be a new file.
3. Use `convert` for a same-format copy, CSV/TSV export, or XLSX single-sheet extraction. Same-format copies of `.xlsx`, `.xlsm`, `.xlsb`, `.xls`, and `.ods` are byte-for-byte copies when no sheet is selected. A selected-sheet XLSX extraction keeps the selected worksheet and its related package parts where supported.
4. For tabular analysis or a new workbook, use the CLI's read, filter, sort, aggregate, or write commands as appropriate. These operations do not offer a full-fidelity round trip of an existing Excel workbook.
5. Inspect the command result and the actual output file. For edits, verify the targeted cells or sheets with `read` or `sheet list`, and check any preservation requirements that matter to the user's workbook. Report rejected operations and remaining fidelity limits accurately.

## Fidelity boundaries

- Excel reading is partial. The in-memory model exposes many common values and metadata, but does not represent every Excel feature.
- Rebuilding an existing workbook is not a safe general-purpose edit path. Use the supported package-preserving commands for existing XLSX/XLSM files.
- Rich-text replacement works only within a single formatting run. Shared or array formula edits, unsupported references, signed workbooks, and other complex package dependencies may be rejected. Treat a rejection as a boundary; do not retry by rebuilding the workbook unless the user accepts that loss of fidelity.
- CSV and TSV contain one sheet of plain data. Ask for a sheet or use the user's specified sheet when the source has multiple sheets.
- Do not claim that visual formatting, macros, charts, external links, or formula behavior was preserved solely because the command exited successfully. Verify the relevant parts or state the validation limit.

See [references/commands.md](references/commands.md) for tested command shapes and format-specific examples. Read it when selecting or composing a command.
