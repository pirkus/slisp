#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROGRAM_PATH="${REPO_ROOT}/tests/programs/memory/escaping_strings.slisp"
OUTPUT_DIR="${REPO_ROOT}/target/valgrind"
OUTPUT_BIN="${OUTPUT_DIR}/escaping_strings"

if ! command -v valgrind >/dev/null 2>&1; then
    echo "valgrind is required but was not found in PATH" >&2
    exit 127
fi

mkdir -p "${OUTPUT_DIR}"

cargo run --quiet --manifest-path "${REPO_ROOT}/Cargo.toml" -- --compile --keep-obj -o "${OUTPUT_BIN}" "${PROGRAM_PATH}"

valgrind \
    --leak-check=full \
    --show-leak-kinds=all \
    --error-exitcode=1 \
    "${OUTPUT_BIN}"

echo "valgrind completed without reporting leaks."
