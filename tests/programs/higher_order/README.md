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

## Compiler Mode Tests (🚧 Infrastructure Only)

These demonstrate the compiler infrastructure (not yet functional):

- **`compiled_map_test.slisp`** - Compiler lowering for map
- **`compiled_filter_test.slisp`** - Compiler lowering for filter
- **`compiled_reduce_test.slisp`** - Compiler lowering for reduce

**Status:** Compilation succeeds, but execution fails due to function pointer address resolution issues. See `docs/higher_order_compilation_status.md` for details.

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
