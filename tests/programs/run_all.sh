#!/usr/bin/env bash

set -uo pipefail

default_timeout="${TEST_TIMEOUT_SECS:-2}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
output_dir="$script_dir/target"

passed=()
failed=()
timed_out=()

mkdir -p "$output_dir"

pushd "$repo_root" > /dev/null || exit 1

while IFS= read -r -d '' file; do
    rel_path="${file#$repo_root/}"
    program_name="$(basename "${file%.slisp}")"
    binary_path="$output_dir/$program_name"
    rel_binary="tests/programs/target/$program_name"

    echo "Compiling $rel_path -> $rel_binary"
    if cargo run --quiet -- --compile -o "$rel_binary" "$rel_path"; then
        echo "Compiled $rel_path"
    else
        echo "Compilation failed for $rel_path"
        failed+=("$program_name (compile)")
        echo
        continue
    fi

    echo "Running $rel_binary"
    if timeout --preserve-status "${default_timeout}s" "$binary_path"; then
        echo "Program $program_name exited successfully"
        passed+=("$program_name")
    else
        status=$?
        if [[ $status -eq 124 ]]; then
            echo "Program $program_name timed out after ${default_timeout}s"
            timed_out+=("$program_name")
        else
            echo "Program $program_name exited with $status"
            failed+=("$program_name")
        fi
    fi
    echo
done < <(find "$script_dir" -type f -name '*.slisp' -print0 | sort -z)

popd > /dev/null || exit 1

echo "Summary:"
if ((${#passed[@]})); then
    echo "  Passed:"
    for test in "${passed[@]}"; do
        echo "    - $test"
    done
else
    echo "  Passed: none"
fi

if ((${#failed[@]})); then
    echo "  Failed:"
    for test in "${failed[@]}"; do
        echo "    - $test"
    done
else
    echo "  Failed: none"
fi

if ((${#timed_out[@]})); then
    echo "  Timed out:"
    for test in "${timed_out[@]}"; do
        echo "    - $test"
    done
else
    echo "  Timed out: none"
fi
