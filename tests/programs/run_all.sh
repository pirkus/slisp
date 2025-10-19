#!/usr/bin/env bash

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
output_dir="$script_dir/target"

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
        echo
        continue
    fi

    echo "Running $rel_binary"
    "$binary_path"
    status=$?
    echo "Program $program_name exited with $status"
    echo
done < <(find "$script_dir" -type f -name '*.slisp' -print0 | sort -z)

popd > /dev/null || exit 1
