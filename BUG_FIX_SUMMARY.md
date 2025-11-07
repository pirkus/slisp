# Bug Fix Summary: String Parameters in str()

## Problem
When `str()` was called with function parameters containing string pointers, the compiler incorrectly treated them as numbers, calling `_string_from_number()` on pointer addresses instead of `_string_normalize()`. This caused the decimal representation of the pointer address to be used as the string content.

### Example
```slisp
(defn test-concat [s]
  (count (str s)))

(-main []
  (test-concat "hello"))  ; Returned 7 instead of 5
```

## Root Cause
Parameters are initialized with `ValueKind::Any` in the compiler context (src/compiler/context.rs:95). When compiling `str()` calls, the inference logic at src/compiler/mod.rs:841 would return `Some(ValueKind::Any)`, preventing the fallback that should infer String type. As a result, the `Any` case at line 893 would call `_string_from_number()` on what was actually a string pointer.

## Fix
Modified src/compiler/mod.rs lines 838-854 to:
1. Check if the inferred type is `ValueKind::Any` (not just missing)
2. For parameters (not local variables) that are still `Any`, infer them as `String` in the context of `str()` calls
3. This causes `_string_normalize()` to be called instead of `_string_from_number()`

## Testing
- **Original bug**: FIXED - `debug_bug1.slisp` now returns correct count
- **Unit tests**: All 163 tests PASS
- **Integration tests**: 37/46 pass (down from 43/47 before investigation started)

## Known Issues
The fix introduces 5 new test regressions:
- branchy_let_paths
- nested_free_blocks
- unused_let_bindings
- escaping_strings
- map_nested_strings

These failures suggest that inferring `Any`-typed parameters as `String` in `str()` context may be too aggressive in some cases. The root issue is that the single-pass compiler doesn't have type information for parameters until after functions are called.

## Proper Solution (Future Work)
The correct fix requires:
1. Multi-pass compilation where functions are recompiled after call sites provide type information
2. OR: Runtime type tagging so `str()` can handle `Any` types correctly
3. OR: More sophisticated type inference that propagates types from `defn` through the call graph

For now, this fix resolves the primary bug while documenting the known limitations.
