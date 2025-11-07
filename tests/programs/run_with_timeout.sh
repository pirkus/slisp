#!/usr/bin/env bash

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
output_dir="$script_dir/target"

mkdir -p "$output_dir"

pushd "$repo_root" > /dev/null || exit 1

# Counters
total=0
passed=0
segfault=0
timeout=0
failed=0

while IFS= read -r -d '' file; do
    total=$((total + 1))
    rel_path="${file#$repo_root/}"
    program_name="$(basename "${file%.slisp}")"
    binary_path="$output_dir/$program_name"
    rel_binary="tests/programs/target/$program_name"

    echo "=== [$total] $program_name ==="

    # Compile
    if ! cargo run --quiet -- --compile -o "$rel_binary" "$rel_path" 2>&1; then
        echo "❌ Compilation failed for $rel_path"
        failed=$((failed + 1))
        echo
        continue
    fi

    # Run with 5 second timeout
    if timeout 5s "$binary_path" > /dev/null 2>&1; then
        status=$?
        if [ $status -eq 0 ]; then
            echo "✅ PASS (exit $status)"
            passed=$((passed + 1))
        else
            echo "✅ PASS (exit $status)"
            passed=$((passed + 1))
        fi
    else
        status=$?
        if [ $status -eq 139 ] || [ $status -eq 134 ]; then
            echo "💥 SEGFAULT (exit $status)"
            segfault=$((segfault + 1))
        elif [ $status -eq 124 ]; then
            echo "⏱️  TIMEOUT (> 5s)"
            timeout=$((timeout + 1))
        else
            echo "❌ FAILED (exit $status)"
            failed=$((failed + 1))
        fi
    fi
    echo
done < <(find "$script_dir" -type f -name '*.slisp' -print0 | sort -z)

popd > /dev/null || exit 1

echo "========================================"
echo "SUMMARY: $total tests"
echo "✅ Passed:    $passed"
echo "💥 Segfaults: $segfault"
echo "⏱️  Timeouts:  $timeout"
echo "❌ Failed:    $failed"
echo "========================================"
