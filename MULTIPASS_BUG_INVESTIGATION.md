# Multi-Pass Compilation Bug Investigation

## Problem
The multi-pass compilation implementation (commit f1486f1) introduces 5 NEW test failures beyond the original 4 failures that existed before.

## Test Results

### Without Multi-Pass (single pass, no hack):
**Failures: 4/46**
- churn_reuse (exit code: 1)
- map_churn (exit code: 1)
- mixed_sizes (exit code: 1)
- mixed_literal_nesting (exit code: 139 - segfault)

### With Multi-Pass Enabled:
**Failures: 9/46**
- All 4 from above, PLUS:
- branchy_let_paths (exit code: 1)
- nested_free_blocks (exit code: 1)
- unused_let_bindings (exit code: 1)
- map_nested_strings (exit code: 1)
- set_churn (exit code: 1)

## Analysis

### Symptom
The 5 new failures all involve:
- Nested let bindings
- String concatenation with local variables created from parameters
- Multiple calls to functions that manipulate strings

### Observed Behavior
Example: `(decorate true "alpha")` which should return a 26-character string returns exit code 200 instead of 26, suggesting severe string corruption or incorrect string literal indices.

## Root Cause Hypothesis

The multi-pass implementation compiles the code twice:
1. **Pass 2** (type gathering): Compiles into `temp_program`
2. **Pass 3** (final): Compiles into `program`

Each IRProgram has its own `string_literals` vector. If type inference causes DIFFERENT code to be generated in pass 3 vs pass 2, the string literal indices could become misaligned.

**Specific Issue**: When a function is compiled the second time with known parameter types, it might generate different IR instructions (e.g., using `_string_normalize` instead of `_string_from_number`), which could cause string literals to be added in a different order, leading to index mismatches.

## Next Steps

1. Add debug logging to track string literal additions across both passes
2. Compare IR generated in pass 2 vs pass 3 for a failing test
3. Verify that string literal indices are consistent between passes
4. Consider alternative approaches:
   - Option A: Share string_literals table between temp_program and program
   - Option B: Only gather type info without full IR generation in pass 2
   - Option C: Reset/clear IR state between passes properly

## Temporary Fix

For now, the multi-pass can be disabled to return to 37/46 passing tests (4 failures) by setting `compile_program_multipass(expressions, false)` in src/compiler/mod.rs:149.
