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
  - ✅ Escaping strings that leave scope by cloning heap values in compiler IR and runtime `_string_clone`.
  - ✅ Extend to variadic `str` and safe nested concatenation in the compiler/runtime.
  - ✅ Implement `_string_get`/`_string_subs` helpers and wire compiler codegen for `get`/`subs`.
  - ✅ Introduce runtime-backed coercions so compiled `str` can accept numbers/booleans/nil (mirroring interpreter conversions).
- **6.3 Lifetime improvements (in progress):**
- ✅ Adopt "borrowed argument, owned return" semantics so callees receive pointers without cloning while callers stay responsible for frees.
- ✅ Insert liveness-aware `FreeLocal` emission to ensure the last user of each allocation performs the release and skip `Allocate` for dead temporaries (covers straight-line and branched `let` bodies with shared liveness helpers).
- ✅ Wire the shared liveness planner into other heap-owning sites (e.g., string helpers outside `let`) so redundant frees disappear across the compiler.
  - ✅ Expand `tests/programs/` to exercise branch-heavy lets, unused bindings, and nested frees so the new lifetime semantics stay regression-tested.
  - ✅ Prototype allocator telemetry (build flag + CLI toggle) to trace allocations/frees and validate reuse with new stress cases under `tests/programs/memory/`.
- **6.4 Composite data structures (planned):**
- ✅ **6.4.1 Vector runtime primitives:** Designed a heap-backed vector layout with runtime helpers for create/access/clone/free and surfaced interpreter support for `vec`, `count`, `get`, and `subs`-style slicing, mirroring string semantics with borrowed inputs and cloned returns. Telemetry workloads (`churn_reuse`, `escaping_strings`, `mixed_sizes`) ran cleanly via `tests/programs/memory/run_allocator_telemetry.sh` with zero outstanding allocations.
  - ✅ **6.4.2 Compiler integration for vectors:** Extended IR/codegen to allocate vectors, lower vector literals, and emit ownership-aware frees while preserving the borrow-on-pass/clone-on-return rule; added regression programs covering element churn and cross-function passes (see `tests/programs/vectors/*`).
  - ✅ **6.4.3 Map runtime groundwork:** Finalised hashing/equality semantics, designed the heap layout, surfaced interpreter APIs (`hash-map`, `assoc`, `dissoc`, `contains?`, `get`), and added targeted regression programs plus runtime tests to prove the new primitives.
  - ✅ **6.4.4 Compiler string equality parity:** Teach the compiled `=` path to lower string comparisons via `_string_equals` so string fixtures can graduate from interpreter-only coverage.
  - ✅ **6.4.5 Map compiler integration:** Extend IR/codegen to allocate maps, lower literals, and emit ownership-aware runtime calls while maintaining borrow-on-pass/clone-on-return rules; unblock integration fixtures (currently skipped) that exercise assoc/dissoc/get across functions.
  - ✅ **6.4.5a Map tagging parity:** Added a dedicated `TAG_MAP` in the runtime, taught assoc/dissoc/get/render and telemetry helpers to respect it, and updated the compiler/tag emitters so nested map values no longer fall back to `TAG_ANY`.
  - ✅ **6.4.6 Map literal syntax:** Support `{}` map literals in the parser, evaluator, and compiler, ensuring the new syntax lowers to the existing runtime helpers and respects ownership/liveness planning.
  - ✅ **6.4.7 Keyword literals:** Parse `:keyword` atoms as self-evaluating values, extend interpreter/compiler map key handling with a dedicated tag, and add unit/runtime coverage so compiled maps share equality semantics with the interpreter.
  - ✅ **6.4.8 Set runtime groundwork:** Added runtime set helpers (create/clone/contains/count/disj/to_string) layered on the map primitives, extended the interpreter with `set`/`disj`/`contains?` support and uniqueness-aware semantics, wired the compiler to lower set construction and operations with liveness-aware freeing, and backfilled runtime plus interpreter tests covering churn and rendering edge cases.
  - ✅ **6.4.9 Set literal syntax:** Extend the reader/compiler to recognise `#{}` forms, lowering them onto the set runtime helpers while maintaining ownership and telemetry coverage alongside new fixtures.
  - ✅ **6.4.10 Spillover ergonomics:** Introduced higher-order functions (`map`, `filter`, `reduce`, `first`, `rest`, `cons`, `conj`) and map utilities (`keys`, `vals`) in both interpreter and compiled modes. Added runtime helpers (`_vector_map`, `_vector_filter`, `_vector_reduce`, `_vector_cons`, `_vector_conj`, `_map_keys`, `_map_vals`), extended IR with `PushFunctionAddress` and comparison operators (`=`, `<`, `>`, `<=`, `>=`), implemented PC-relative addressing for JIT and 64-bit absolute relocations for AOT, and fixed register preservation (RBX→RCX) in arithmetic ops to maintain System V ABI compliance. All higher-order functions and collection operations now work in both JIT and AOT modes with proper heap ownership tracking. Created 12 comprehensive test programs demonstrating compiled mode functionality. All 182 tests pass with no regressions.
  - **6.4.11 Remaining collection utilities (planned):** Complete compiler support for remaining interpreter-only functions to achieve full REPL/JIT/AOT feature parity for collection operations.
    - **concat:** Add `_vector_concat` runtime helper to merge multiple collections into a single vector. Implement `compile_concat` with variadic argument handling and proper heap ownership tracking. Test with chained concatenations and mixed collection types.
    - **merge:** Add `_map_merge` runtime helper to combine multiple maps (later keys override earlier ones). Implement `compile_merge` with variadic support and ensure key uniqueness. Test with overlapping keys and multiple map merges.
    - **select-keys:** Add `_map_select_keys` runtime helper that takes a map and vector of keys, returns new map with only those keys. Implement `compile_select_keys` with proper argument ordering and ownership. Test with missing keys and empty selections.
    - **zipmap:** Add `_map_zipmap` runtime helper that takes two vectors (keys and values) and creates a map. Handle length mismatches (truncate to shorter). Implement `compile_zipmap` and ensure both vectors are properly freed. Test with equal and unequal length vectors.
    - Create 4 additional test programs (`compiled_concat_test.slisp`, `compiled_merge_test.slisp`, `compiled_select_keys_test.slisp`, `compiled_zipmap_test.slisp`) demonstrating compiled functionality.
    - Update documentation to reflect complete feature parity between REPL and compiled modes for all collection operations.
