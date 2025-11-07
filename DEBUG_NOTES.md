# Segfault Investigation - Session Notes

## Summary
Fixed linking issues by implementing `strlen` in the runtime. Identified that the GET operation, not ASSOC, is the root cause of segfaults.

## Completed Fixes

### 1. Linker Error Fix ✅
**Problem:** Runtime needed `strlen` and `memcpy`, but linker used `-nostdlib` preventing libc linking.

**Solution:**
- Implemented `strlen` in `targets/x86_64_linux/runtime/src/memory.rs`
- `memcpy` was already implemented
- Exported both from lib.rs
- Reverted linker changes (kept `-nostdlib`)

**Result:** All programs compile and link successfully.

### 2. Initial Stack Slot Allocation Improvements ✅
**Changes made:**
1. Variables no longer reuse temp slots from `free_slots` (line 71-75 of context.rs)
2. Disabled immediate temp slot release (line 264 of context.rs - now a no-op)

**Result:** These changes didn't fix the segfaults (issue is elsewhere).

## Root Cause Identification

### Key Discovery: GET operation causes segfaults, not ASSOC

**Test Results:**
```lisp
;; ✅ WORKS
(assoc (hash-map) "key" 42)
(let [m1 (assoc (hash-map) "user" 41)
      m2 (assoc m1 "user" 42)]
  0)

;; ❌ SEGFAULTS
(let [m (assoc (hash-map) "user" 42)
      v (get m "user" 0)]
  v)
```

**GDB Evidence:**
```
Program received signal SIGSEGV, Segmentation fault.
map_find_index (map=0x7ea38b08c000, key_tag=3, key_value=42)
```

- `key_tag=3` is STRING (correct)
- `key_value=42` should be a string pointer (WRONG!)
- The number 42 from the map value has corrupted/replaced the string pointer for the key

## Next Steps for Investigation

### Hypotheses to Test:

1. **Evaluation Stack Corruption**
   - `compile_get` for maps pushes multiple values (lines 675-685)
   - The key value on the stack might get overwritten by subsequent Push operations

2. **Stack Frame Size Issues**
   - With temp slot release disabled, `next_slot` keeps growing
   - May exceed allocated stack frame size causing memory corruption

3. **Local Slot Address Calculation**
   - `StoreLocal` and `LoadLocal` might calculate wrong offsets
   - Particularly when many temp slots are allocated

### Suggested Debugging Approach:

1. Add debug output to `compile_get` to log:
   - Number of temp slots allocated
   - Stack layout before `_map_get` call
   - Whether key is stored in a slot vs. left on stack

2. Check x86-64 codegen for:
   - How stack frame size is calculated
   - If there's a maximum local slot count
   - How `StoreLocal`/`LoadLocal` calculate offsets

3. Test with explicit slot tracking:
   - Print which slots are used for what
   - Verify no overlap between evaluation stack and local slots

## Current Test Status
- **3/47 passing** (6.4%)
- **8 segfaults** (17%)
- **4 timeouts** (8.5%)
- **32 logic errors** (68%)

## Files Modified This Session
- `targets/x86_64_linux/runtime/src/memory.rs` - Added `strlen`
- `targets/x86_64_linux/runtime/src/lib.rs` - Exported `strlen`
- `src/codegen/x86_64_linux/mod.rs` - Removed libc linking attempt
- `src/compiler/context.rs` - Stack slot allocation improvements
- `tests/programs/run_with_timeout.sh` - New test runner with timeout protection

## Key Code Locations
- `compile_get`: src/compiler/mod.rs:610-749
- String literal compilation: src/compiler/expressions.rs:13-16
- Map get operation: src/compiler/mod.rs:662-698
- Stack slot allocation: src/compiler/context.rs:71-75, 200-208, 264-266
