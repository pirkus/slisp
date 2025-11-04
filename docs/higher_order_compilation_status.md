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
- ✅ `filter` works in both JIT and AOT modes
- ✅ `reduce` works in both JIT and AOT modes
- ✅ Comparison operators (`>`, `<`, `>=`, `<=`, `=`) fully implemented
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

## Critical Fixes

### Fix 1: Register Preservation

**Problem:** Arithmetic instructions (add, mul, sub) were using RBX as a scratch register, but RBX is callee-saved in System V ABI. When runtime helpers called slisp functions, RBX was clobbered, causing crashes.

**Solution:** Changed arithmetic instructions to use RCX (caller-saved) instead:
```rust
// Before: Used RBX (callee-saved - WRONG)
vec![0x58, 0x5b, 0x48, 0x01, 0xd8, 0x50]  // add
vec![0x58, 0x5b, 0x48, 0x0f, 0xaf, 0xd8, 0x53]  // mul

// After: Use RCX (caller-saved - CORRECT)
vec![0x58, 0x59, 0x48, 0x01, 0xc8, 0x50]  // add
vec![0x58, 0x59, 0x48, 0x0f, 0xaf, 0xc8, 0x51]  // mul
```

This ensures functions preserve callee-saved registers as required by System V ABI.

### Fix 2: Comparison Operators

**Problem:** Compiler lowering generated comparison IR instructions, but codegen didn't implement them.

**Solution:** Added x86-64 instruction generators using `cmp` and `setcc` instructions:
```rust
pub fn generate_greater() -> Vec<u8> {
    vec![
        0x58,               // pop rax (second operand)
        0x59,               // pop rcx (first operand)
        0x48, 0x39, 0xc1,   // cmp rcx, rax
        0x0f, 0x9f, 0xc0,   // setg al (set AL to 1 if rcx > rax, 0 otherwise)
        0x48, 0x0f, 0xb6, 0xc0,  // movzx rax, al (zero-extend)
        0x50,               // push rax
    ]
}
```

Implemented: `Equal`, `Less`, `Greater`, `LessEqual`, `GreaterEqual`

### Fix 3: Reduce Argument Ordering

**Problem:** `compile_reduce` pushed function address and init value early, but vector creation consumed them from the stack before RuntimeCall.

**Solution:** Save all arguments in local slots, then reload just before RuntimeCall:
```rust
// Save to locals
let func_slot = context.allocate_temp_slot();
instructions.push(IRInstruction::PushFunctionAddress(func_name));
instructions.push(IRInstruction::StoreLocal(func_slot));

let init_slot = context.allocate_temp_slot();
instructions.extend(init_result.instructions);
instructions.push(IRInstruction::StoreLocal(init_slot));

// ... create vector and save to vec_slot ...

// Reload in correct order for RuntimeCall
instructions.push(IRInstruction::LoadLocal(func_slot));  // RDI
instructions.push(IRInstruction::LoadLocal(init_slot));  // RSI
instructions.push(IRInstruction::LoadLocal(vec_slot));   // RDX
instructions.push(IRInstruction::RuntimeCall("_vector_reduce", 3));
```

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
| `compiled_filter_test.slisp` | ✅ PASS (exit code 5) | Filters `[1 2 3 4 5]` where `> 0`, counts result |
| `compiled_reduce_test.slisp` | ✅ PASS (exit code 15) | Reduces `[1 2 3 4 5]` with `add` from 0 = 15 |

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
