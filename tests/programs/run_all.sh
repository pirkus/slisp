#!/usr/bin/env bash

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
output_dir="$script_dir/target"

mkdir -p "$output_dir"

# Initialize test counters
declare -a successful_tests=()
declare -a failed_tests=()
declare -a timed_out_tests=()
declare -a compilation_failed=()
total_tests=0

pushd "$repo_root" > /dev/null || exit 1

echo "========================================="
echo "Running slisp test suite"
echo "========================================="
echo

while IFS= read -r -d '' file; do
    rel_path="${file#$repo_root/}"
    program_name="$(basename "${file%.slisp}")"
    binary_path="$output_dir/$program_name"
    rel_binary="tests/programs/target/$program_name"

    ((total_tests++))

    echo "[$total_tests] Compiling $rel_path"
    if cargo run --quiet -- --compile -o "$rel_binary" "$rel_path" 2>&1; then
        echo "  ✓ Compiled successfully"
    else
        echo "  ✗ Compilation failed"
        compilation_failed+=("$program_name")
        echo
        continue
    fi

    echo "  Running $rel_binary"

    # Run with timeout (10 seconds per test)
    timeout 10s "$binary_path" > /dev/null 2>&1
    status=$?

    if [ $status -eq 0 ]; then
        echo "  ✓ PASSED (exit code 0)"
        successful_tests+=("$program_name")
    elif [ $status -eq 124 ]; then
        echo "  ⏱ TIMEOUT (exceeded 10s)"
        timed_out_tests+=("$program_name")
    else
        echo "  ✗ FAILED (exit code $status)"
        failed_tests+=("$program_name:$status")
    fi
    echo
done < <(find "$script_dir" -type f -name '*.slisp' -print0 | sort -z)

popd > /dev/null || exit 1

# Generate summary report
echo
echo "========================================="
echo "TEST SUMMARY"
echo "========================================="
echo "Total tests:      $total_tests"
echo "Successful:       ${#successful_tests[@]}"
echo "Failed:           ${#failed_tests[@]}"
echo "Timed out:        ${#timed_out_tests[@]}"
echo "Compilation failed: ${#compilation_failed[@]}"
echo

if [ ${#successful_tests[@]} -gt 0 ]; then
    echo "✓ Successful tests (${#successful_tests[@]}):"
    for test in "${successful_tests[@]}"; do
        echo "  - $test"
    done
    echo
fi

if [ ${#failed_tests[@]} -gt 0 ]; then
    echo "✗ Failed tests (${#failed_tests[@]}):"
    for test in "${failed_tests[@]}"; do
        test_name="${test%:*}"
        exit_code="${test#*:}"
        echo "  - $test_name (exit code: $exit_code)"
    done
    echo
fi

if [ ${#timed_out_tests[@]} -gt 0 ]; then
    echo "⏱ Timed out tests (${#timed_out_tests[@]}):"
    for test in "${timed_out_tests[@]}"; do
        echo "  - $test"
    done
    echo
fi

if [ ${#compilation_failed[@]} -gt 0 ]; then
    echo "⚠ Compilation failed (${#compilation_failed[@]}):"
    for test in "${compilation_failed[@]}"; do
        echo "  - $test"
    done
    echo
fi

# Exit with non-zero if any tests failed
if [ ${#failed_tests[@]} -gt 0 ] || [ ${#timed_out_tests[@]} -gt 0 ] || [ ${#compilation_failed[@]} -gt 0 ]; then
    exit 1
fi

echo "All tests passed! ✓"
exit 0
