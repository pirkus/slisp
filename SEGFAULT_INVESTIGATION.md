# Segfault Investigation - Map/Set Equality Tests

## Problem
8 tests segfault when comparing maps/sets with keyword keys:
- maps/basic_assoc.slisp
- maps/keyword_keys.slisp
- maps/map_equality.slisp
- maps/map_literal.slisp
- memory/map_churn.slisp
- memory/set_churn.slisp
- strings/equals_nested.slisp
- strings/equals_vector_default.slisp

## Root Cause: Compiler Stack Slot Reuse Bug

**The Issue**: When creating 4+ maps/sets in a single `let` binding, the compiler incorrectly reuses stack slots that were used for temporary values during collection creation to later store the collection pointers themselves.

### Evidence

1. **Valgrind Output**:
```
Invalid read of size 1 at _string_count
  by _string_equals
  by map_keys_equal (stored_tag=6, stored_value=4386816, query_tag=6, query_value=2)
```
The `query_value=2` with `query_tag=6` (TAG_KEYWORD) shows a NUMBER value (2) being treated as a keyword pointer!

2. **GDB Stack Analysis**:
- At 0x402582: `mov $0x1,-0x40(%rbp)` - stores value_tag during map1 creation
- At 0x4025ce: `mov %rax,-0x40(%rbp)` - stores map1 pointer (correct!)
- Later: Compiler reuses `-0x40(%rbp)` for map creation temps, overwriting the pointer
- At 0x402757: `push -0x40(%rbp)` - expects map pointer, gets garbage value

3. **Reproduction Pattern**:
- 1-2 maps: Works fine
- 3 maps: Works fine
- 4+ maps: Segfault when comparing

## Technical Details

### Map Creation Flow
When creating `{:a 1 :b 2}`, the compiler:
1. Allocates 4 temp slots for: keys[], key_tags[], values[], value_tags[]
2. Fills these arrays
3. Calls `_map_create(keys, key_tags, values, value_tags, count)`
4. Stores result in another slot
5. **Should** release/reuse the 4 temp slots ONLY after all maps are created

### The Bug
With 4+ maps, the compiler releases temp slots too early, causing:
- Map pointer slots to alias with temp creation slots
- Subsequent map creations overwrite previously stored map pointers
- `_map_equals` receives corrupted pointers

### Why _map_equals Crashes
When `_map_equals(left, right)` is called:
- `left` should be a map pointer
- Instead it's a NUMBER (like 1 or 2) from overwritten memory
- `_map_get(right, key, key_tag, ...)` tries to look up this number as if it were a keyword
- `map_keys_equal` calls `_string_equals(valid_keyword_ptr, NUMBER_VALUE_AS_PTR)`
- `_string_count(0x2)` segfaults on invalid address

## Solution Options

### Option 1: Fix Compiler Stack Allocation (Recommended)
Location: `src/compiler/mod.rs` - stack slot management in `CompileContext`

The issue is likely in how `release_temp_slot()` works when compiling nested expressions.

**Problem**: Temp slots for map creation (keys/key_tags/values/value_tags arrays) are being released while they're still conceptually "live" for map pointer storage.

**Fix**: Ensure temp slots used for collection creation are not reused until ALL collections in the current `let` binding are fully created and their pointers stored.

### Option 2: Workaround in Tests
Split tests to use fewer than 4 collections per `let` binding. This is a temporary workaround only.

### Option 3: Change Compiler Strategy
Instead of using stack-allocated arrays for map creation, pass individual values. More calling convention overhead but simpler allocation.

## Progress Update

### Partial Fix Applied ✓

**Changed**: `src/compiler/context.rs`
- Modified `add_variable()` to never reuse slots from `free_slots`
- Variables now always get fresh slots using `next_slot`
- Modified `allocate_temp_slot()` to track high water mark

**Results**:
- ✅ Fixed 3 tests: `map_equality`, `set_churn`, `equals_nested` (moved from segfault to timeout)
- ❌ Still segfaulting: `basic_assoc`, `keyword_keys`, `map_literal`, `map_churn`, `map_nested_strings`, `equals_vector_default`

### Remaining Issue

The partial fix prevents simple variable-temp conflicts but doesn't fully solve the problem. The remaining segfaults occur in tests using `assoc`/`dissoc`/complex operations where intermediate temp values can still conflict with each other or with variable slots.

**Root Cause**: The compiler's slot allocation doesn't properly track value lifetimes. When temp slots are released and reused within the same expression tree, values that are still needed get overwritten.

### Full Solution Needed

A complete fix requires rethinking the slot allocation strategy:

1. **Option A**: Implement proper lifetime tracking
   - Track which slots are "live" at each point in compilation
   - Only reuse slots that are provably dead
   - Most correct but complex

2. **Option B**: Separate temp and variable slot ranges
   - Variables use slots [0...N)
   - Temps use slots [N...∞)
   - Simple but wastes stack space

3. **Option C**: Defer all temp slot releases until expression completes
   - Don't release temp slots during expression compilation
   - Release all at once when result is stored
   - Balance of simplicity and efficiency

### Next Steps

1. **Implement Option C** as it's the best balance
2. Test all 8 originally segfaulting tests
3. Verify no regressions in passing tests

## Files to Investigate

- `src/compiler/mod.rs` - lines 944-1012 (compile_hash_map)
- `src/compiler/context.rs` or similar - CompileContext implementation
- `src/codegen/x86_64_linux/codegen.rs` - stack frame layout

## Test Case

Minimal reproduction:
```lisp
(defn -main []
  (let [m1 {:a 1 :b 2}
        m2 {:a 1 :b 2}
        m3 {:a 1 :b 3}
        m4 {:a 1}      ; 4th map triggers bug
        result (+ (if (= m1 m2) 100 0)
                  (if (= m1 m3) 0 10))]
    (if (= result 110) 0 1)))
```

Compile and run - will segfault.
