# Repository Guidelines

## Project Structure & Module Organization
- `src/ast` parses S-expressions; `src/evaluator` runs the interpreter; `src/compiler` builds IR; `src/codegen/x86_64_linux` lowers to machine code; `src/repl.rs`/`src/cli.rs` drive REPL/CLI; `src/jit_runner.rs` supports JIT.
- `targets/x86_64_linux/runtime` is the runtime crate linked into compiled binaries.
- Tests live in `tests/programs/**`; `tests/programs/run_all.sh` compiles each sample into `tests/programs/target/` and runs them.

## Build, Test, and Development Commands
- `cargo build` / `cargo test` — compile the workspace and run the suite.
- `cargo run` — start the interpreter REPL; add `-- --compile` for the compiler REPL.
- `cargo run -- --compile -o out tests/programs/functions/simple_add.slisp` — sample AOT compile.
- `cargo fmt` (max width 200) and `cargo clippy --all-targets --all-features` — format and lint before pushing.
- `TEST_TIMEOUT_SECS=3 tests/programs/run_all.sh` — compile and run every sample program with a custom timeout.

## Coding Style & Naming Conventions
- Rust defaults: 4-space indent, `snake_case` for functions/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for consts.
- Favor a functional style: prefer immutability, pass immutable params when possible, and lean on higher-order helpers over mutation-heavy flows.
- favor iterator use instead of imperative loops
- Keep functions small; propagate errors via `Result`/`?`.
- Match REPL/CLI flags in `cli.rs`; isolate platform specifics to `codegen`/`targets`.
- Run `cargo fmt` before committing (width 200).

## Testing Guidelines
- Add unit tests near the code; use integration fixtures under `tests/programs/<area>/example.slisp`.
- Name tests after behavior (e.g., `evaluates_nested_vectors`, `jit_emits_tail_calls`).
- Use `cargo test -- --nocapture` when you need REPL output for debugging; keep temp logs out of commits.
- When adding new compiler features, provide paired interpreter and compiler coverage to ensure parity.

## Commit & Pull Request Guidelines
- Recent history favors short, imperative subjects (e.g., “Type inference for maps”, “update README.md”); keep subjects under ~72 characters.
- Include a brief body when changing behavior: what changed, why, and how to reproduce (commands or sample paths).
- Link related issues and call out new flags/features (`--compile`, `--keep-obj`, `allocator-telemetry`) that affect users.
- For PRs, summarize scope, testing performed (`cargo test`, `cargo fmt`, script runs), and any generated artifacts to ignore (`tests/programs/target`, `target/allocator_runs/logs`).

## Security & Configuration Tips
- AOT/JIT outputs target x86-64 Linux; note portability limits when adding platform-specific changes.
- Keep generated binaries and telemetry logs out of version control; clean `tests/programs/target/` and `target/allocator_runs/logs/` as needed.
