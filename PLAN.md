# Lisp to Machine Code Compiler Plan

## Current State
- ✅ **AST parser** - Complete with robust error handling for malformed s-expressions
- ✅ **JIT runner** - Working x86-64 machine code execution using memory-mapped pages
- ✅ **Domain model** - Clean `Node` enum (List, Primitive, Symbol variants)
- ✅ **Tree-walking evaluator** - Full implementation with comprehensive operations
- ✅ **REPL interface** - Interactive shell with Ctrl+D exit and error handling
- ✅ **ELF executable generation** - Compiles SLisp expressions to standalone native executables
- ✅ **Modular architecture** - Refactored codebase with well-organized modules (codegen, compiler, evaluator, repl, cli)
- ✅ **Heap allocation** - Free-list malloc/free implementation for dynamic memory
- ✅ **String operations** - Working `str` concatenation and `count` in both interpreter and compiler modes

**Session 3 Summary (2025-10-15):**  
1. **Fixed `str` operation bug:** XOR instruction had wrong REX prefix (0x49 instead of 0x4d), causing garbage length calculations. String concatenation now works!
2. **Implemented automatic memory management:** Added scope-based memory freeing for heap-allocated strings in `let` bindings. When heap-allocated variables go out of scope, they are automatically freed using new `FreeLocal` IR instruction that preserves RAX register.

## Architecture Overview
```
Lisp Source → AST → [Tree Evaluator] → IR → Code Generation → Machine Code → JIT Execution
                           ↓                    ↓                    ↓
                    ✅ INTERPRETER       ✅ COMPILER         ✅ ELF EXECUTABLE
                      (Full Functions)    (Full Functions)    (Multi-function programs)
```

## Current Implementation Status

### ✅ **Completed - Phase 1: Basic Evaluator & Compiler**
- ✅ **Arithmetic operations** (`+`, `-`, `*`, `/`) with multi-operand support
- ✅ **Comparison operations** (`=`, `<`, `>`, `<=`, `>=`)
- ✅ **Logical operations** (`and`, `or`, `not`) with short-circuit evaluation
- ✅ **Conditional expressions** (`if`) with proper truthiness handling
- ✅ **Comprehensive error handling** (arity, type, undefined symbol errors)
- ✅ **Nested expression evaluation** - Full recursive support
- ✅ **Test coverage** - 25 passing tests across parser and evaluator
- ✅ **IR compilation** - Stack-based intermediate representation
- ✅ **x86-64 code generation** - Basic machine code generation for simple expressions
- ✅ **ELF executable generation** - Creates standalone native executables
- ✅ **Dual execution modes** - Both interpreter and compiler with CLI interface

### ✅ **Completed - Phase 1: Basic Evaluator & Compiler**
- ✅ **Arithmetic operations** (`+`, `-`, `*`, `/`) with multi-operand support
- ✅ **Comparison operations** (`=`, `<`, `>`, `<=`, `>=`)
- ✅ **Logical operations** (`and`, `or`, `not`) with short-circuit evaluation
- ✅ **Conditional expressions** (`if`) with proper truthiness handling
- ✅ **Comprehensive error handling** (arity, type, undefined symbol errors)
- ✅ **Nested expression evaluation** - Full recursive support
- ✅ **IR compilation** - Stack-based intermediate representation
- ✅ **x86-64 code generation** - Optimized single-pass machine code generation
- ✅ **ELF executable generation** - Creates standalone native executables
- ✅ **Dual execution modes** - Both interpreter and compiler with CLI interface
- ✅ **Conditional compilation** - Full support for if/and/or/not in compiler mode

### ✅ **Completed - Phase 2: Language Features**
- ✅ **Variable bindings** (`let`) and lexical environments
- ✅ **File compilation with main functions** (`defn -main`) - Clojure-style entry points

### ✅ **Completed - Phase 4.1: Multi-Expression Compilation**
- ✅ **Multi-function file parsing** - Parse `.slisp` files with multiple `defn` statements using depth-tracking
- ✅ **Automatic -main discovery** - Find and extract `-main` function from multi-function programs
- ✅ **Expression-level compilation** - Compile `-main` function body (function calls need IR extensions)

## Feature Support Matrix

