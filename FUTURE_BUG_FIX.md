# Future Bug Fixes

## Critical: Collection Values in Maps Cause Segfault

**Status:** FIXED
**Severity:** Critical (causes segmentation faults)
**Affects:** All programs that store collections (vectors, maps, sets, strings) as map values

### Root Cause

Double-free/use-after-free bug when storing heap-allocated collections as map values.

When creating a map like `{:nums #{1 2 3}}`:
1. The set `#{1 2 3}` is created (owned, heap-allocated)
2. It's stored in a temporary slot and tracked for liveness analysis
3. The pointer is passed to `_map_create` runtime function
4. **Bug:** Liveness analysis frees the temp slot immediately after map creation
5. The map now contains a dangling pointer to freed memory
6. Later when we `(get map :nums)` and use the result, we're accessing freed memory → **segfault**

### Reproduction

```lisp
(defn -main []
  (let [numbers #{1 2 3}
        combos {:nums numbers}
        nums (get combos :nums)]
    (count nums)))  ; Segfaults here
```

**Exit code:** 139 (segfault)

### Technical Details

The compiler in `compile_hash_map` (mod.rs:1034-1041):
- Compiles each value expression (which may create owned heap objects)
- Stores the value pointer in a temp slot
- Passes the temp slot addresses to `_map_create`
- Releases all temp slots after map creation

The runtime `_map_create` (map.rs:739-740):
- Copies the pointers directly without cloning: `*value_data_dst.add(idx) = *values.add(idx);`
- Does NOT clone the heap objects

Result: The map contains raw pointers to memory that gets freed immediately after map creation.

### Affected Operations

- Maps with vector values: `{:v [1 2 3]}`
- Maps with map values: `{:m {:a 1}}`
- Maps with set values: `{:s #{1 2}}`
- Maps with owned string values: `{:s (str "a" "b")}`

### Solution Options

**Option 1: Clone values during map creation (safest)**
- Modify `_map_create` to clone heap-allocated values based on their tags
- Add cloning logic for TAG_VECTOR, TAG_MAP, TAG_SET, TAG_STRING

**Option 2: Don't track map value slots for liveness**
- Modify `compile_hash_map` to not release value slots or not track them
- Let the map take ownership of the heap objects
- Problem: Requires careful tracking of which slots are "transferred" to the map

**Option 3: Reference counting**
- Implement proper reference counting for all heap objects
- Maps would increment ref counts when storing values
- More complex but handles all ownership cases correctly

### Fix Applied

**Option 1** was implemented successfully. Modified `_map_create`, `map_assoc_impl`, and `_vector_create` to check tags and clone heap objects:

**Changes made:**

1. **map.rs:**
   - Added forward declarations for `_vector_clone` and `_set_clone`
   - Modified `_map_create` to clone heap-allocated keys and values (TAG_STRING, TAG_VECTOR, TAG_MAP, TAG_SET, TAG_KEYWORD)
   - Modified `map_assoc_impl` to clone heap-allocated keys and values before storing

2. **vector.rs:**
   - Added forward declarations for `_map_clone` and `_set_clone`
   - Modified `_vector_create` to clone heap-allocated elements after copying

**Results:**
- All 163 unit tests pass ✓
- 40/47 integration tests pass (up from segfaults) ✓
- 7 tests fail with exit code 1 (logic errors, not segfaults)
- No segmentation faults in any test ✓

### Original Recommended Fix

**Option 1** was recommended as the immediate fix. Modify `_map_create` to check value tags and clone heap objects:

```rust
// In _map_create, after copying pointers:
let mut idx = 0usize;
while idx < len {
    let value_tag = (*value_tags_src.add(idx) & 0xff) as u8;
    let value_ptr = *values.add(idx);

    // Clone heap-allocated values
    let owned_value = match value_tag {
        TAG_STRING => _string_clone(value_ptr as *const u8) as i64,
        TAG_VECTOR => _vector_clone(value_ptr as *const u8) as i64,
        TAG_MAP => _map_clone(value_ptr as *const u8) as i64,
        TAG_SET => _set_clone(value_ptr as *const u8) as i64,
        _ => value_ptr,  // Numbers, booleans, keywords, nil - copy as-is
    };

    *value_data_dst.add(idx) = owned_value;
    idx += 1;
}
```

**Same issue affects:**
- `_map_assoc` - when associating collection values
- `_vector_create` - when creating vectors with collection elements
- `_set_create` - when creating sets with... wait, sets can't contain collections (only hashable values)

### Test Coverage

Failing tests that demonstrate this bug:
- `tests/programs/sets/mixed_literal_nesting.slisp` - segfaults on line 8-10
- Any test that stores collections as map values

### Related Files

- `src/compiler/mod.rs` - `compile_hash_map`, `compile_assoc`
- `targets/x86_64_linux/runtime/src/map.rs` - `_map_create`, `_map_assoc`
- `targets/x86_64_linux/runtime/src/vector.rs` - `_vector_create`

---

## Non-Critical: Test Exit Codes

**Status:** Not a compiler bug
**Severity:** Low (test infrastructure issue)

### Issue

Some tests were modified to return computed results instead of 0/1 success indicators:

**Before:**
```lisp
(if (= result 74) 0 1)
```

**After:**
```lisp
(+ (+ a b) (+ (+ c d) e))  ; Returns sum directly
```

Since program exit codes are the lower 8 bits of the return value, large sums (> 255) produce non-zero exit codes, causing test failures.

### Affected Tests

- `tests/programs/memory/churn_reuse.slisp` - returns sum, expects 0
- `tests/programs/memory/map_churn.slisp` - returns sum, expects 0
- `tests/programs/memory/mixed_sizes.slisp` - returns sum, expects 0
- `tests/programs/memory/map_nested_strings.slisp` - returns sum, expects 0

### Solutions

**Option 1:** Revert test modifications (restore `if (= result EXPECTED) 0 1` pattern)

**Option 2:** Update test runner to not check exit codes, only compilation success

**Option 3:** Keep tests as-is if the intent was to just verify compilation works

### Recommendation

Clarify the intent of these tests. If they're meant to verify correctness, restore the success checks. If they're just compilation smoke tests, update the test runner.
