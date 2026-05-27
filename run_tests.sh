#!/bin/bash

# Simple test runner with aggregated summary
set -e

echo "🧪 Running excli test suite..."
echo ""

# Run tests and capture output
OUTPUT=$(cargo test --all 2>&1)

# Extract test counts
PASSED=$(echo "$OUTPUT" | grep "test result:" | grep -oP '\d+(?= passed)' | awk '{sum+=$1} END {print sum}')
FAILED=$(echo "$OUTPUT" | grep "test result:" | grep -oP '\d+(?= failed)' | awk '{sum+=$1} END {print sum}')

# Extract per-crate results
echo "📊 Per-crate results:"
echo "$OUTPUT" | grep "test result:" | nl

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ -z "$PASSED" ]; then
    PASSED=0
fi
if [ -z "$FAILED" ]; then
    FAILED=0
fi

if [ "$FAILED" -eq 0 ]; then
    echo "✅ All tests passed: $PASSED passed, $FAILED failed"
else
    echo "❌ Some tests failed: $PASSED passed, $FAILED failed"
    exit 1
fi