### ✅ **Interpreter Mode** (`slisp` or `slisp --compile` REPL)
**Fully Supported:**
- ✅ Number literals (`42`, `123`)
- ✅ **String literals** (`"hello world"`) with escape sequences (`\n`, `\t`, `\"`, `\\`)
- ✅ Arithmetic operations (`+`, `-`, `*`, `/`) with multi-operand support
- ✅ Comparison operations (`=`, `<`, `>`, `<=`, `>=`) - works with numbers, strings, booleans
- ✅ Logical operations (`and`, `or`, `not`) with short-circuit evaluation
- ✅ Conditional expressions (`if condition then else`)
- ✅ Nested expressions (`(+ 2 (* 3 4))`)
- ✅ Empty lists (`()`)
- ✅ **Variable bindings** (`let [var val ...] body`) with lexical scoping
- ✅ **Function definitions** (`defn name [params] body`) with persistent environment
- ✅ **Anonymous functions** (`fn [params] body`) with closures
- ✅ **Function calls** with proper arity checking and lexical scoping
- ✅ **Variable definitions** (`def name value`) with persistent environment
- ✅ **String operations** (`str`, `count`, `get`, `subs`) - Clojure-style
- ✅ Comprehensive error handling and type checking

**Examples:**
```lisp
42                          ; → 42
"hello world"               ; → "hello world"
(+ 2 3)                     ; → 5
(* (+ 1 2) (- 5 3))        ; → 6
(if (> 10 5) 42 0)         ; → 42
(and (> 5 3) (< 2 4))      ; → true
(let [x 5] x)              ; → 5
(let [x 5 y 10] (+ x y))   ; → 15
(defn inc [x] (+ x 1))     ; → #<function/1>
(inc 5)                     ; → 6
(defn add [x y] (+ x y))    ; → #<function/2>
(add 3 4)                   ; → 7
((fn [x] (* x x)) 5)        ; → 25
(def pi 3.14159)            ; → 3.14159
(* pi 2)                    ; → 6.28318
(str "hello" " " "world")   ; → "hello world"
(count "hello")             ; → 5
(get "hello" 0)             ; → "h"
(subs "hello world" 0 5)    ; → "hello"
```

### ✅ **Compiler Mode** (`slisp --compile -o <file> [expr|file.slisp]`) - **MAJOR BREAKTHROUGH!**
**Fully Supported with Stack-Based Code Generation:**
- ✅ Number literals → native executables (`42` → exits with code 42)
- ✅ Basic arithmetic (`+`, `-`, `*`, `/`) → native executables
- ✅ **Multi-operand arithmetic** (`(+ 1 2 3 4)`) → native executables 🎉
- ✅ **Nested expressions** (`(+ 2 (* 3 4))`) → native executables 🎉
- ✅ **Comparison operations** (`=`, `<`, `>`, `<=`, `>=`) → native executables 🎉
- ✅ **Logical operations** (`and`, `or`, `not`) → native executables 🎉
- ✅ **Conditional expressions** (`if`) → native executables 🎉
- ✅ **Variable bindings** (`let [var val ...] body`) → native executables 🎉
- ✅ **Complex expressions** → ELF x86-64 executables

**Examples:**
```bash
# Simple cases
slisp --compile -o hello "42"              # ./hello exits with 42
slisp --compile -o add "(+ 2 3)"           # ./add exits with 5
slisp --compile -o multi "(+ 1 2 3)"       # ./multi exits with 6
slisp --compile -o nested "(+ 2 (* 3 4))"  # ./nested exits with 14
slisp --compile -o comp "(> 5 3)"          # ./comp exits with 1
slisp --compile -o logical "(and 1 1)"     # ./logical exits with 1
slisp --compile -o conditional "(if (> 5 3) 42 0)" # ./conditional exits with 42
slisp --compile -o let_simple "(let [x 5] x)"     # ./let_simple exits with 5
slisp --compile -o let_expr "(let [x 5] (+ x 3))" # ./let_expr exits with 8
slisp --compile -o complex "(* (+ 1 2) (- 8 3))" # ./complex exits with 15
```

