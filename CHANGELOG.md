# Changelog

All notable changes to the excli project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Multi-sheet workbook support
- Formula evaluation engine with 200+ functions
- CSV/TSV format detection with automatic encoding detection
- ODS (OpenDocument Spreadsheet) read support
- Data transformation operations (transpose, reshape, pivot)
- Full-text search across spreadsheets
- Data aggregation with GROUP BY support
- Conditional formatting preservation
- Unicode normalization for international text
- Progress indicators with terminal-friendly output
- JSON schema generation for AI integration
- Comprehensive error recovery suggestions

### Changed
- Refactored CLI to use modular command pattern
- Improved memory usage for large files (streaming where possible)
- Enhanced error messages with actionable guidance
- Optimized cell coercion for better performance

### Fixed
- Fixed handling of NaN values in numeric operations
- Corrected formula dependency analysis for circular references
- Fixed Unicode normalization edge cases
- Improved CSV detection for various dialects

### Security
- Input validation on all file formats
- Safe handling of malformed spreadsheets (no crashes)
- No arbitrary code execution (intentional design)

---

## [0.1.0] - 2026-05-27

### Added
- Initial release
- Read support for Excel (.xlsx, .xls, .xlsm, .xlsb)
- Read support for OpenDocument (.ods)
- Read support for CSV/TSV with auto-detection
- Write support for Excel and CSV
- Basic data filtering and sorting
- CLI interface with 25+ commands
- JSON output for programmatic access
- Comprehensive test suite (672+ tests)
- Multi-language support for content (Chinese, Japanese, Korean, Arabic, Russian, Thai)
- Formula parsing and basic evaluation

### Known Issues
- Parquet format write support is experimental
- Some advanced Excel features (shapes, charts) are read but not written
- Macros in .xlsm files are preserved but not executed (intentional)

---

## Versioning Policy

We follow Semantic Versioning:

- **MAJOR** (0.x.0): Breaking API changes
- **MINOR** (x.1.0): New features, backward compatible
- **PATCH** (x.x.1): Bug fixes, security patches

---

## Release Schedule

- Minor releases: Monthly (if changes exist)
- Patch releases: As needed (within 48 hours of critical fixes)
- Major releases: Annually (or when justified)

---

## How to Contribute Changes

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.
