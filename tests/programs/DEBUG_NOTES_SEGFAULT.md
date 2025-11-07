# Segfault Investigation: mixed_literal_nesting

## Crash Location
```
Program received signal SIGSEGV, Segmentation fault.
map_find_index (map=0x7eb3cdbf8180, key_tag=1, key_value=2)
  at targets/x86_64_linux/runtime/src/map.rs:235

Called from: _map_contains
```

## Root Cause
The `contains?` function is calling `_map_contains` on a SET value, not a map.

## Reproduction
```slisp
(defn -main []
  (let [numbers #{1 2 3}
        combos {:nums numbers}
        trimmed (assoc combos :nums (disj numbers 2))
        nums (get trimmed :nums)]
    (if (contains? nums 2) 0 1)))
```

Segfaults on `(contains? nums 2)` where `nums` should be a set.

## What Works
- `disj` alone: ✓
- `get` retrieving set from map: ✓
- `assoc` storing set in map: ?

## What Fails
- `assoc` storing result of `disj` in map, then `get` + `contains?`: ✗

## Hypothesis
When `assoc` stores the result of `disj` in a map, it may be:
1. Losing the ValueKind::Set type information, OR
2. Corrupting the set pointer/data, OR  
3. The compiler is generating incorrect type metadata

Result: `contains?` uses wrong runtime function (_map_contains instead of _set_contains)

## Next Steps
- Check ValueKind tracking through assoc operation
- Examine IR for the failing pattern
- Check if disj return type is correctly tracked
- Verify map value type tagging in assoc implementation