- **6.5 Type inference pass (planned):**
  - **6.5.1 Analysis scaffolding:** Build a reusable analysis pipeline over the AST/IR that iterates until stable `ValueKind` assignments emerge for locals, parameters, and returns.
  - **6.5.2 Constraint solving & propagation:** Encode primitive operations, runtime helpers, and composite data semantics as constraints; ensure borrowed/owned markers survive the pass so codegen can keep clone/free behaviour correct.
  - **6.5.3 Diagnostics & UX:** Surface actionable errors for mismatched arity/types, ambiguous branches, and unsupported coercions, with location info that plugs into existing formatter/output.
  - **6.5.4 Compiler integration:** Feed inferred kinds back into lowering (skipping redundant runtime conversions, tightening liveness frees) and gate code paths that still require fallbacks.
  - **6.5.5 Test harness:** Add focused unit tests for the solver plus integration fixtures in `tests/programs/` that cover polymorphic functions, nested lets, and composite containers introduced in 6.4.
- **6.6 Lightweight shared ownership (planned):**
  - ⏳ Define ownership invariants for shared heap values (ref counts, borrow semantics, decay rules) so the runtime can manage lifetimes without user intervention.
  - ⏳ Prototype runtime support (header layout plus `_inc_ref`/`_dec_ref` helpers) and validate it against the existing allocator.
  - ⏳ Teach the compiler’s liveness planner to emit reference bumps/drops alongside current `Allocate`/`FreeLocal` logic and ensure borrow-on-argument semantics still hold.
  - ⏳ Extend telemetry workloads and add focused `tests/programs/` fixtures that stress shared ownership, double-free protection, and long-lived captures.
  - ⏳ Benchmark representative programs to gauge ref counting overhead, adjust heuristics, and document guidance in `README.md` or follow-up tickets.

### Phase 7 – I/O and System Interaction 🗂️
- **7.1 Terminal I/O:** `print`/`println`, stderr helpers, and simple formatting.
- **7.2 File I/O:** `slurp`, `spit`, existence checks, and file metadata.
- **7.3 Module system:** `(require ...)` semantics, dependency compilation order, and optional namespaces/standard library packaging.

### Phase 8 – Advanced Language Features 🚀
- **8.1 Closures and anonymous functions in compiled code:** Environment capture layout, closure call conventions, and heap-stored activation records. This phase will enable anonymous `fn` expressions in compiled mode (currently interpreter-only).
  - ⏳ Align interpreter/JIT closure capture semantics by cloning captured values up front so compiled closures can share borrow/ownership rules.
  - ⏳ Extend compiler to support anonymous `fn` expressions by generating closure objects with captured environment and function pointer.
- **8.2 Control flow:** `loop`/`recur`, pattern matching, and structured error handling (`try`/`catch`).
- **8.3 Optimisations:** Constant folding, dead code elimination, tail-call optimisation, register allocation, and selective inlining.

### Phase 9 – Tooling & Developer Experience 🧰
- **9.1 Debugging:** Stack traces, breakpoint support in interpreter, and environment inspection commands.
- **9.2 Diagnostics:** Source locations, syntax highlighting, and typo suggestions.
- **9.3 Build system:** Multi-file projects, incremental compilation cache, release/optimised build profiles, and CLI ergonomics (e.g., `--keep-obj` flag for retaining AOT object files).

## Quality & Testing Safeguards
- Unit/integration coverage across parser, evaluator, compiler, runtime, and executable outputs.
- CircleCI workflow enforces warnings-as-errors and runs the full cargo test suite.
- Use sample programs in `tests/programs/` to validate new runtime or compiler capabilities; memory-specific cases live under `tests/programs/memory/` with `tests/programs/memory/run_allocator_telemetry.sh` capturing allocator traces.

## Working Agreements
- Prioritise interpreter implementations before porting features to the compiler.
- Update PLAN.md and documentation alongside feature work.
- Maintain idiomatic Rust (no `try`/`catch` around imports) and ensure new phases keep tests green.
