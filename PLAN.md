# Lisp to Machine Code Compiler Plan

## Project Snapshot
- **Execution modes:** Tree-walking interpreter plus native compiler that powers a JIT-backed REPL and ELF AOT builds.
- **Language surface:** Numbers, strings, arithmetic/logic/comparison, `if`, `let`, `def`/`defn`, anonymous `fn`, closures, and string helpers (`str`, `count`, `get`, `subs`).
- **Runtime & tooling:** Heap allocator with scoped freeing, runtime support crate, unified CLI/REPL, and CircleCI coverage for parser/evaluator/compiler tests.

## Support Matrix
### Interpreter (`cargo run`)
- Fully supports core language features, lexical scoping, closures, and string operations.
- Rich diagnostics for arity/type errors and malformed syntax.

### Compiler (`slisp --compile`)
- REPL uses the native compiler pipeline to JIT machine code while preserving interpreter semantics.
- AOT pipeline emits ELF executables with automatic `-main` discovery and runtime linkage.
- Handles arithmetic/logic/comparison, conditionals, `let`, functions, heap-managed strings, and scoped frees.

## Phase Overview
The roadmap is organised as multi-phase efforts. Completed phases are retained for context; active phases highlight remaining work.

### Phase 1 – Core Evaluator & Compiler ✅
- Implemented AST parser, domain model, evaluator with lexical environments, and initial stack-oriented IR.
- Added CLI/REPL, interpreter error handling, and baseline machine-code emission for expressions.

### Phase 2 – Stack-Based Compilation ✅
- Migrated codegen to true stack evaluation enabling multi-operand and nested expressions.
- Brought compiler feature parity for arithmetic, comparisons, logic, and `if` expressions.

### Phase 3 – Advanced Language Features ✅
- Delivered `def`/`defn`, anonymous `fn`, closures, and function invocation in interpreter mode.
- Established persistent environments and comprehensive function arity/type validation.

### Phase 4 – Function Compilation Architecture ✅
- Extended parser and IR for multi-expression programs and function metadata.
- Implemented System V ABI-compliant call frames, multi-function codegen, and ELF entry stubs.
- Verified via `tests/programs/functions/*` covering nested calls and parameter passing.

### Phase 5 – Code Quality & Refactoring ✅
- Modularised crate layout (compiler, codegen, evaluator, repl, cli) with no functional regressions.
- Expanded automated test suite (≈70+) and CI gating.

### Phase 6 – Runtime Data Types & Memory Management 🔄
- **6.1 Heap allocation (done):** Free-list allocator in runtime crate, IR/runtime hooks (`InitHeap`, `Allocate`, `FreeLocal`).
- **6.2 Strings (done/remaining):**
  - ✅ Interpreter strings with escapes and helpers (`str`, `count`, `get`, `subs`).
  - ✅ Compiler string literals via rodata, runtime-backed `count`/`str` (2-arg) with scoped freeing.
  - 🔜 Extend to variadic `str`, safe nested concatenation, `get`/`subs` via runtime functions, and escaping strings that leave scope.
- **6.3 Lifetime improvements (planned):** Smarter temporary management, unused allocation elision, block coalescing, and exploration of reference counting or GC for values escaping scope.
- **6.4 Composite data structures (planned):** Heap-backed vectors, maps, and sets with associated primitives (`vec`, `conj`, `assoc`, etc.).

### Phase 7 – I/O and System Interaction 🗂️
- **7.1 Terminal I/O:** `print`/`println`, stderr helpers, and simple formatting.
- **7.2 File I/O:** `slurp`, `spit`, existence checks, and file metadata.
- **7.3 Module system:** `(require ...)` semantics, dependency compilation order, and optional namespaces/standard library packaging.

### Phase 8 – Advanced Language Features 🚀
- **8.1 Closures in compiled code:** Environment capture layout, closure call conventions, and heap-stored activation records.
- **8.2 Control flow:** `loop`/`recur`, pattern matching, and structured error handling (`try`/`catch`).
- **8.3 Optimisations:** Constant folding, dead code elimination, tail-call optimisation, register allocation, and selective inlining.

### Phase 9 – Tooling & Developer Experience 🧰
- **9.1 Debugging:** Stack traces, breakpoint support in interpreter, and environment inspection commands.
- **9.2 Diagnostics:** Source locations, syntax highlighting, and typo suggestions.
- **9.3 Build system:** Multi-file projects, incremental compilation cache, and release/optimised build profiles.

## Quality & Testing Safeguards
- Unit/integration coverage across parser, evaluator, compiler, runtime, and executable outputs.
- CircleCI workflow enforces warnings-as-errors and runs the full cargo test suite.
- Use sample programs in `tests/programs/` to validate new runtime or compiler capabilities.

## Working Agreements
- Prioritise interpreter implementations before porting features to the compiler.
- Update PLAN.md and documentation alongside feature work.
- Maintain idiomatic Rust (no `try`/`catch` around imports) and ensure new phases keep tests green.
