#!/usr/bin/env bash

# Run every memory workload with allocator telemetry enabled. Each workload is
# compiled with --trace-alloc (which implies the allocator-telemetry feature on
# the runtime) and executed under a short timeout so misbehaving binaries do not
# hang the harness. Execution logs and exit codes are written to
# target/allocator_runs for quick inspection.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PROGRAM_DIR="${REPO_ROOT}/tests/programs/memory"
OUTPUT_DIR="${REPO_ROOT}/target/allocator_runs"
LOG_DIR="${OUTPUT_DIR}/logs"

mkdir -p "${OUTPUT_DIR}" "${LOG_DIR}"

mapfile -t PROGRAMS < <(find "${PROGRAM_DIR}" -maxdepth 1 -type f -name '*.slisp' -print | sort)

if [[ ${#PROGRAMS[@]} -eq 0 ]]; then
    echo "No memory programs found in ${PROGRAM_DIR}" >&2
    exit 1
fi

declare -a RESULTS=()

for program in "${PROGRAMS[@]}"; do
    base_name="$(basename "${program}" .slisp)"
    output_bin="${OUTPUT_DIR}/${base_name}"
    log_path="${LOG_DIR}/${base_name}.log"

    echo "==> Compiling ${base_name}"
    cargo run \
        --quiet \
        --manifest-path "${REPO_ROOT}/Cargo.toml" \
        --features allocator-telemetry \
        -- \
        --compile \
        --trace-alloc \
        -o "${output_bin}" \
        "${program}"

    echo "==> Running ${base_name} (telemetry enabled)"
    set +e
    timeout -k 2 10s "${output_bin}" >"${log_path}" 2>&1
    status=$?
    set -e

    echo "    exit status: ${status}"
    RESULTS+=("${base_name}:${status}")
done

echo
echo "Run summary:"
for entry in "${RESULTS[@]}"; do
    IFS=':' read -r name status <<<"${entry}"
    printf "  %-24s %s\n" "${name}" "${status}"
done
