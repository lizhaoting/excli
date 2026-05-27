#!/bin/bash
# Fast test summary with total count

echo "🧪 excli test suite"
echo ""

# Run tests and filter results
cargo test --all 2>&1 | tee /tmp/excli_test.log | grep "test result:"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Calculate totals using awk (macOS compatible)
TOTALS=$(grep "test result:" /tmp/excli_test.log | awk '{
    for(i=1; i<=NF; i++) {
        if($i == "passed;") passed_count += $(i-1)
        if($i == "failed;") failed_count += $(i-1)
    }
}
END {
    print passed_count, failed_count
}')

TOTAL_PASSED=$(echo $TOTALS | awk '{print $1}')
TOTAL_FAILED=$(echo $TOTALS | awk '{print $2}')

if [ -z "$TOTAL_PASSED" ]; then TOTAL_PASSED=0; fi
if [ -z "$TOTAL_FAILED" ]; then TOTAL_FAILED=0; fi

if [ "$TOTAL_FAILED" -eq 0 ]; then
    echo "✅ Summary: $TOTAL_PASSED tests passed"
else
    echo "❌ Summary: $TOTAL_PASSED passed, $TOTAL_FAILED failed"
    exit 1
fi
