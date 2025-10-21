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
- Comprehensive runtime errors for arity, type, and undefined symbols

### Compiler Modes

Slisp includes a stack-based compiler that lowers expressions to an intermediate representation before emitting x86-64 machine code.

- JIT mode (`slisp --compile`) compiles and executes expressions immediately
- Ahead-of-time compilation produces standalone ELF binaries linked with the runtime
- Supports the same expression set as the interpreter, including arithmetic, comparisons, logical operations, `if`, and `let`
- Handles nested and multi-operand expressions
- Performs automatic memory management for heap-allocated strings within lexical scopes

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

- `scripts/run_valgrind_memory.sh` – Builds the `tests/programs/memory/escaping_strings.slisp` workload, runs it under Valgrind with leak checking, and retains the object file for further inspection.
- `tests/programs/memory/churn_reuse.slisp`, `tests/programs/memory/mixed_sizes.slisp` – Stress workloads that exercise allocator reuse; run them with `--trace-alloc` to inspect telemetry.

## Project Structure

- `src/ast` – S-expression parser
- `src/evaluator` – Tree-walking interpreter
- `src/compiler` – High-level IR generation
- `src/codegen` – x86-64 code generation and ELF output
- `src/repl.rs` – Shared REPL implementation
- `targets/x86_64_linux/runtime/` – Support library linked into compiled executables

## Roadmap

See [PLAN.md](PLAN.md) for detailed progress tracking and upcoming features.