**Remaining Compiler Limitations:**
- ❌ **Function definitions** (`defn`) - Requires architectural changes (see Phase 4 below)
- ❌ **Function calls** - Requires architectural changes (see Phase 4 below)
- ❌ **Variable definitions** (`def`) - Requires persistent global state management

## Next Implementation Priorities

### ✅ **Phase 2: Stack-Based Compiler Enhancement - COMPLETED!**
**BREAKTHROUGH: Stack-based evaluation has unlocked full expression compilation!**

- ✅ **Implement CPU stack-based evaluation** - Use x86-64 push/pop instructions
- ✅ **Multi-operand arithmetic** - Support `(+ 1 2 3 4)` via stack accumulation
- ✅ **Nested expressions** - Support `(+ 2 (* 3 4))` via recursive stack operations
- ✅ **Comparison operations** - Stack-based `=`, `<`, `>`, `<=`, `>=` compilation
- ✅ **Conditional compilation** - Stack-based `if` expression support with conditional jumps
- ✅ **Logical operations** - Stack-based `and`, `or`, `not` with short-circuit evaluation

### ✅ **Phase 2.5: Language Features - COMPLETED!**
- ✅ **Variable bindings** (`let`) with environments (interpreter + compiler modes)

### ✅ **Phase 3: Advanced Language Features - COMPLETED!**
- ✅ **Function definitions** (`defn`) and calls (interpreter mode) - Full implementation with persistent environment
- ✅ **Anonymous functions** (`fn`) with closures and proper scoping
- ✅ **Variable definitions** (`def`) with persistent global environment
- [ ] **Function definitions** (`defn`) and calls (compiler mode) - Future work requiring call conventions
- [ ] **Recursive functions** - Currently limited due to closure scope (future enhancement)
- [ ] **Memory model optimization** for function calls (current implementation is adequate)

### **Phase 4: Function Compilation Architecture - MAJOR ARCHITECTURAL CHANGE REQUIRED**

**Current Issue:** The compiler currently compiles single expressions to standalone executables. Function support requires compiling entire programs with multiple functions, which needs a fundamentally different architecture.

**Required Architectural Changes:**

#### ✅ **Phase 4.1: Multi-Expression Compilation - COMPLETED!**
- ✅ **Multi-expression parsing** - Parse multiple top-level expressions from `.slisp` files using depth-tracking approach
- ✅ **Program-level file compilation** - Successfully compile entire files with multiple `defn` statements
- ✅ **Entry point selection** - Support `-main` functions as program entry points with automatic discovery
- ✅ **Expression extraction** - Extract and compile `-main` function body from multi-function programs

**BREAKTHROUGH: Multi-expression parsing now works perfectly!**

**Examples:**
```bash
# test.slisp containing multiple functions:
(defn add [x y] (+ x y))
(defn multiply [x y] (* x y))
(defn -main [] (+ (add 3 4) (multiply 2 5)))

# Compilation works:
slisp --compile -o test test.slisp
# Status: Parses all 3 expressions, finds -main, ready for function call compilation
```

**Current Status:** Multi-expression parsing ✅ | Function call compilation ❌ (next priority)

#### ✅ **Phase 4.2: IR Extensions for Functions - COMPLETED!**
- ✅ **Function IR instructions** - `DefineFunction`, `Call`, `CallIndirect`, `Return` with proper semantics
- ✅ **Stack frame IR** - `PushFrame`, `PopFrame`, parameter and local variable management
- ✅ **Program structure** - Support for multiple functions in single IR program with `FunctionInfo` metadata
- ✅ **Function metadata** - Track parameter counts, start addresses, local variable counts
- ✅ **Function compilation** - `defn` compilation with parameter handling and function calls
- ✅ **Multi-program compilation** - `compile_program()` function for multi-expression files
- ✅ **Entry point detection** - Automatic `-main` function discovery and entry point setting

**Current Status:** ✅ Full function compilation working! Multi-function programs compile to native executables with proper calling conventions!

#### ✅ **Phase 4.3: x86-64 Function Call Implementation - COMPLETED!**
- ✅ **System V ABI compliance** - Proper calling conventions for x86-64 Linux
- ✅ **Stack frame management** - Function prologue/epilogue generation
- ✅ **Parameter passing** - Registers (RDI, RSI, RDX, RCX, R8, R9) implementation
- ✅ **Return value handling** - RAX register for return values
- ✅ **Function entry/exit** - Proper register preservation and stack management

