# Bug #1: String Concatenation with Function Parameters - Root Cause Analysis

## Summary
When `str()` is called with function parameters (not local variables), it adds 2 extra bytes per parameter because the compiler is passing pointer ADDRESSES as string data instead of dereferencing the pointers.

## Evidence from Debug Output

### Test Case
```slisp
(defn test-concat [s]
  (count (str s)))

(defn -main []
  (test-concat "hello"))
```

### Expected Behavior
- `"hello"` has 5 characters
- `(count (str s))` should return 5

### Actual Behavior
- Returns 7 (exit code 7)
- Debug output shows: `[string_bytes: 4231168]` instead of `[string_bytes: hello]`

## Root Cause

The string being passed to `_string_concat` contains the ASCII digits "4231168" (7 bytes) instead of "hello" (5 bytes).

This appears to be a pointer address (e.g., 0x04231168) being interpreted as string data. The compiler is:
1. Loading the parameter value (which is a pointer to "hello")
2. Passing that pointer VALUE as if it were the string data itself
3. The runtime then reads memory at that location, which happens to contain interpretable bytes

## Next Steps

1. Examine how `PushLocalAddress` works in the compiler when the value is a function parameter
2. Check if there's a missing dereference step for string parameters
3. Compare with how local variables (which work correctly) are handled
4. Fix the parameter handling to properly dereference string pointers

## Files Modified for Investigation
- `targets/x86_64_linux/runtime/src/lib.rs`: Added debug helper functions
- `targets/x86_64_linux/runtime/src/strings.rs`: Added debug output to string functions
- `build.rs`: Added support for passing `debug` feature flag to runtime
- `Cargo.toml`: Added `debug` feature
- `debug_bug1.slisp`: Minimal test case for reproduction
