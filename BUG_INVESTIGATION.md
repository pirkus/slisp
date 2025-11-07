# Bug Investigation Summary

## Current Test Status
- **Passing**: 43/47 tests (91.5%)
- **Failing**: 4 tests
  - churn_reuse (exit code 1) - logic error
  - map_churn (exit code 1) - logic error
  - mixed_sizes (exit code 1) - logic error
  - mixed_literal_nesting (exit code 139) - segfault

## Bug #1: String Concatenation with Function Parameters

### Symptoms
When `str()` is called with function parameters (not local variables), it adds 2 extra bytes per parameter to the result.

### Evidence
```slisp
;; Test 1: Function parameter
(defn test-concat [s] (count (str s)))
(test-concat "hello")
;; Expected: 5, Actual: 7 (+2 bytes)

;; Test 2: Two parameters
(defn test-concat [s1 s2] (count (str s1 s2)))
(test-concat "hello" "world")
;; Expected: 10, Actual: 14 (+4 bytes, 2 per param)

;; Test 3: Local variable (works correctly)
(defn test-local []
  (let [s "hello"] (count (str s))))
;; Expected: 5, Actual: 5 (correct!)
```

### Investigation
- The bug occurs even when `_string_normalize` is completely removed
- String literals in rodata are clean (no metadata): "hello\0"
- Parameters are correctly stored at [rbp-8*(slot+1)]
- Temp slots for str() are allocated contiguously
- The runtime `_string_count` function appears correct

### Root Cause **[IDENTIFIED]**
When passing a string parameter to `str()`, the compiler passes a POINTER VALUE as the string data instead of dereferencing the pointer.

Debug output shows: `[string_bytes: 4231168]` instead of `[string_bytes: hello]`

The digits "4231168" (7 bytes) match a pointer address like 0x04231168 converted to ASCII. This explains the +2 bytes:
- "hello" = 5 bytes
- "0x4231168" as hex = 7 bytes displayed as "4231168"

### Affected Tests
- churn_reuse
- map_churn
- mixed_sizes

## Bug #2: Segfault in mixed_literal_nesting

### Symptoms
Segfault when calling `contains?` on a set retrieved from a map in specific scenarios.

### Valgrind Output
```
Invalid read of size 8
  at map_find_index (map.rs:235)
  by _map_contains (map.rs:775)
Address 0x4900000 is not stack'd, malloc'd or (recently) free'd
```

### Investigation
The segfault happens at line 235 in map.rs:
```rust
let stored_value = *key_data.add(idx);  // Reading from invalid address 0x4900000
```

Attempted to create minimal reproduction but couldn't isolate the exact trigger. These all work:
- Getting a set from a map: ✓
- Getting two sets from a map: ✓
- Assoc with disj result: ✓
- Three keys in map: ✓

The bug seems to require a very specific combination of operations from the original test.

### Original Failing Test
```slisp
(let [numbers #{1 2 3}
      letters ["a" "b" "c"]
      combos {:nums numbers :letters letters :tags #{:hot :cold}}
      trimmed (assoc combos :nums (disj numbers 2))
      nums (get trimmed :nums)
      tags (get trimmed :tags)
      ...]
  (contains? tags :hot))  ;; Segfault here
```

### Hypothesis
Possible issue with:
- Memory management when multiple collections are stored in/retrieved from maps
- Type tracking or tagging for nested collections
- Liveness analysis incorrectly freeing a set that's still needed

## Next Steps

### For Bug #1 (str with parameters)
1. Add runtime debug logging to trace actual pointer values and string contents
2. Examine the generated assembly in detail with gdb to see memory contents
3. Check if the issue is with how `PushLocalAddress` calculates addresses for parameter-derived values
4. Consider if there's an issue with stack frame layout when parameters are involved

### For Bug #2 (segfault)
1. Add more detailed valgrind output showing the full call stack
2. Examine what value is actually at address 0x4900000 - it looks like a corrupted pointer
3. Check if liveness analysis is incorrectly freeing the set
4. Verify that set/map copying in assoc/disj preserves all metadata correctly
5. Add runtime assertions to validate pointers before dereferencing

## Files Modified During Investigation
- `src/compiler/mod.rs`: Attempted fix to prevent parameter cloning (reverted)
- Created numerous test files to narrow down reproduction cases

## Recommendation
These bugs appear to be deep issues in the compiler's memory management or code generation. They likely require:
- Detailed assembly-level debugging
- Runtime instrumentation
- Possibly a refactor of how heap values are tracked through compilation

The bugs have been partially characterized but not fixed. Future sessions should focus on:
1. Getting runtime debug output working (printf or custom logging)
2. Using gdb to trace actual memory contents during execution
3. Reviewing the liveness analysis code for potential premature frees