#### ✅ **Phase 4.4: Code Generation for Functions - COMPLETED!**
- ✅ **Function code layout** - Generate assembly for multiple functions with correct ordering
- ✅ **Call instruction generation** - Proper `call` and `ret` x86-64 instructions with correct offsets
- ✅ **Stack pointer management** - RSP alignment and management via RBP
- ✅ **Local variable addressing** - RBP-relative addressing for locals and parameters
- ✅ **Two-pass code generation** - Calculate function addresses before generating calls
- ✅ **ELF entry point** - Proper program entry that calls -main and exits with return value

#### ✅ **Phase 4.5: Linker and Executable Generation - COMPLETED!**
- ✅ **Multi-function ELF generation** - Support multiple functions in single executable
- ✅ **Symbol resolution** - Link function calls to function definitions via two-pass approach
- ✅ **Entry point management** - Proper entry stub that calls -main and exits with return value

**Working Examples:** (see `tests/programs/functions/`)
```bash
# tests/programs/functions/simple_add.slisp - Simple function call
(defn add [x y] (+ x y))
(defn -main [] (add 3 4))
# → exits with 7 ✅

# tests/programs/functions/simple_multiply.slisp - Multiplication
(defn multiply [x y] (* x y))
(defn -main [] (multiply 6 7))
# → exits with 42 ✅

# tests/programs/functions/nested_calls.slisp - Nested function calls
(defn add [x y] (+ x y))
(defn double [x] (add x x))
(defn -main [] (double 5))
# → exits with 10 ✅

# tests/programs/functions/multi_param_compute.slisp - Multi-param with nested calls
(defn add [x y] (+ x y))
(defn multiply [x y] (* x y))
(defn compute [a b c] (add (multiply a b) c))
(defn -main [] (compute 3 4 5))
# → exits with 17 (3*4+5) ✅
```

**Key Implementation Details:**
- Proper stack frame allocation: `param_count * 8 + local_count * 8 + 128` bytes scratch space
- Prevents stack corruption during nested function calls
- System V ABI compliant parameter passing via registers (RDI, RSI, RDX, RCX, R8, R9)
- Two-pass code generation for correct function address resolution

### **Phase 5: Code Quality & Refactoring - COMPLETED!**
- ✅ **Modular architecture** - Split large files into focused modules
  - ✅ `codegen/` module (702 → 611 lines across 4 files): ABI, instructions, sizing
  - ✅ `compiler/` module (920 → 975 lines across 5 files): context, expressions, functions, bindings
  - ✅ `evaluator/` module (844 → 874 lines across 3 files): primitives, special forms
  - ✅ `main` refactor (458 → 365 lines across 3 files): main, repl, cli
- ✅ **All 70 tests passing** after refactoring
- ✅ **No functional changes** - Pure code organization improvement

### **Phase 6: Runtime Data Types & Memory Management**

**Goal:** Support rich data types beyond numbers with proper memory management.

#### **Phase 6.1: Heap Allocation (Proper Malloc/Free) - ✅ COMPLETED!**

**Goal:** Implement proper heap memory allocation with deallocation support for runtime-allocated data (strings, future data structures).

**Design Decisions:**
- **Allocator Type:** Free list-based malloc - first-fit allocation with explicit deallocation
- **Implementation:** Runtime support functions (`_heap_init`, `_allocate`, `_free`) called from generated code
- **Memory Model:** Single mmap-allocated 1MB region with free list management
- **Block Structure:**
  - Free blocks: `[size: 8 bytes][next: 8 bytes][data...]`
  - Allocated blocks: `[size | ALLOCATED_BIT: 8 bytes][data...]`
- **ELF Structure:** Multi-segment executable (code segment RX, data segment RW)
- **Data segment layout (0x403000-0x403018):**
  - `heap_base` (0x403000): Start of heap region
  - `heap_end` (0x403008): End of heap region
  - `free_list_head` (0x403010): Pointer to first free block
- ✅ **Proper memory management:** Individual objects can be freed and memory reused
- 🔄 **Future Work:** Add block coalescing for better memory efficiency (Phase 6.3)

