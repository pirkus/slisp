# Higher-Order Function Test Programs

This directory contains test programs demonstrating higher-order functions in slisp.

## Interpreter Mode Tests (✅ Working)

These programs demonstrate the fully functional interpreter mode:

### Basic Operations
- **`map_test.slisp`** - Map over vector
- **`filter_test.slisp`** - Filter vector
- **`reduce_test.slisp`** - Reduce/fold vector
- **`first_rest.slisp`** - First and rest operations
- **`cons_conj.slisp`** - Cons and conj operations
- **`concat_test.slisp`** - Concatenate collections

### Polymorphic Collection Tests
- **`polymorphic_map.slisp`** - Map over vectors, sets, and maps
- **`polymorphic_filter.slisp`** - Filter preserving collection types
- **`polymorphic_reduce.slisp`** - Reduce over different collection types
- **`polymorphic_first_rest.slisp`** - First/rest on all collection types

### Running Interpreter Tests

These programs are tested through unit tests (76 passing tests in `src/evaluator/mod.rs`).

To test manually in REPL:
```bash
cargo run
# Then paste the code from any .slisp file
```

## Compiler Mode Tests (✅ Fully Working)

These programs work in both JIT and AOT compiled modes:

### Fully Implemented
- **`compiled_map_test.slisp`** - Map with function pointers ✅
- **`compiled_filter_test.slisp`** - Filter with predicates ✅
- **`compiled_reduce_test.slisp`** - Reduce with accumulator ✅
- **`compiled_equality_test.slisp`** - Equality operator (=) ✅
- **`compiled_comparisons_test.slisp`** - All comparison operators (>, <, >=, <=, =) ✅
- **`compiled_pipeline_test.slisp`** - Chained map/filter/reduce ✅
- **`compiled_empty_test.slisp`** - Edge case with empty collections ✅

### Not Yet Compiled (Interpreter Only)
These programs require compiler support for additional functions:
- **`compiled_first_rest_test.slisp`** - Needs first/rest compilation
- **`compiled_cons_test.slisp`** - Needs cons compilation
- **`compiled_conj_test.slisp`** - Needs conj compilation
- **`compiled_concat_test.slisp`** - Needs concat compilation
- **`compiled_keys_test.slisp`** - Needs keys compilation
- **`compiled_vals_test.slisp`** - Needs vals compilation
- **`compiled_merge_test.slisp`** - Needs merge compilation
- **`compiled_select_keys_test.slisp`** - Needs select-keys compilation
- **`compiled_zipmap_test.slisp`** - Needs zipmap compilation

**Status:** Map, filter, and reduce work perfectly in compiled mode with full function pointer support. Comparison operators (=, <, >, <=, >=) fully functional. See `docs/higher_order_compilation_status.md` for implementation details.

### Running Compiled Tests
```bash
# Compile and run
./target/release/slisp --compile -o test_map tests/programs/higher_order/compiled_map_test.slisp
./test_map
echo $?  # Should output: 5

# Test all working compiled programs
for prog in compiled_map_test compiled_filter_test compiled_reduce_test compiled_equality_test compiled_comparisons_test compiled_pipeline_test compiled_empty_test; do
    ./target/release/slisp --compile -o /tmp/$prog tests/programs/higher_order/$prog.slisp
    /tmp/$prog
    echo "$prog: exit code $?"
done
```

Expected outputs:
- `compiled_map_test`: 5 (count of mapped vector)
- `compiled_filter_test`: 5 (all 5 numbers are positive)
- `compiled_reduce_test`: 15 (sum 1+2+3+4+5)
- `compiled_equality_test`: 1 (one element equals 5)
- `compiled_comparisons_test`: 11 (sum of all comparison results)
- `compiled_pipeline_test`: 24 (double [1 2 3 4 5] -> filter > 5 -> sum)
- `compiled_empty_test`: 0 (operations on empty collections)

## Mixed Workload Tests

Located in `tests/programs/memory/`:
- **`mixed_higher_order.slisp`** - Stress test with multiple higher-order operations
- **`pipeline_transforms.slisp`** - Chained transformations for allocator testing

These test allocator pressure under mixed workloads with vectors, maps, and sets.

## Test Coverage

### Interpreter Unit Tests (76 passing)
- ✅ Map over vectors, sets, maps
- ✅ Filter preserving collection types
- ✅ Reduce with optional initial values
- ✅ First/rest on all collection types
- ✅ Cons, conj, concat operations
- ✅ Keys, vals, merge, select-keys, zipmap
- ✅ Edge cases (empty collections, nil handling)

### Example Output
```lisp
; Map over different types
(defn double [x] (* x 2))
(map double [1 2 3])     ; => [2 4 6]
(map double #{1 2 3})    ; => [2 4 6] (order varies)

; Filter preserving types
(defn positive [x] (> x 0))
(filter positive [1 -2 3]) ; => [1 3]
(filter positive #{1 -2 3}) ; => #{1 3}

; Reduce over collections
(defn add [a b] (+ a b))
(reduce add 0 [1 2 3 4 5])  ; => 15
(reduce add 0 #{10 20 30})  ; => 60
```

## See Also
- `docs/higher_order_functions.md` - Function documentation with examples
- `docs/higher_order_compilation_status.md` - Compiler implementation status
