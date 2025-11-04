# Higher-Order Functions: Compilation Status

## Current Implementation Status

### ✅ Fully Working: Interpreter Mode

All higher-order functions work perfectly in interpreter mode:
- `map`, `filter`, `reduce` over vectors, sets, and maps
- `first`, `rest`, `cons`, `conj`, `concat`
- Map utilities: `keys`, `vals`, `merge`, `select-keys`, `zipmap`

**Testing:** 76 unit tests passing, covering all collection types and edge cases.

### ✅ Working: Compiler Support (JIT & AOT)

Compiler infrastructure is complete and functional for `map`:

**Completed:**
- Runtime helpers in `targets/x86_64_linux/runtime/src/vector.rs`:
  - `_vector_map(func_ptr, vec)` - Apply function to each element
  - `_vector_filter(pred_ptr, vec)` - Filter elements
  - `_vector_reduce(func_ptr, init, vec)` - Fold/accumulate
- IR extension: `PushFunctionAddress(String)` instruction
- Compiler lowering: `compile_map`, `compile_filter`, `compile_reduce` functions
- Liveness analysis updated to handle function addresses
- **JIT mode:** PC-relative LEA for runtime function addresses
- **AOT mode:** 64-bit absolute ELF relocations for function symbols
- **Register preservation:** Fixed arithmetic instructions to preserve callee-saved registers

**Status:**
- ✅ `map` works in both JIT and AOT modes
- ⚠️ `filter` and `reduce` have pre-existing compiler limitations (comparison operators not yet implemented)
- ✅ Function pointers correctly passed to runtime helpers
- ✅ System V ABI calling convention properly maintained

## Implementation Details: Function Pointer Address Resolution

When compiling `(map double [1 2 3])`, the compiler needs to:
1. Get the address of the `double` function
2. Pass it to `_vector_map` runtime helper

### ✅ Solved: JIT Mode (PC-relative addressing)

In JIT mode, code is loaded at a runtime base address. We use PC-relative LEA:

**Generated code** (`src/codegen/x86_64_linux/instructions.rs`):
```asm
lea rax, [rip + relative_offset]  ; Load function address
push rax                            ; Push onto stack
```

Where `relative_offset = func_offset - (current_pos + 7)`. The RIP register contains the address of the next instruction, so this calculates the actual runtime address.

### ✅ Solved: AOT Mode (ELF relocations)

For object file linking, we use 64-bit absolute relocations:

**Generated code**:
```asm
movabs rax, 0                      ; Placeholder (will be filled by linker)
push rax
```

**Relocation** (`src/codegen/x86_64_linux/mod.rs`):
```rust
obj.add_relocation(text_section, ObjectRelocation {
    offset: (reloc.offset + stub_len) as u64,
    symbol: *symbol_id,
    addend: 0,
    flags: RelocationFlags::Generic {
        kind: RelocationKind::Absolute,  // R_X86_64_64
        encoding: RelocationEncoding::Generic,
        size: 64,
    },
})
```

The linker resolves these to actual function addresses.

## Critical Fix: Register Preservation

**Problem:** Arithmetic instructions (mul, sub) were using RBX as a scratch register, but RBX is callee-saved in System V ABI. When runtime helpers called slisp functions, RBX was clobbered, causing crashes.

**Solution:** Changed arithmetic instructions to use RCX (caller-saved) instead:
```rust
// Before: Used RBX (callee-saved - WRONG)
vec![0x58, 0x5b, 0x48, 0x0f, 0xaf, 0xd8, 0x53]

// After: Use RCX (caller-saved - CORRECT)
vec![0x58, 0x59, 0x48, 0x0f, 0xaf, 0xc8, 0x51]
```

This ensures functions preserve callee-saved registers as required by System V ABI.

## Testing Results

### Interpreter Mode
**Status:** ✅ 76 unit tests passing
- All higher-order functions work across vectors, sets, and maps
- Edge cases covered (empty collections, nil handling, etc.)

### Compiled Mode (JIT & AOT)
**Test programs** in `tests/programs/higher_order/compiled_*.slisp`:

| Program | Result | Notes |
|---------|--------|-------|
| `compiled_map_test.slisp` | ✅ PASS (exit code 5) | Maps `double` over `[1 2 3 4 5]`, counts result |
| `compiled_filter_test.slisp` | ⚠️ FAIL (exit code 0) | Requires comparison operators (`>` not yet compiled) |
| `compiled_reduce_test.slisp` | ⚠️ SEGFAULT | Requires additional compiler support |

**Verified:**
- ✅ Function pointers correctly passed to runtime
- ✅ PC-relative addressing works in JIT mode
- ✅ ELF relocations work in AOT mode
- ✅ System V ABI calling convention maintained
- ✅ Register preservation (callee-saved registers)

## Usage

### Interpreter Mode (All functions work)
```bash
# Start interpreter REPL
cargo run

# Test higher-order functions
(defn double [x] (* x 2))
(map double [1 2 3])  ; => [2 4 6]
(map double #{1 2 3}) ; => [2 4 6]
```

### Compiled Mode (map works)
```bash
# Compile and run
cargo run -- --compile -o my_program my_program.slisp
./my_program
```

## Related Work

This infrastructure lays groundwork for:
- **Phase 8:** Closures in compiled code (requires capturing environments)
- **Phase 8.2:** First-class functions as values
- **Phase 8.3:** Anonymous functions (lambda)

The runtime helpers and IR instructions added here will be reused for those features.