**✅ Completed Implementation:**
- ✅ **IR instructions** - Added `Allocate(size)`, `InitHeap`, `Free` to IR
- ✅ **Runtime functions** - Implemented in `codegen/runtime.rs`:
  - `_heap_init`: Uses mmap syscall to allocate 1MB heap, initializes free list with single block
  - `_allocate`: First-fit free list allocator (searches free list, removes block, marks as allocated)
  - `_free`: Returns blocks to free list (clears allocated bit, prepends to free list head)
- ✅ **ELF multi-segment support** - Conditionally creates data segment when heap is needed:
  - Program header 1: Code segment (RX) at 0x401000
  - Program header 2: Data segment (RW) at 0x403000-0x403018 (24 bytes for heap globals, only if heap needed)
- ✅ **Instruction generation** - Full implementation in `codegen/instructions.rs`:
  - `generate_call_heap_init()` - Generates call to _heap_init runtime function
  - `generate_call_allocate()` - Generates call to _allocate runtime function
  - `generate_allocate_inline()` - Generates size parameter + call
  - `generate_free_inline()` - Generates pop + call to _free runtime function
- ✅ **Code generation wiring** - Connected in both single-expression and multi-function paths
- ✅ **Two-pass compilation** - Pass 1 calculates addresses, Pass 2 generates correct call offsets
- ✅ **Three runtime functions** - All three (_heap_init, _allocate, _free) appended to code blob
- ✅ **Entry stub enhancement** - Conditionally calls `_heap_init` before user code when heap is needed
- ✅ **RuntimeAddresses tracking** - Struct in X86CodeGen tracks all three runtime function locations
- ✅ **Function prologue/epilogue** - Fixed single-expression path to use proper stack frames
- ✅ **Integration tests** - Existing `test_heap_allocation_in_executable` test passes
- ✅ **All 99 tests passing** - No regressions, heap allocation and deallocation fully working

**Why Free List Malloc?**
- ✅ Proper memory reuse: freed blocks can be reallocated
- ✅ Standard malloc/free semantics familiar to developers
- ✅ Better for long-running programs: memory doesn't grow indefinitely
- ✅ Foundation for garbage collection (Phase 6.3)
- ✅ Foundation for heap-allocated strings (Phase 6.2)
- ✅ Simple first-fit algorithm: reasonable performance with minimal complexity
- 🔄 Future optimization: block coalescing to reduce fragmentation

#### **Phase 6.2: Heap-Allocated Strings**
**✅ Interpreter Mode (Fully Working):**
- ✅ **String literals** in parser (`"hello world"`) - Full parsing with escape sequences
- ✅ **String AST type** - `Primitive::String(String)` variant in domain
- ✅ **String value type** - `Value::String(String)` in evaluator
- ✅ **Escape sequences** - Support `\n`, `\t`, `\r`, `\"`, `\\` in string literals
- ✅ **String operations** (Clojure-style):
  - ✅ `str` - Concatenation with automatic type conversion
  - ✅ `count` - String length
  - ✅ `get` - Character at index (returns nil for out of bounds)
  - ✅ `subs` - Substring extraction
- ✅ **Enhanced equality** - `=` operator now supports strings, booleans, and nil
- ✅ **String truthiness** - Empty strings are falsy, non-empty are truthy
- ✅ **REPL display** - Proper string formatting with quotes
- ✅ **Comprehensive test coverage** - All string features tested in interpreter mode

**✅ Compiler Mode (Fully Working - String Literals):**
- ✅ **PushString IR instruction** - Added to IR with string table tracking
- ✅ **IRProgram string table** - Deduplicates and tracks string literals
- ✅ **Compiler refactoring** - IRProgram threaded through all compile functions
- ✅ **x86-64 code generation** - `generate_push_string()` generates movabs + push with rodata addresses
- ✅ **Rodata segment in ELF** - Read-only segment at 0x404000 for string literals
- ✅ **String address calculation** - `X86CodeGen::set_string_addresses()` computes correct offsets
- ✅ **Working compiled strings** - String literals compile to native executables
- ✅ **Integration test** - `test_string_literal_in_executable` verifies functionality

