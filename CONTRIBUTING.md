# Contributing to excli

Thank you for your interest in contributing to excli! This document provides guidelines and instructions for contributing.

## Code of Conduct

Be respectful, inclusive, and constructive. We follow the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct).

## Getting Started

### Prerequisites

- Rust 1.70+ ([install](https://rustup.rs/))
- Git
- macOS, Linux, or Windows with WSL

### Setup

```bash
# Clone the repository
git clone https://github.com/excli/excli.git
cd excli

# Install development dependencies (optional)
rustup update
cargo install cargo-watch  # For live testing

# Verify setup
cargo test
```

## Development Workflow

Shared instructions for AI coding agents are documented in [AGENTS.md](AGENTS.md). If you use an agent to prepare a change, ensure it follows these rules before opening a PR.

### 1. Create an Issue First

Before starting major work, [create an issue](https://github.com/excli/excli/issues) describing:
- What problem you're solving
- Your proposed solution
- Any design questions

This prevents duplicate work and ensures alignment.

### 2. Create a Feature Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 3. Make Your Changes

Follow these guidelines:

#### Code Style

- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- Write idiomatic Rust
- Add doc comments for public items

```rust
/// Parse and evaluate an Excel formula.
///
/// # Arguments
/// * `formula` - Excel formula string (e.g., "=SUM(A1:A10)")
/// * `context` - Evaluation context with cell references
///
/// # Returns
/// The calculated value or an error
///
/// # Examples
/// ```
/// let result = evaluate_formula("=2+3", &context)?;
/// assert_eq!(result, 5.0);
/// ```
pub fn evaluate_formula(formula: &str, context: &Context) -> Result<f64> {
    // implementation
}
```

#### Testing

- Write tests for new functionality
- Run `cargo test` before committing
- Aim for >80% code coverage

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operation() {
        assert_eq!(some_function(42), 84);
    }

    #[test]
    fn test_error_handling() {
        let result = risky_function("invalid");
        assert!(result.is_err());
    }
}
```

#### Performance

- Don't optimize prematurely
- Profile before optimizing: `cargo flamegraph`
- Add benchmarks for critical paths

### 4. Commit with Clear Messages

```bash
# Good commits
git commit -m "feat: Add support for TEXTJOIN function"
git commit -m "fix: Handle NaN values in SUM aggregation"
git commit -m "docs: Clarify formula syntax in README"
git commit -m "test: Add edge cases for CellValue coercion"
git commit -m "perf: Optimize sheet iteration with iterator pattern"

# Avoid
git commit -m "stuff"
git commit -m "fixes things"
git commit -m "wip"
```

Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation
- `test:` Test additions
- `perf:` Performance improvement
- `refactor:` Code restructuring
- `chore:` Maintenance

### 5. Run Quality Checks

```bash
# Format code
cargo fmt

# Check for common mistakes
cargo clippy -- -D warnings

# Run all tests
cargo test

# Build documentation
cargo doc --no-deps --open
```

### 6. Push and Create a PR

```bash
git push origin feature/your-feature-name
```

Then open a [Pull Request](https://github.com/excli/excli/pulls) with:
- Clear title: "Add TEXTJOIN function" or "Fix NaN handling in SUM"
- Description of changes
- Reference to related issue (#123)
- Screenshots/examples if visual

## Architecture Guidelines

### Adding a New Excel Function

1. **Define in `excli-formula/src/functions.rs`**

```rust
define_function! {
    fn my_new_function(arg1: f64, arg2: f64) -> f64 {
        arg1 + arg2
    }
}
```

2. **Add tests**

```rust
#[test]
fn test_my_new_function() {
    let evaluator = Evaluator::new();
    let result = evaluator.eval("=MY_NEW_FUNCTION(1, 2)").unwrap();
    assert_eq!(result, 3.0);
}
```

3. **Update function registry**

Functions are auto-registered via macros—no manual registry needed.

### Adding a New File Format

1. **Create reader in `excli-io/src/format_io.rs`**
2. **Implement `FormatReader` trait**
3. **Add format detection in `excli-io/src/format_detect.rs`**
4. **Write integration tests**

### Adding a New CLI Command

1. **Define in `excli-cli/src/commands.rs`**
2. **Add to clap command enum**
3. **Implement command handler**
4. **Add help text and examples**

```rust
#[derive(Parser)]
pub enum Commands {
    /// Read spreadsheet data
    #[command(name = "read")]
    Read(ReadArgs),
    
    /// Your new command
    #[command(name = "mynew")]
    MyNew(MyNewArgs),
}

#[derive(Parser)]
pub struct MyNewArgs {
    /// Input file path
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Optional flag
    #[arg(short, long)]
    flag: bool,
}
```

## Testing Strategy

### Unit Tests

Place in-file with `#[cfg(test)]` modules:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_something() { ... }
}
```

### Integration Tests

In `tests/` directory:

```rust
// tests/integration_test.rs
#[test]
fn test_end_to_end() {
    let workbook = read_excel("fixtures/sample.xlsx").unwrap();
    assert_eq!(workbook.sheet_count(), 1);
}
```

### Doc Tests

In doc comments with examples:

```rust
/// # Examples
/// ```
/// use excli_core::cell::CellValue;
/// assert_eq!(CellValue::Number(42.0).coerce_to_number(), 42.0);
/// ```
pub fn coerce_to_number(&self) -> f64 { ... }
```

### Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_name

# With output
cargo test -- --nocapture

# Doc tests only
cargo test --doc

# Specific crate
cargo test -p excli-core
```

## Debugging

### Enable Logging

```bash
RUST_LOG=debug cargo run -- read data.xlsx
```

### Use Debugger (VS Code)

1. Install CodeLLDB extension
2. Add to `.vscode/launch.json`:

```json
{
    "name": "Debug",
    "type": "lldb",
    "request": "launch",
    "cargo": {
        "args": [
            "build",
            "--bin",
            "excli-cli",
            "--release"
        ]
    },
    "args": ["read", "test.xlsx"]
}
```

## Documentation

### Update README

When adding features, update the relevant section in `README.md`.

### Inline Comments

For complex logic, explain the "why":

```rust
// ✅ Good - explains intent
// Use linear search here because the list is small (<100 items)
// and we need to preserve insertion order
for item in items.iter() { ... }

// ❌ Bad - just restates code
// Iterate through items
for item in items.iter() { ... }
```

### API Docs

All public items need doc comments:

```rust
/// Brief description.
///
/// Longer explanation if needed.
///
/// # Errors
/// Returns an error if X happens.
///
/// # Panics
/// Panics if Y condition.
pub fn important_function() -> Result<T> { }
```

## Review Process

1. **Automated checks** (GitHub Actions)
   - Cargo build
   - `cargo fmt --check`
   - `cargo clippy`
   - `cargo test`

2. **Code review** (maintainers)
   - Correctness
   - Performance
   - Testing
   - Documentation
   - Style consistency

3. **Approval and merge**
   - At least 1 approval required
   - All checks must pass

## Performance Considerations

### Profiling

```bash
# Flamegraph
cargo install flamegraph
cargo flamegraph --bin excli-cli -- read large_file.xlsx

# Benchmarking
cargo bench -p excli-core
```

### Memory Usage

Use `valgrind` or heaptrack:

```bash
valgrind --leak-check=full ./target/debug/excli-cli read data.xlsx
```

## Filing Issues

Include:

- **Title**: Clear, specific
- **Description**: What's happening and what should happen
- **Steps to reproduce**: Exact commands to trigger
- **Environment**: OS, Rust version, excli version
- **Files**: Attach minimal test case

Example:

```
Title: Filter command crashes on CSV with Unicode BOM

Description:
When running `excli filter` on UTF-8 BOM-encoded CSV files, 
the command crashes with a parsing error.

Steps to Reproduce:
1. Create UTF-8 BOM CSV file
2. Run: excli filter data.csv --where "col1 > 5"
3. See crash

Environment:
- OS: macOS 13.2
- Rust: 1.70.0
- excli: 0.1.0

Attachment: test_bom.csv
```

## Recognition

Contributors are recognized in:
- Release notes (CHANGELOG.md)
- Contributors section of README
- GitHub contributors page

## Questions?

- Check [existing issues](https://github.com/excli/excli/issues)
- Ask in [Discussions](https://github.com/excli/excli/discussions)
- Email: support@excli.io

---

**Happy coding! 🚀**
