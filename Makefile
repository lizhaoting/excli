.PHONY: test test-clean test-quiet test-verbose test-summary build check clean help

## Test targets
test:
	@cargo test --all 2>&1 | grep "test result:"

test-clean:
	@./test-clean.sh

test-summary:
	@echo "🧪 Running all tests..." && \
	OUTPUT=$$(cargo test --all 2>&1); \
	echo "$$OUTPUT" | grep "test result:"; \
	echo ""; \
	echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"; \
	if echo "$$OUTPUT" | grep -q "0 failed"; then \
		TOTAL=$$(echo "$$OUTPUT" | grep "test result:" | grep -oP '\d+(?= passed)' | paste -sd+ | bc 2>/dev/null || echo "?"); \
		echo "✅ All $$TOTAL tests PASSED"; \
	else \
		echo "❌ Some tests FAILED"; \
	fi

test-verbose:
	cargo test --all -- --nocapture

test-quiet:
	cargo test --all 2>&1 | tail -50

## Build targets
build:
	cargo build

build-release:
	cargo build --release

check:
	cargo check

## Crate-specific tests
test-core:
	cargo test -p excli-core

test-formula:
	cargo test -p excli-formula

test-io:
	cargo test -p excli-io

test-cli:
	cargo test -p excli-cli

test-ops:
	cargo test -p excli-ops

test-style:
	cargo test -p excli-style

test-i18n:
	cargo test -p excli-i18n

## Utility targets
clean:
	cargo clean

fmt:
	cargo fmt --all

lint:
	cargo clippy --all

help:
	@echo "Available targets:"
	@echo "  make test          - Run all tests (compact summary)"
	@echo "  make test-clean    - Run all tests (hide 0 count, cleanest!)"
	@echo "  make test-summary  - Run all tests with nice summary"
	@echo "  make test-verbose  - Run all tests with output"
	@echo "  make test-quiet    - Run all tests (tail last 50 lines)"
	@echo "  make test-<crate>  - Run specific crate tests"
	@echo "  make build         - Build debug"
	@echo "  make build-release - Build release"
	@echo "  make check         - Check without building"
	@echo "  make clean         - Clean build artifacts"
	@echo "  make fmt           - Format code"
	@echo "  make lint          - Run clippy"
