# Bug Fix Summary: String Parameters in str()

## Problem
When `str()` was called with function parameters containing string pointers, the compiler incorrectly treated them as numbers, calling `_string_from_number()` on pointer addresses instead of `_string_normalize()`. This caused the decimal representation of the pointer address to be used as the string content.

### Example
```slisp
(defn test-concat [s]
  (count (str s)))

(defn -main []
  (test-concat "hello"))  ; Returned 7 instead of 5
```

## Root Cause
Parameters are initialized with `ValueKind::Any` in the compiler context because the single-pass compiler doesn't have type information for parameters until after functions are called. When compiling `str()` calls with parameters, the compiler couldn't determine if they were strings or numbers, defaulting to `_string_from_number()` which treated pointer addresses as integers.

## Attempted Solution: Multi-Pass Compilation ⚠️

### Implementation
Implemented a three-pass compilation strategy (src/compiler/mod.rs:148-202):

1. **Pass 1**: Register all function signatures
2. **Pass 2**: Compile into temporary program to gather type information from call sites
   - When functions call other functions, parameter types are recorded via `record_function_parameter_type`
   - Type information is propagated back to parent context (src/compiler/functions.rs:103-110)
3. **Pass 3**: Recompile with known parameter types
   - Functions now have accurate parameter types from call sites
   - `str()` calls use correct runtime functions based on actual types

### Status: PARTIALLY FIXED - INVESTIGATION ONGOING ⚠️

The multi-pass implementation successfully fixes the original bug (debug_bug1.slisp) and the string literal preservation fix has been applied. However, **5 integration tests and 1 unit test still fail** due to extra clone instructions being generated.

**Test Results**:
- **Without multi-pass**: 42/46 passing (4 failures) ✅ **Current**
- **With multi-pass**: 37/46 passing (9 failures)

**Failures with multi-pass**:
- branchy_let_paths (integration)
- nested_free_blocks (integration)
- unused_let_bindings (integration)
- map_nested_strings (integration)
- set_churn (integration)
- test_clone_argument_for_function_call (unit test)

**Root Cause Identified**: Multi-pass generates extra `_string_clone` instructions. The issue is in src/compiler/functions.rs:71-74 where `ValueKind::Any` returns are assumed to be strings and cloned. This hack interacts badly with type inference, causing:
1. Extra clones in pass 3 when types are known
2. Wrong behavior in nested let bindings with complex string operations

**Fix Applied**: String literal preservation between passes (src/compiler/mod.rs:200) ensures indices remain consistent.

**Remaining Work**: Remove or conditionally disable the `Any` → `String` assumption in function returns, or handle clone generation more carefully with type inference.

See **MULTIPASS_BUG_INVESTIGATION.md** for detailed analysis.

## Current Status: Partial Fix with Multi-Pass ENABLED

Multi-pass compilation is **enabled** with string literal preservation fix applied. The original bug (debug_bug1.slisp) **is fixed** - programs now correctly handle string parameters in `str()` calls.

**Current Test Status with Multi-Pass**: 38/47 passing (43/47 without multi-pass)
- **New failures introduced by multi-pass** (6 tests):
  - branchy_let_paths
  - nested_free_blocks
  - unused_let_bindings
  - map_nested_strings
  - set_churn
  - test_clone_argument_for_function_call (unit test)

- **Pre-existing failures** (4 tests, unrelated to multi-pass):
  - churn_reuse (exit code: 1)
  - map_churn (exit code: 1)
  - mixed_sizes (exit code: 1)
  - mixed_literal_nesting (exit code: 139 - segfault)

## Next Steps

### Option 1: Fix Multi-Pass Bug
Debug the codegen issue causing 5 test regressions when multi-pass is enabled. Requires:
- Detailed IR comparison between pass 2 and pass 3
- Understanding why generated code differs in complex nested cases
- Likely issue with variable scope, liveness analysis, or instruction generation order

### Option 2: Alternative Type Inference
Implement a lighter-weight type inference approach that doesn't require full recompilation:
- Single-pass with deferred function body compilation
- On-demand recompilation only for functions with inferred parameter types
- Type constraint propagation without full IR regeneration

### Option 3: Runtime Type Handling
Add runtime type tagging so `str()` can handle `Any` types correctly:
- Tag parameters with their actual types at runtime
- `str()` dispatches to correct handler based on runtime tag
- More runtime overhead but simpler compiler

## Compatibility
The multi-pass approach (when working correctly) maintains Clojure compatibility by using proper type inference from call sites rather than making assumptions about parameter types in specific contexts.
