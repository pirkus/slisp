# Repository Guidelines

## Project Structure & Module Organization
- `src/` holds the Rust crates that power the interpreter, compiler, code generator, CLI, and REPL. Modules follow a feature-based layout (`ast/`, `compiler/`, `codegen/`, etc.).
- `tests/programs/` contains integration samples used to validate runtime behaviour; memory-focused workloads live in `tests/programs/memory/`.
- `targets/x86_64_linux/runtime/` is the platform-specific runtime crate linked into AOT executables.
- `PLAN.md`, `README.md`, and this guide summarise roadmap, usage, and contributor practices.

## Build, Test, and Development Commands
- `cargo build` — compile the workspace (interpreter, compiler, runtime).
- `cargo test` — run all unit and integration tests; required before any PR.
- `cargo run` — start the interpreter REPL (`slisp` binary).
- `cargo run -- --compile` — launch the compiler REPL with JIT evaluation.
- `cargo run -- --compile [--keep-obj] -o <out> <file.slisp>` — emit an ELF executable; `--keep-obj` preserves the intermediate `.o` file.
- `tests/programs/memory/run_allocator_telemetry.sh` — compile every memory workload with allocator telemetry enabled, run them under a timeout, and capture the runtime allocator report for each binary in `target/allocator_runs/logs/`.

## Coding Style & Naming Conventions
- Rust 2021 edition with `rustfmt`; use the project’s `rustfmt.toml`. Run `cargo fmt` before committing.
- Indent with 4 spaces; avoid tab characters.
- Prefer snake_case for files, modules, functions; CamelCase for types and traits; SCREAMING_SNAKE_CASE for constants.
- Keep modules small and well-commented only when necessary; rely on descriptive names over verbose comments.
- function style prefered, higher order functions, immutability where possible

## Testing Guidelines
- Primary harness: `cargo test`. Add focused tests under `src/{module}/tests` or new fixtures in `tests/programs/`.
- Name tests after the behaviour they assert (e.g., `test_clone_argument_for_function_call`).
- For memory regressions, update or extend `tests/programs/memory/` and re-run `run_allocator_telemetry.sh` to generate fresh allocator traces.

## Commit & Pull Request Guidelines
- Commit messages follow a single concise line in imperative voice (e.g., `Add runtime clone helper`), optionally amended with detailed body text.
- Each PR should:
  - Reference related issues or tasks when available.
  - Describe functional changes, testing performed, and any follow-up work.
  - Include screenshots or CLI transcripts only when behaviour is user-visible.
- Ensure `cargo fmt` and `cargo test` pass locally before requesting review.