**Design Decisions:**
- **Storage:** Strings stored in rodata segment (not heap) - more efficient, no deallocation needed
- **Null termination:** All strings null-terminated for C interop compatibility
- **Deduplication:** String table automatically deduplicates identical literals
- **Address space:** Rodata at 0x404000, separate from code (0x401000) and data (0x403000)
**✅ Compiler Mode (String Operations - count operation COMPLETED!):**
- ✅ **RuntimeCall IR instruction** - Generic instruction for calling runtime functions (extensible for all string ops)
- ✅ **Compiler integration** - `compile_count()` function generates `RuntimeCall` IR instruction
- ✅ **Code generation** - `RuntimeCall` handler implemented in both `generate_instruction()` and `generate_code()` methods
- ✅ **Runtime function** - `_string_count` runtime function implemented in x86-64 assembly
- ✅ **Runtime address tracking** - `string_count` address wired up in `RuntimeAddresses` with two-pass compilation
- ✅ **Integration tests** - Full test coverage for compiled count operation

**Implementation Details:**
- **Runtime function**: `generate_string_count()` generates x86-64 assembly that iterates null-terminated strings
- **Calling convention**: System V ABI (RDI = string pointer, RAX = return value)
- **Two-pass compilation**: Calculates runtime function addresses before generating calls
- **Modular design**: `generate_runtime_call()` supports multiple runtime functions with variable argument counts

**✅ Compiler Mode (String Operations - str operation COMPLETED!):**
- ✅ **Infrastructure complete** - `RuntimeCall` for `_string_concat_2`, compiler integration, two-pass compilation
- ✅ **Runtime function fully implemented** - `generate_string_concat_2()` with heap allocation, string copying, null termination
- ✅ **Cross-function calls working** - `_string_concat_2` successfully calls `_allocate` with proper relative offsets
- ✅ **2-argument str working** - `(str "hello" " world")` compiles and produces "hello world" in heap memory
- ✅ **Testing verified** - Concatenated strings correctly allocated, length = 11, content verified with memory inspection

**Bug Fixed (Session 3):**
- **Issue**: XOR instruction used wrong encoding (`49 31 f6` = `xor r14, rsi` instead of `4d 31 f6` = `xor r14, r14`)
- **Impact**: r14 (string length counter) was initialized with garbage, causing allocate to request random sizes
- **Fix**: Changed byte sequence in `generate_string_concat_2()` line 137 to correct REX prefix
- **Result**: String concatenation now works perfectly in compiled executables

**What Works (Session 3):**
```lisp
;; Simple string concatenation (2 arguments only)
(str "hello" " world")  ; Returns heap-allocated "hello world"

;; String operations in let bindings with automatic cleanup
(let [s1 (str "hello" " world")]
  (count s1))  ; Returns 11, s1 automatically freed on scope exit

;; Multiple string allocations - all freed automatically
(let [s1 (str "first" " string")
      s2 (str "second" " string")]
  42)  ; Returns 42, both s1 and s2 freed when let scope ends

;; String literals (in rodata, no allocation needed)
(count "hello")  ; Returns 5, no heap allocation

;; Mixing strings and other operations
(let [greeting (str "Hello" " World")
      len (count greeting)]
  len)  ; Returns 11, greeting freed before return
```

**Known Limitations:**
- **2-argument str only** - `(str "a" "b")` works, but `(str "a" "b" "c")` not supported
- **❌ Nested str calls fail** - `(str (str "a" "b") "c")` returns NULL
  - Issue: Inner result not saved as temporary, gets freed before outer call can use it
  - Fix needed: Temporary variable management for nested heap-allocating expressions
- **No value sharing between scopes** - Can't return or pass heap values (would be freed prematurely)
  - Would need string duplication on escape or reference counting

**Future Work (Phase 6.2+):**
- ❌ **get operation** - Character at index (requires runtime function)
- ❌ **subs operation** - Substring extraction (requires runtime function)
- ❌ **N-argument str** - Support `(str "a" "b" "c" ...)` with variadic arguments
- ❌ **Nested str fix** - Temporary value management for `(str (str "a" "b") "c")`
- ❌ **String escape/duplication** - Copy strings when returned or passed between scopes
- ❌ **String mutation** - Not planned (strings are immutable in design)

