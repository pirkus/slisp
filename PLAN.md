# Lisp to Machine Code Compiler Plan

## Current State
- ✅ **AST parser** - Complete with robust error handling for malformed s-expressions
- ✅ **JIT runner** - Working x86-64 machine code execution using memory-mapped pages
- ✅ **Domain model** - Clean `Node` enum (List, Primitive, Symbol variants)
- ✅ **Tree-walking evaluator** - Full implementation with comprehensive operations
- ✅ **REPL interface** - Interactive shell with Ctrl+D exit and error handling
- ✅ **ELF executable generation** - Compiles SLisp expressions to standalone native executables

## Architecture Overview
```
Lisp Source → AST → [Tree Evaluator] → IR → Code Generation → Machine Code → JIT Execution
                           ↓                    ↓                    ↓
                    ✅ INTERPRETER       ✅ COMPILER         ✅ ELF EXECUTABLE
                      (Full Functions)    (Expressions Only)    (Single Expression)
```

**Current Compiler Scope:** Expression compilation with multi-function file support - perfect for mathematical expressions, conditionals, data processing, and multi-function programs. Function call compilation in progress.

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
- ✅ Arithmetic operations (`+`, `-`, `*`, `/`) with multi-operand support
- ✅ Comparison operations (`=`, `<`, `>`, `<=`, `>=`)
- ✅ Logical operations (`and`, `or`, `not`) with short-circuit evaluation
- ✅ Conditional expressions (`if condition then else`)
- ✅ Nested expressions (`(+ 2 (* 3 4))`)
- ✅ Empty lists (`()`)
- ✅ **Variable bindings** (`let [var val ...] body`) with lexical scoping
- ✅ **Function definitions** (`defn name [params] body`) with persistent environment
- ✅ **Anonymous functions** (`fn [params] body`) with closures
- ✅ **Function calls** with proper arity checking and lexical scoping
- ✅ **Variable definitions** (`def name value`) with persistent environment
- ✅ Comprehensive error handling and type checking

**Examples:**
```lisp
42                          ; → 42
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

### **Phase 5: Advanced Features** (After Function Compilation)
- [ ] **Closures** and lexical scoping in compiled code
- [ ] **Garbage collection** for compiled programs
- [ ] **Standard library** functions
- [ ] **Optimization passes** (constant folding, dead code elimination)
- [ ] **Register allocation** optimization

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
- Functional programming principles for clarity and maintainability (immutability, pure functions where possible)
- Consider debugging/profiling hooks early
- Always update documentation and tests and PLAN.md with current status so that it is next session ready
- if you fail to rewrite a file, try the diff again, do not try simpler solutions or complete rewrites
- feel free to add/expand the plan as you see fit
