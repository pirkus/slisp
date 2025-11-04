# Higher-Order Functions: Compilation Status

## Current Implementation Status

### ✅ Fully Working: Interpreter Mode

All higher-order functions work perfectly in interpreter mode:
- `map`, `filter`, `reduce` over vectors, sets, and maps
- `first`, `rest`, `cons`, `conj`, `concat`
- Map utilities: `keys`, `vals`, `merge`, `select-keys`, `zipmap`

**Testing:** 76 unit tests passing, covering all collection types and edge cases.

### 🚧 Partial: Compiler Infrastructure

The compiler infrastructure is in place but **not yet functional** for execution:

**Completed:**
- Runtime helpers in `targets/x86_64_linux/runtime/src/vector.rs`:
  - `_vector_map(func_ptr, vec)` - Apply function to each element
  - `_vector_filter(pred_ptr, vec)` - Filter elements
  - `_vector_reduce(func_ptr, init, vec)` - Fold/accumulate
- IR extension: `PushFunctionAddress(String)` instruction
- Compiler lowering: `compile_map`, `compile_filter`, `compile_reduce` functions
- Liveness analysis updated to handle function addresses

**Not Yet Working:**
- ❌ JIT execution fails - function pointers are compile-time offsets, not runtime addresses
- ❌ AOT compilation incomplete - needs symbol relocations for function references

## The Problem: Function Pointer Address Resolution

When compiling `(map double [1 2 3])`, the compiler needs to:
1. Get the address of the `double` function
2. Pass it to `_vector_map` runtime helper

**What happens now:**
```
PushFunctionAddress("double")
// Pushes compile-time offset (e.g., 0x100) onto stack
```

**What's needed:**

### For JIT (in-memory execution):
The generated code is loaded at a runtime base address. Function addresses need adjustment:
```
actual_address = base_address + function_offset
```

This requires the JIT runner to:
1. Track the base address where code was loaded
2. Patch `PushFunctionAddress` instructions to add base offset
3. Or use PC-relative addressing

### For AOT (object file linking):
Function references need proper ELF relocations:
```
Symbol relocation:
  Type: R_X86_64_64 (absolute address)
  Symbol: "double"
  Offset: instruction_position + 2 (inside mov immediate)
```

The linker will then resolve these to actual addresses.

## Implementation Plan

### Option 1: Fix JIT (Quickest)
Modify `src/codegen/x86_64_linux/codegen.rs`:
```rust
IRInstruction::PushFunctionAddress(func_name) => {
    // For JIT: Use PC-relative LEA instead of immediate MOV
    // lea rax, [rip + offset_to_function]
    // push rax
    let func_offset = calculate_relative_offset(func_name);
    instructions::generate_lea_rip_relative(func_offset)
}
```

### Option 2: Fix AOT (Proper solution)
Add function symbol relocations in `generate_instruction`:
```rust
IRInstruction::PushFunctionAddress(func_name) => {
    let current_pos = self.code.len();
    let code = instructions::generate_push(0); // Placeholder

    if matches!(self.link_mode, LinkMode::ObjFile) {
        // Record relocation for linker
        self.record_function_relocation(current_pos + 2, func_name);
    }
    code
}
```

Then add relocation handling in object file generation.

### Option 3: Defer to Phase 8 (Current approach)
Higher-order functions work perfectly in interpreter mode. The compiler support is infrastructure for future closure implementation. Full compilation can wait until Phase 8 when we implement proper closure support with captured environments.

## Testing Strategy

**Current:** Unit tests cover all interpreter functionality thoroughly.

**When compilation works:**
- Test programs in `tests/programs/higher_order/compiled_*.slisp`
- Run through both JIT and AOT paths
- Verify function pointers are passed correctly
- Test polymorphic behavior with compiled code

## Workaround: Use Interpreter Mode

For now, all higher-order functions are fully usable in interpreter mode:
```bash
# Start interpreter REPL
cargo run

# Test in REPL
(defn double [x] (* x 2))
(map double [1 2 3])  ; => [2 4 6]
(map double #{1 2 3}) ; => [2 4 6]
```

## Related Work

This infrastructure lays groundwork for:
- **Phase 8:** Closures in compiled code (requires capturing environments)
- **Phase 8.2:** First-class functions as values
- **Phase 8.3:** Anonymous functions (lambda)

The runtime helpers and IR instructions added here will be reused for those features.
