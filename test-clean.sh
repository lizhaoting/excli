#!/bin/bash
# Clean test runner - shows only unit tests with content, hides empty doc-tests

echo "🧪 excli unit tests"
echo ""

cargo test --all 2>&1 | tee /tmp/excli_test.log > /dev/null

# Show only actual unit test results (not doc-tests, and skip lines with 0 passed)
echo "📊 Unit test results (with tests):"
grep "test result:" /tmp/excli_test.log | grep -v "Doc-tests" | grep -v " 0 passed;" | head -20

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Calculate totals (all test results except doc-tests)
TOTALS=$(grep "test result:" /tmp/excli_test.log | grep -v "Doc-tests" | awk '{
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
    echo "✅ All unit tests passed: $TOTAL_PASSED total"
else
    echo "❌ Some tests failed: $TOTAL_PASSED passed, $TOTAL_FAILED failed"
    exit 1
fi

echo ""
echo "💡 What are those 0s?"
echo "   • main.rs (binary) has no unit tests - normal"
echo "   • Doc-tests = /// code examples in comments - all empty (normal)"