#### **Phase 6.2: Data Structure Support**
- [ ] **Vectors/Lists** - Mutable/immutable sequences `[1 2 3]`
  - [ ] Heap allocation with reference counting
  - [ ] Operations: `vec`, `conj`, `get`, `count`, `first`, `rest`
- [ ] **Hash maps** - Key-value pairs `{:key "value"}`
  - [ ] Heap allocation with hash table implementation
  - [ ] Operations: `hash-map`, `assoc`, `get`, `dissoc`, `keys`, `vals`
- [ ] **Sets** - Unique value collections `#{1 2 3}`
  - [ ] Heap allocation with hash set implementation
  - [ ] Operations: `hash-set`, `conj`, `disj`, `contains?`

#### **Phase 6.3: Memory Management - ✅ SCOPE-BASED DEALLOCATION IMPLEMENTED!**

**Current Memory Model (Session 3):**
The compiler implements a **simple scope-based ownership model** for heap-allocated values:

```
Ownership Rules:
1. Each `let` binding owns its heap-allocated values
2. Values are freed when the `let` scope ends (after body evaluation, before return)
3. No sharing between scopes - values cannot escape their defining scope
4. Automatic tracking - compiler identifies heap-allocating operations (like `str`)
```

**Example Execution Flow:**
```lisp
(let [s1 (str "hello" " world")  ; 1. Allocate 12 bytes, store pointer in s1
      s2 (str "foo" "bar")]      ; 2. Allocate 7 bytes, store pointer in s2
  (count s1))                    ; 3. Use s1 (still valid)
                                 ; 4. Free s2 (FreeLocal slot 1)
                                 ; 5. Free s1 (FreeLocal slot 0)
                                 ; 6. Return count result
```

**What Gets Freed Automatically:**
- ✅ Heap-allocated strings from `str` operation
- ✅ Multiple allocations in same scope (freed in reverse order)
- ✅ Allocations even if unused (e.g., `(let [s (str "a" "b")] 42)`)

