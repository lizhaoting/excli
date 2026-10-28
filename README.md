# excli — Rust CLI for Spreadsheet Processing

[![Rust](https://img.shields.io/badge/rust-1.70+-brightgreen)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/excli)](https://crates.io/crates/excli)
[![Docs](https://img.shields.io/docsrs/excli)](https://docs.rs/excli)

`excli` is a Rust-based command-line utility and workspace of libraries for reading, transforming, validating, and writing spreadsheet data. The project targets batch-oriented spreadsheet workflows, shell pipelines, and programmatic integration through structured output.

## Overview

### Scope

- **Format interoperability** — Read `.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`, `.csv`, and `.tsv`; write new `.xlsx`, `.csv`, `.tsv`, and `.json` files
- **Pipeline-oriented I/O** — Table output for interactive use and JSON output for automation or downstream tooling
- **Unicode and encoding handling** — Support for multilingual content with encoding detection and normalization utilities
- **Formula processing** — Formula parsing, evaluation, and dependency analysis for spreadsheet expressions
- **Single-binary deployment** — Native executable without a Python, Java, or Node.js runtime dependency
- **Composable command design** — Subcommands are structured for use in Unix-style pipelines and batch jobs

### Operations

| Feature | Status | Details |
|---------|--------|---------|
| **Read** | Partial | Values, formulas, and selected workbook metadata; advanced Excel features are not fully modeled |
| **Write** | Limited | New XLSX and single-sheet CSV/TSV export; existing Excel workbooks require package-preserving operations |
| **Filter** | Supported | Predicate filters, column selection, regular expressions |
| **Sort** | Supported | Multi-key ordering and stable sort behavior |
| **Aggregate** | Supported | Summary statistics, grouped aggregation, pivot-style operations |
| **Transform** | Supported | Projection, transpose, reshape, and column derivation |
| **Merge** | Supported | Horizontal and vertical merge workflows with deduplication support |
| **Search** | Supported | Full-text, regex, and fuzzy matching |
| **Formula** | Partial | Parse, evaluate, inspect dependencies, and edit ordinary formulas in XLSX/XLSM |
| **Styling** | Limited | In-memory style operations; full Excel style roundtrips are not supported |

---

## Editing existing Excel workbooks

XLSX/XLSM reads retain formula text and cached values, rich-text runs, common cell/row/column styles, custom number formats and raw number-format IDs, numeric date/time serials, named ranges with workbook or sheet scope, hidden state, and comments, external workbook link paths and sheet names, common data connection types and source details, the workbook date system, common worksheet data validations (including Office 2010 extension rules), worksheet AutoFilter ranges and criteria, conditional-format ranges and priorities (with simple color scales, data bars, and formula or cell-value rules using supported differential styles decoded, other rules retained as raw XML, and Office 2010 extension groups exposed separately with their raw XML), table names/ranges/columns, common PivotTable names, locations, worksheet sources and field layout, legacy cell comment text and authors, modern threaded comments with reply links, person names, and mention positions, embedded picture bytes and placement, common chart metadata, sheet protection permissions, print areas and repeat rows/columns, and selected sheet layout metadata. Value, Top N count, and dynamic filter criteria are decoded; complex criteria retain their original XML in `FilterType::Custom`. Advanced Excel features are still not fully represented by the in-memory model.

`excli replace report.xlsx --find Old --replace New` edits plain string cells in an existing `.xlsx` or `.xlsm` package. Other workbook parts, including formulas, formatting, and macros, are retained; namespace-prefixed worksheet elements are preserved when a cell is replaced. Rich-text matches contained within individual runs preserve their run formatting, including common shared-string cells converted to inline rich text. Matches crossing run boundaries or using dependent namespaces are rejected without changing the file. Use `--dry-run` to count matching cells first.

`excli sheet hide report.xlsx --sheet Archive` and `excli sheet unhide report.xlsx --sheet Archive` change sheet visibility without rebuilding the workbook. `excli formula set report.xlsx --sheet Sales --cell B2 --formula '=A1+1'` writes an ordinary formula while retaining the other package parts and requests recalculation when Excel opens the file. Formula and typed-value edits preserve worksheet element prefixes. Formula edits to shared or array formulas, rich-text cells, and cells with extension metadata are rejected. Digitally signed workbooks are rejected for editing.

`excli sheet move report.xlsx --sheet Archive --index 0` reorders worksheet tabs while preserving worksheet files, formulas, and package relationships. Sheet-scoped names and workbook view indices are updated to follow their original sheets.

`excli sheet add report.xlsx --name Archive` adds a blank sheet to an existing XLSX/XLSM package while retaining the original worksheet parts. Invalid or duplicate Excel sheet names are rejected.

`excli sheet copy report.xlsx --source Sales --dest SalesCopy` copies a worksheet's XML inside an existing XLSX/XLSM package, retaining its cells, formulas, formatting, and hyperlinks. Explicit references to the source sheet in the copied worksheet and copied sheet-scoped names are changed to the new sheet name. Attached tables receive new package parts, names, and IDs; legacy notes and modern comment threads receive separate parts, with new thread and mention IDs. Worksheet drawings and common charts are copied into separate parts, chart references to the source sheet are updated, and embedded images continue to use the existing media bytes. Common PivotTables receive separate definitions and unique names while retaining their shared source cache. The command rejects unsupported drawing or PivotTable relationships, charts with dependent package relationships, table-name formulas requiring rewriting, or workbooks with VBA projects or calculation chains until those dependencies can be cloned safely.

`excli sheet rename report.xlsx Data Sales` changes a sheet name without rebuilding the workbook. Common worksheet, defined-name, chart, table, and validation formula references are updated; formula string literals remain unchanged. Unresolved references cause the command to fail without changing the file.

`excli sheet delete report.xlsx --sheet Archive` removes a worksheet and its exclusive package parts without rebuilding the remaining sheets. The command leaves the file unchanged if another part still references the sheet, if it is the last visible sheet, or if the workbook contains a VBA project or calculation chain.

`excli write report.xlsx --append --sheet Sales --data '[[12.5,true,"new item"]]'` appends typed values to an existing XLSX/XLSM worksheet while retaining its other package parts. Writing to an existing Excel file without `--append` is rejected to prevent replacing the workbook. Edits to rich-text cells, formula groups, and merged cells outside the top-left anchor are rejected.

For CSV text input, quoted commas and newlines are parsed correctly: `excli write report.xlsx --append --sheet Sales --data '"item, one",12.5'`.

Other commands that rebuild an existing Excel workbook currently fail instead of silently dropping formulas, formatting, or workbook features. Rebuilding `.xlsm`, `.xls`, `.ods`, and other unsupported output formats also fails explicitly. CSV/TSV export requires one sheet; select a sheet during conversion, for example `excli convert report.xlsx report.csv --sheet Sales`.

For an unchanged workbook in the same format, `excli convert report.xlsx backup.xlsx` copies the file byte-for-byte. The same applies to `.xlsm`, `.xlsb`, `.xls`, and `.ods`. This path requires no `--sheet` selection. For XLSX, `excli convert report.xlsx selected.xlsx --sheet Sales` extracts one worksheet through package edits, retaining that sheet's XML and related parts. Extraction fails without changing the output if a retained part still references a removed sheet.

---

## Installation

### From Cargo

```bash
cargo install excli
```

### From Binary (macOS / Linux / Windows)

```bash
# macOS (Intel)
curl -L https://github.com/excli/excli/releases/download/v0.1.0/excli-x86_64-apple-darwin.tar.gz | tar xz
sudo mv excli /usr/local/bin/

# macOS (ARM)
curl -L https://github.com/excli/excli/releases/download/v0.1.0/excli-aarch64-apple-darwin.tar.gz | tar xz
sudo mv excli /usr/local/bin/

# Linux
curl -L https://github.com/excli/excli/releases/download/v0.1.0/excli-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv excli /usr/local/bin/
```

### From Source

```bash
git clone https://github.com/excli/excli.git
cd excli
cargo build --release
./target/release/excli-cli --version
```

---

## Quick Start

### Read a Spreadsheet

```bash
# Display as table
excli read data.xlsx

# Show first 10 rows
excli read data.xlsx --nrows 10

# Output as JSON for downstream tooling
excli read data.xlsx --json

# Get statistics
excli read data.xlsx --stats

# Specific sheet and range
excli read data.xlsx --sheet Sales --range A1:D100
```

### Filter Data

```bash
# Filter rows with a predicate expression
excli filter data.xlsx --where "age > 30 AND status = 'active'"

# Fuzzy match selected columns
excli filter data.xlsx --search "John" --columns name,email

# Apply a regular-expression filter
excli filter data.xlsx --regex "column:^prod_.*" 
```

### Transform & Aggregate

```bash
# Sort by multiple columns
excli sort data.xlsx --by Name ASC --then Age DESC

# Grouped aggregation
excli aggregate data.xlsx --group-by Department --count --sum Salary

# Pivot table
excli pivot data.xlsx --rows Region --columns Category --values Sales

# Transpose
excli transform data.xlsx --transpose
```

### Convert Formats

```bash
# Excel to CSV
excli convert sales.xlsx output.csv --sheet Sales

# CSV to Excel
excli convert data.csv result.xlsx

# Excel to JSON
excli read data.xlsx --json > data.json

# Format conversion across spreadsheet types
excli read input.xlsx --sheet Report | excli write output.ods
```

### Formula Evaluation

```bash
# Parse and evaluate an expression
excli calc --formula "=SUM(A1:A10) * 1.2"

# Derive a column from an expression
excli transform data.xlsx --apply "=Column1 + Column2 AS Sum"

# Inspect formula dependencies
excli formula data.xlsx --analyze --sheet Sheet1
```

---

## Architecture

### Workspace Layout

```
excli/
├── excli-cli          # CLI interface, command handlers
├── excli-core         # Core data structures (Cell, Sheet, Workbook)
├── excli-formula      # Formula parser/evaluator (200+ functions)
├── excli-i18n         # Internationalization (encoding, collation, normalization)
├── excli-io           # Format I/O (Excel, ODS, CSV, etc.)
├── excli-ops          # Data operations (sort, filter, aggregate, merge)
└── excli-style        # Styling (colors, themes, conditional formatting)
```

### Design Principles

1. **Operational separation** — The workspace is split into focused crates for I/O, data structures, formulas, styling, and CLI integration.
2. **Pipeline composition** — Subcommands are intended to be chained with shell pipelines when processing tabular data.
3. **Machine-readable output** — Structured output modes support automation and service integration.
4. **Typed error handling** — Domain-specific errors are surfaced through Rust error types and CLI diagnostics.
5. **Batch-processing focus** — Implementations favor predictable execution in non-interactive workflows.

---

## Command Reference

The current CLI exposes the following top-level operations:

```bash
excli read <file>                 # Read worksheet data
excli write <output>              # Write worksheet data
excli sheet <subcommand>          # Sheet management operations
excli formula <subcommand>        # Formula inspection and editing
excli calc                        # Evaluate expressions or workbook formulas
excli filter <file>               # Filter rows
excli sort <file>                 # Sort rows
excli agg <file>                  # Aggregate or pivot data
excli merge <file1> <file2>       # Join two datasets
excli concat <files...>           # Concatenate workbooks
excli transpose <file>            # Transpose a sheet
excli split <file>                # Split a file by rows or column values
excli sample <file>               # Sample rows
excli search <file>               # Search workbook content
excli replace <file>              # Replace workbook content
excli info <file>                 # Show file metadata
excli convert <input> <output>    # Convert between supported formats
excli clean <file>                # Run cleaning operations
excli diff <file1> <file2>        # Compare two files
excli batch <glob> <command...>   # Run a command across multiple files
excli detect-encoding <file>      # Detect file encoding
excli detect-lang <file>          # Detect content language
excli validate <file>             # Compare against a reference file
excli describe [command]          # Print command schema metadata
```

### `sheet` subcommands

```bash
excli sheet list <file>                # List sheets
excli sheet info <file> --sheet NAME   # Show sheet metadata
excli sheet rename <file> OLD NEW      # Rename a sheet
excli sheet add <file> --name NAME     # Add a sheet
excli sheet delete <file> --sheet NAME # Delete a sheet
excli sheet copy <file> --source A --dest B
excli sheet move <file> --sheet NAME --index N
excli sheet hide <file> --sheet NAME
excli sheet unhide <file> --sheet NAME
```

### `formula` subcommands

```bash
excli formula read <file>              # Read formulas
excli formula set <file> --sheet NAME --cell B2 --formula EXPR
excli formula deps <file> --cell A1    # Show dependencies
excli formula dependents <file> --cell A1
excli formula errors <file>            # List formula errors
excli formula circular <file>          # Detect circular references
```

---

## Examples

### Example 1: Data Pipeline

```bash
# Read Excel → Filter → Sort → Write CSV
excli read sales.xlsx \
  | excli filter --where "revenue > 10000" \
  | excli sort --by date DESC \
  | excli write output.csv
```

### Example 2: Aggregation

```bash
# Group by region, count records, sum sales
excli aggregate data.xlsx \
  --group-by "Region" \
  --count \
  --sum "Sales" \
  --avg "Commission" \
  --json
```

### Example 3: Conversion with Validation

```bash
# Convert a legacy workbook and validate the result
excli read old_data.xls \
  --sheet "Data" \
  | excli transform --deduplicate \
  | excli validate --schema schema.json \
  | excli write new_data.xlsx
```

### Example 4: Structured Output

```bash
# Emit a JSON schema for downstream consumers
excli read data.xlsx --json-schema | jq .

# Send structured output to an external service
excli aggregate data.xlsx --group-by Category --sum Sales --json | \
  curl -X POST https://api.openai.com/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d @-
```

---

## Format Support

### Read

| Format | Extension | Status | Notes |
|--------|-----------|--------|-------|
| Excel | `.xlsx`, `.xlsm` | Partial | Values, formulas, names, visibility, and selected layout metadata; advanced features are not fully modeled |
| Excel Binary | `.xls`, `.xlsb` | Partial | Values and formulas through calamine; workbook features are not fully modeled |
| OpenDocument | `.ods` | Partial | Values and formulas through calamine |
| CSV/TSV | `.csv`, `.tsv` | Full | Auto-delimiter detection |
| JSON | `.json` | Full | Array/object format |
| Parquet | `.parquet` | Partial | Column-oriented |
| XML | `.xml` | Partial | Office 2003 XML |

### Write

| Format | Status | Notes |
|--------|--------|-------|
| Excel `.xlsx` | Limited | New workbook export; targeted edits preserve existing packages |
| CSV/TSV | Supported | Single-sheet export with quoting |
| OpenDocument `.ods` | Unsupported | Existing files cannot be written |
| JSON | Supported | Schema export |
| Parquet | Partial | Experimental |

---

## Library Usage

The workspace crates can also be consumed directly from Rust code:

```rust
use excli_core::workbook::Workbook;
use excli_io::reader::read_excel;

fn main() -> Result<()> {
    // Read Excel file
    let workbook = read_excel("data.xlsx")?;
    
    // Access sheets
    for sheet in workbook.sheets() {
        println!("Sheet: {}", sheet.name());
        println!("Rows: {}", sheet.row_count());
    }
    
    // Get cell value
    let cell = workbook.cell("Sheet1", "A1")?;
    println!("Value: {}", cell.value);
    
    Ok(())
}
```

Generate API documentation locally with:

```bash
cargo doc --open
```

---

## Testing

The repository includes unit tests, integration tests, and documentation tests:

```bash
# Run all tests
cargo test

# Run tests with stdout/stderr
cargo test -- --nocapture

# Run tests for specific crate
cargo test -p excli-core

# Run doc-tests
cargo test --doc

# Run the project test shortcut with additional output
make test-verbose
```

The current workspace contains tests across all primary crates. Use crate-scoped commands during local development when iterating on a single subsystem.

---

## Performance Notes

Sample measurements from local development on Apple Silicon:

| Operation | Input | Time | Memory |
|-----------|-------|------|--------|
| Read | 100 MB `.xlsx` | 1.2 s | 45 MB |
| Filter (50% rows) | 100 MB `.xlsx` | 0.8 s | 50 MB |
| Sort | 100 MB `.csv` | 1.5 s | 60 MB |
| Aggregate | 100 MB `.xlsx` | 0.9 s | 52 MB |
| Write | 100 MB dataset | 0.7 s | 40 MB |

Performance characteristics depend on file format, worksheet layout, formula density, and output mode. Reproduce measurements in your target environment before using them as capacity estimates.

---

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for workflow and review guidelines.

## Agent Usage

This repository is configured to work well with common coding agents. To keep automated edits reliable and reviewable, follow the shared agent rules in [AGENTS.md](AGENTS.md).

### Agent Quick Start

```bash
# Format
cargo fmt --all -- --check

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Test
cargo test --workspace
```

Before opening a PR (human or agent-authored), make sure all three commands pass locally.

### Contribution Areas

- **Format support** — Additional readers or writers for tabular and analytical formats
- **Formula coverage** — Implementation of unsupported spreadsheet functions
- **Performance work** — Profiling, hot-path optimization, and memory behavior improvements
- **Localization** — Additional language coverage for CLI output and documentation
- **Documentation** — Operational examples, tutorials, and API references

### Development Setup

```bash
# Clone and enter directory
git clone https://github.com/excli/excli.git
cd excli

# Build the workspace
cargo build

# Run tests
cargo test

# Run formatting and lints
cargo fmt --check
cargo clippy

# Generate local API documentation
cargo doc --open
```

---

## License

Licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.

---

## Roadmap

### v0.2.0
- [ ] Parquet write support
- [ ] Parallel processing for large files
- [ ] Formula auto-complete in interactive mode
- [ ] SQL-like query language (SELECT, WHERE, JOIN)

### v0.3.0
- [ ] Interactive REPL mode
- [ ] Visual diff tool for spreadsheets
- [ ] Data profiling and quality reports
- [ ] GraphQL API endpoint

### v1.0.0
- [ ] Broader Excel compatibility validation
- [ ] Performance benchmarks published


---

## FAQ

**Q: Can I use `excli` as a library?**  
A: Yes. The workspace is split into crates that can be consumed directly from Rust projects. See the generated API documentation for crate-level interfaces.

**Q: Does `excli` execute Excel macros?**  
A: No. Macro execution is intentionally out of scope. Macro-bearing files such as `.xlsm` can be read and written without executing embedded code.

**Q: Is the CLI suitable for containers or serverless jobs?**  
A: The distribution model is a native binary, which makes it practical for containerized and batch execution environments.

---

## Support

- [Documentation](https://docs.rs/excli)
- [Discussions](https://github.com/excli/excli/discussions)
- [Issue Tracker](https://github.com/excli/excli/issues)

---

## Acknowledgments

This project builds on the Rust ecosystem, including:
- [calamine](https://github.com/tafia/calamine) — Excel format parsing
- [serde](https://github.com/serde-rs/serde) — Serialization
- [clap](https://github.com/clap-rs/clap) — CLI framework

---

Maintained by the excli contributors.
