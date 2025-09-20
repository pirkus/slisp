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

### 🚧 **In Progress - Phase 1 Remaining**
- ❌ **Variable bindings** (`let`) and lexical environments

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
- ✅ Comprehensive error handling and type checking

**Examples:**
```lisp
42                          ; → 42
(+ 2 3)                     ; → 5
(* (+ 1 2) (- 5 3))        ; → 6
(if (> 10 5) 42 0)         ; → 42
(and (> 5 3) (< 2 4))      ; → true
```

### ✅ **Compiler Mode** (`slisp --compile -o <file> <expr>`)
**Fully Supported:**
- ✅ Number literals → native executables (`42` → exits with code 42)
- ✅ Basic arithmetic (`+`, `-`, `*`, `/`) → native executables
- ✅ Simple expressions → ELF x86-64 executables

**Examples:**
```bash
slisp --compile -o hello "42"        # ./hello exits with 42
slisp --compile -o add "(+ 2 3)"     # ./add exits with 5
slisp --compile -o mult "(* 4 5)"    # ./mult exits with 20
slisp --compile -o sub "(- 10 3)"    # ./sub exits with 7
```

**Compiler Limitations (Solvable with Stack-Based Approach):**
- ❌ Multi-operand arithmetic (`(+ 1 2 3)`) - **Needs stack accumulation**
- ❌ Nested expressions (`(+ 2 (* 3 4))`) - **Needs recursive stack ops**
- ❌ Comparison operations (`=`, `<`, `>`) - **Needs stack-based comparisons**
- ❌ Logical operations (`and`, `or`, `not`) - **Needs conditional stack logic**
- ❌ Conditional expressions (`if`) - **Needs stack + conditional jumps**
- ❌ Variables and functions - **Future language features**

## Next Implementation Priorities

### 🎯 **Phase 2: Stack-Based Compiler Enhancement** 
**Key Insight: Current 2-register approach limits us to simple binary operations. Stack-based evaluation will unlock full expression compilation.**

- [ ] **Implement CPU stack-based evaluation** - Use x86-64 push/pop instructions
- [ ] **Multi-operand arithmetic** - Support `(+ 1 2 3 4)` via stack accumulation
- [ ] **Nested expressions** - Support `(+ 2 (* 3 4))` via recursive stack operations
- [ ] **Comparison operations** - Stack-based `=`, `<`, `>` compilation
- [ ] **Conditional compilation** - Stack-based `if` expression support

### **Phase 2.5: Language Features**
- [ ] **Variable bindings** (`let`) with environments (interpreter + compiler)
- [ ] **Function definitions** (`defun`) and calls (interpreter + compiler)

### **Phase 3: Advanced Compiler Features**
- [ ] **Register allocation** - Optimize beyond simple 2-register approach
- [ ] **Function call conventions** - System V ABI for x86-64
- [ ] **Memory management** - Stack frames and local variables

### Phase 3: Advanced Code Generation  
- [ ] **Register allocation** - Simple linear scan for locals
- [ ] **Function call conventions** - System V ABI for x86-64
- [ ] **Stack frame management** - Proper function prologue/epilogue

### Phase 4: Advanced Features
- [ ] **Closures** and lexical scoping
- [ ] **Garbage collection** 
- [ ] **Standard library** functions
- [ ] **Optimization passes** (constant folding, dead code elimination)

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

## Technical Considerations
- Use existing JIT infrastructure with `memmap2`
- Incremental development - start with expression evaluation
- Test each phase thoroughly before moving to next
- Functional programming principles for clarity and maintainability
- Consider debugging/profiling hooks early