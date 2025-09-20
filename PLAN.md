# Lisp to Machine Code Compiler Plan

## Current State
- ✅ **AST parser** - Complete with robust error handling for malformed s-expressions
- ✅ **JIT runner** - Working x86-64 machine code execution using memory-mapped pages  
- ✅ **Domain model** - Clean `Node` enum (List, Primitive, Symbol variants)
- ✅ **Tree-walking evaluator** - Full implementation with comprehensive operations

## Architecture Overview
```
Lisp Source → AST → [Tree Evaluator] → IR → Code Generation → Machine Code → JIT Execution
                           ↑ CURRENT IMPLEMENTATION
```

## Current Implementation Status

### ✅ **Completed - Phase 1: Basic Evaluator**
- ✅ **Arithmetic operations** (`+`, `-`, `*`, `/`) with multi-operand support
- ✅ **Comparison operations** (`=`, `<`, `>`, `<=`, `>=`)
- ✅ **Logical operations** (`and`, `or`, `not`) with short-circuit evaluation
- ✅ **Conditional expressions** (`if`) with proper truthiness handling
- ✅ **Comprehensive error handling** (arity, type, undefined symbol errors)
- ✅ **Nested expression evaluation** - Full recursive support
- ✅ **Test coverage** - 25 passing tests across parser and evaluator

### 🚧 **In Progress - Phase 1 Remaining**
- ❌ **Variable bindings** (`let`) and lexical environments
- ❌ **CLI integration** - main.rs is currently empty

## Missing Components

### 1. **Code Generation Bridge** 🔥 CRITICAL GAP
- No connection between evaluator and JIT runner
- Need IR representation for expressions
- x86-64 instruction generation for basic arithmetic

### 2. **Intermediate Representation (IR)**
- Bridge between high-level AST and low-level machine code
- Consider stack-based IR matching current evaluator design
- Enable optimizations and better code generation

### 3. **Runtime System**
- Memory management (stack allocation initially)  
- Built-in function implementations in machine code
- Error handling and stack unwinding

### 4. **Advanced Language Features**
- Function definitions and calls
- Closures and lexical scoping
- Recursion support

## Updated Implementation Roadmap

### 🎯 **Next Priority: Phase 2 - Code Generation Bridge**
- [ ] **Create simple IR** - Stack-based instructions matching evaluator
- [ ] **x86-64 instruction encoder** - Basic arithmetic operations
- [ ] **Integrate evaluator → code generator** - Compile expressions to machine code
- [ ] **CLI interface** - Parse, compile, and execute Lisp expressions

### Phase 2.5: Enhanced Evaluator
- [ ] **Variable bindings** (`let`) with environments
- [ ] **Function definitions** (`defun`) and calls
- [ ] **REPL interface** for interactive development

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
1. **Stack-based evaluation** - Push operands, apply operations
2. **Register allocation** - Simple linear scan for locals
3. **Calling convention** - System V ABI for x86-64
4. **Memory layout** - Stack frames with proper alignment

## Technical Considerations
- Use existing JIT infrastructure with `memmap2`
- Incremental development - start with expression evaluation
- Test each phase thoroughly before moving to next
- Functional programming principles for clarity and maintainability
- Consider debugging/profiling hooks early