**What Doesn't Work Yet:**
- ❌ Returning heap values from functions (would be freed before return)
- ❌ Passing heap values as arguments (caller's scope frees them)
- ❌ Nested allocations like `(str (str "a" "b") "c")` (inner value freed too early)
- ❌ Conditional allocations (would need smarter lifetime tracking)

**Implementation Details (Session 3):**
- Added `heap_allocated_vars: HashMap<String, bool>` to `CompileContext`
- Helper function `is_heap_allocating_expression()` identifies `str` calls
- `FreeLocal(slot)` instruction: `push rax; mov rdi,[rbp-slot*8]; call _free; pop rax`
- Preserves RAX because `_free` clobbers it with internal operations
- Free instructions inserted after body evaluation, before function epilogue

**Why RAX Preservation Matters:**
```
Without preservation:
  push 7            ; Return value on stack
  mov rdi, [rbp-8]  ; Load s1 pointer
  call _free        ; _free clobbers RAX internally!
  pop rax           ; Pop return value into RAX (correct value: 7)
  ret               ; Return RAX (now contains garbage from _free!)

With preservation:
  push 7            ; Return value on stack
  push rax          ; Save RAX (stack protection)
  mov rdi, [rbp-8]  ; Load s1 pointer
  call _free        ; _free clobbers RAX (we don't care)
  pop rax           ; Restore RAX (still has correct value)
  pop rax           ; Pop return value (overwrites with 7)
  ret               ; Return 7 ✓
```

**Future Enhancements:**
- [ ] **Block coalescing** - Merge adjacent free blocks to reduce fragmentation
- [ ] **Reference counting** - Track value lifetimes across scopes for sharing
- [ ] **String duplication on escape** - Copy strings when returned/passed to enable value sharing
- [ ] **Smart lifetime analysis** - Only free values that won't be used again
  - [ ] Auto-free when count reaches zero
- [ ] **Alternative: Mark & Sweep GC** - More robust but complex
  - [ ] Root set identification (stack, globals)
  - [ ] Marking phase - trace reachable objects
  - [ ] Sweep phase - free unreachable objects
  - [ ] GC trigger on allocation threshold

**Recommended Approach:** Start with manual malloc/free (current), add block coalescing for efficiency, then consider reference counting or mark-and-sweep if automatic memory management is needed.

### **Phase 7: I/O and System Interaction**

#### **Phase 7.1: Terminal Output**
- [ ] **Print functions** - `print`, `println` for output
  - [ ] Compiler mode: System calls (write syscall)
  - [ ] Interpreter mode: Rust print! macro
- [ ] **Format strings** - Basic string formatting
- [ ] **Error output** - `prn-err` for stderr

#### **Phase 7.2: File I/O**
- [ ] **File reading** - `slurp` to read entire file as string
- [ ] **File writing** - `spit` to write string to file
- [ ] **File operations** - `file-exists?`, `delete-file`, `file-size`

#### **Phase 7.3: Module System**
- [ ] **File importing** - `(require "path/to/file.slisp")`
  - [ ] Load and parse external files
  - [ ] Compile dependencies before main program
  - [ ] Namespace isolation (optional)
- [ ] **Standard library** - Core functions in separate files
  - [ ] Math functions (`abs`, `max`, `min`, `mod`)
  - [ ] List functions (`map`, `filter`, `reduce`)
  - [ ] String functions (`split`, `join`, `trim`)

### **Phase 8: Advanced Language Features**

#### **Phase 8.1: Closures in Compiled Code**
- [ ] **Closure representation** - Heap-allocated environment + function pointer
- [ ] **Free variable capture** - Identify and capture variables from outer scopes
- [ ] **Closure calling convention** - Pass environment pointer as hidden parameter

#### **Phase 8.2: Advanced Control Flow**
- [ ] **Loops** - `loop`/`recur` for tail recursion
- [ ] **Pattern matching** - `case` or `match` expressions
- [ ] **Exception handling** - `try`/`catch` for error handling

#### **Phase 8.3: Optimization**
- [ ] **Constant folding** - Evaluate constant expressions at compile time
- [ ] **Dead code elimination** - Remove unused code paths
- [ ] **Tail call optimization** - Convert tail recursion to loops
- [ ] **Register allocation** - Better use of x86-64 registers
- [ ] **Inline small functions** - Eliminate call overhead

### **Phase 9: Tooling & Developer Experience**

#### **Phase 9.1: Debugging Support**
- [ ] **Stack traces** - Show call stack on errors
- [ ] **Breakpoints** - Interactive debugging (interpreter mode)
- [ ] **Variable inspection** - Print environment state

#### **Phase 9.2: Error Messages**
- [ ] **Line numbers** - Track source locations in AST
- [ ] **Syntax highlighting** - Color error messages
- [ ] **Suggestions** - "Did you mean...?" for typos

#### **Phase 9.3: Build System**
- [ ] **Multi-file projects** - Project structure and dependencies
- [ ] **Compilation cache** - Incremental compilation
- [ ] **Release builds** - Optimization flags

## Code Generation Strategy

### **Current (Limited) Approach:**
- **2-register evaluation** - RAX (accumulator) + RBX (operand)
- **Binary operations only** - Can't handle multi-operand or nested expressions

### **Proposed Stack-Based Approach:**
1. **CPU stack evaluation** - Use native x86-64 push/pop instructions
2. **Operand accumulation** - Push all operands, then apply operations
3. **Recursive evaluation** - Natural support for nested expressions
4. **Unlimited operands** - Stack can handle any number of arguments

### **Implementation Benefits:**
- **Simpler code generation** - Direct mapping from IR to stack ops
- **Full expression support** - No artificial limitations
- **Better performance** - Native stack operations are fast
- **Natural recursion** - Stack handles nesting automatically

## Instructions for agents
- Test each phase thoroughly before moving to next
- Functional programming principles for clarity and maintainability (immutability, pure functions where possible, higher-order functions prefered to for loops)
- Consider debugging/profiling hooks early
- Always update documentation and tests and PLAN.md with current status so that it is next session ready
- if you fail to rewrite a file, try the diff again, do not try simpler solutions or complete rewrites
- feel free to add/expand the plan as you see fit
- When implementing new features, start with interpreter mode first (easier), then add compiler support
- sample slisp programs for testing are in tests/programs/
