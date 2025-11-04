# Slisp

Build: [![CircleCI](https://dl.circleci.com/status-badge/img/gh/pirkus/slisp/tree/main.svg?style=svg)](https://dl.circleci.com/status-badge/redirect/gh/pirkus/slisp/tree/main)

## Overview

Slisp is a small Lisp implementation written in Rust with both an interactive tree-walking interpreter and a native code compiler. Expressions can be evaluated interactively, JIT-compiled on the fly, or compiled ahead-of-time into standalone ELF executables.

## Getting Started

```bash
cargo build        # Build the project
cargo test         # Run the unit and integration test suite
cargo run          # Start the default interpreter REPL
```

### Command-Line Options

- `slisp` – Launch the interpreter REPL.
- `slisp --compile` – Launch the compiler REPL which JITs expressions to machine code before running them.
- `slisp --compile [--keep-obj] -o <output> <file.slisp>` – Compile a `.slisp`/`.lisp` file that defines `(-main ...)` into a native executable; pass `--keep-obj` to retain the intermediate object file for inspection.
- `slisp --compile --trace-alloc` – Emit allocator telemetry logs in the compiler REPL (build with `--features allocator-telemetry`).
- `slisp --compile --trace-alloc [--keep-obj] -o <output> <file.slisp>` – Compile to an executable that prints allocator telemetry to stdout on exit.

## Supported Functionality

### Interpreter REPL

The interpreter provides a complete Lisp experience with rich error reporting and lexical scoping.

- Number and string literals (including escape sequences)
- Arithmetic operations: `+`, `-`, `*`, `/` with any number of operands
- Comparison operations: `=`, `<`, `>`, `<=`, `>=`
- Logical operations with short-circuiting: `and`, `or`, `not`
- Conditionals: `if`
- Lists and nested expressions
- Lexically-scoped bindings via `let`
- Top-level definitions: `def` and `defn`
- Anonymous functions via `fn` with closures
- Function invocation with arity checking
- String helpers: `str`, `count`, `get`, `subs`
- Keyword literals like `:name` that self-evaluate and act as map keys
- Vector literals `[...]` and helpers (`vec`, `get`, `subs`)
- Hash map helpers (`hash-map`, `assoc`, `dissoc`, `contains?`, `get`) and `{}` literal syntax
- Set helpers (`set`, `disj`, `contains?`) with deterministic rendering and duplicate elimination
- Comprehensive runtime errors for arity, type, and undefined symbols

### Compiler Modes

Slisp includes a stack-based compiler that lowers expressions to an intermediate representation before emitting x86-64 machine code.

- JIT mode (`slisp --compile`) compiles and executes expressions immediately
- Ahead-of-time compilation produces standalone ELF binaries linked with the runtime
- Supports the same expression set as the interpreter, including arithmetic, comparisons, logical operations, `if`, and `let`
- Emits keyword literals (`:name`) with dedicated tagging so compiled maps and equality checks mirror interpreter semantics
- Handles nested and multi-operand expressions
- Performs automatic memory management for heap-allocated strings within lexical scopes
- Generates ownership-aware code for vectors and maps, including `[...]` and `{...}` literal syntax

## Sample Session

```text
$ cargo run
SLisp Interpreter REPL v0.1.0
slisp> (let [x 5 y 7] (+ x y))
12

$ cargo run -- --compile
SLisp Compiler REPL v0.1.0
slisp-compile> (defn add [a b] (+ a b))
#<function/2>
slisp-compile> (add 3 4)
7
```

To compile a file with a `-main` function into a native executable:

```text
$ cargo run -- --compile --keep-obj -o hello tests/programs/functions/simple_add.slisp
Successfully compiled file 'tests/programs/functions/simple_add.slisp' to 'hello'
```

## Developer Utilities

- `tests/programs/memory/run_allocator_telemetry.sh` – Compiles every memory workload with allocator telemetry enabled, runs each binary under a short timeout, and stores telemetry logs in `target/allocator_runs/logs/`.
- `tests/programs/memory/churn_reuse.slisp`, `tests/programs/memory/mixed_sizes.slisp` – Stress workloads that exercise allocator reuse; inspect the logs produced by the telemetry harness for allocation/free patterns.

## Project Structure

- `src/ast` – S-expression parser
- `src/evaluator` – Tree-walking interpreter
- `src/compiler` – High-level IR generation
- `src/codegen` – x86-64 code generation and ELF output
- `src/repl.rs` – Shared REPL implementation
- `targets/x86_64_linux/runtime/` – Support library linked into compiled executables

## Roadmap

See [PLAN.md](PLAN.md) for detailed progress tracking and upcoming features.
