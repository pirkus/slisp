/// x86-64 Linux code generation backend
///
/// This module implements code generation for x86-64 Linux systems:
/// - Machine code generation (System V ABI calling convention)
/// - ELF executable generation (Linux binary format)
/// - Linux syscall conventions
///
/// Submodules:
/// - `abi`: System V ABI implementation (calling convention, stack frames)
/// - `instructions`: Individual x86-64 instruction generation
/// - `sizing`: Instruction size calculation for position-independent code
/// - `runtime`: Runtime support functions (heap allocation, etc.)
/// - `executable`: ELF executable generation for Linux
mod abi;
pub mod executable;
mod instructions;
pub mod runtime;
mod sizing;

use crate::codegen::backend::{CodeGenBackend, RuntimeAddresses};
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use std::collections::HashMap;

pub struct X86CodeGen {
    code: Vec<u8>,
    instruction_positions: Vec<usize>,
    function_addresses: HashMap<String, usize>, // function name -> code offset
    runtime_addresses: RuntimeAddresses,        // addresses of runtime support functions
}

impl X86CodeGen {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            instruction_positions: Vec::new(),
            function_addresses: HashMap::new(),
            runtime_addresses: RuntimeAddresses {
                heap_init: None,
                allocate: None,
            },
        }
    }

    /// Generate code to call _heap_init runtime function
    fn generate_heap_init_code(&self, current_pos: usize) -> Vec<u8> {
        if let Some(heap_init_addr) = self.runtime_addresses.heap_init {
            let offset = (heap_init_addr as i32) - ((current_pos + 5) as i32);
            instructions::generate_call_heap_init(offset)
        } else {
            // Placeholder if heap_init address not yet known
            instructions::generate_call_heap_init(0)
        }
    }

    /// Generate code to call _allocate runtime function
    fn generate_allocate_code(&self, size: usize, current_pos: usize) -> Vec<u8> {
        if let Some(allocate_addr) = self.runtime_addresses.allocate {
            // Account for the size of the mov instruction (7 bytes)
            let offset = (allocate_addr as i32) - ((current_pos + 7 + 5) as i32);
            instructions::generate_allocate_inline(size, offset)
        } else {
            // Placeholder if allocate address not yet known
            instructions::generate_allocate_inline(size, 0)
        }
    }

    /// Generate x86-64 machine code from IR program
    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        if !program.functions.is_empty() && program.entry_point.is_some() {
            return self.generate_multi_function(program);
        }

        let has_locals = program.instructions.iter().any(|inst| {
            matches!(
                inst,
                IRInstruction::StoreLocal(_) | IRInstruction::LoadLocal(_)
            )
        });

        self.calculate_positions(program);

        self.code.clear();
        self.generate_code(program, has_locals);

        self.code.clone()
    }

    /// Generate code for multi-function programs
    fn generate_multi_function(&mut self, program: &IRProgram) -> Vec<u8> {
        self.code.clear();
        self.function_addresses.clear();

        // TWO-PASS APPROACH:
        // Pass 1: Calculate addresses by generating functions in order
        // Pass 2: Generate all functions with correct addresses now available

        let mut ordered_functions = Vec::new();

        if let Some(entry_name) = &program.entry_point {
            if let Some(entry_func) = program.functions.iter().find(|f| &f.name == entry_name) {
                ordered_functions.push(entry_func.clone());
            }
        }

        for func_info in &program.functions {
            if program.entry_point.as_ref() != Some(&func_info.name) {
                ordered_functions.push(func_info.clone());
            }
        }

        // Pass 1: Calculate addresses
        let mut current_address = 0;
        for func_info in &ordered_functions {
            self.function_addresses
                .insert(func_info.name.clone(), current_address);

            let saved_code = self.code.clone();
            self.code.clear();
            self.generate_function(program, func_info);
            let func_size = self.code.len();
            self.code = saved_code;

            current_address += func_size;
        }

        // Pass 2: Generate all functions with correct addresses
        self.code.clear();
        for func_info in &ordered_functions {
            self.generate_function(program, func_info);
        }

        self.code.clone()
    }

    /// Generate code for a single function
    fn generate_function(&mut self, program: &IRProgram, func_info: &FunctionInfo) {
        let prologue = abi::generate_prologue(func_info);
        self.code.extend(prologue);

        let mut in_function = false;
        let mut function_instructions = Vec::new();

        for inst in &program.instructions {
            match inst {
                IRInstruction::DefineFunction(name, _, _) if name == &func_info.name => {
                    in_function = true;
                }
                IRInstruction::Return if in_function => {
                    function_instructions.push(inst.clone());
                    break;
                }
                _ if in_function => {
                    function_instructions.push(inst.clone());
                }
                _ => {}
            }
        }

        for inst in &function_instructions {
            self.generate_instruction(inst, func_info);
        }
    }

    /// Generate code for a single instruction
    fn generate_instruction(&mut self, inst: &IRInstruction, func_info: &FunctionInfo) {
        let code = match inst {
            IRInstruction::Push(value) => instructions::generate_push(*value),
            IRInstruction::PushString(_index) => {
                // For now, push a placeholder address (0)
                // This will be properly resolved when we implement ELF string sections
                instructions::generate_push_string(0)
            }
            IRInstruction::Add => instructions::generate_add(),
            IRInstruction::Sub => instructions::generate_sub(),
            IRInstruction::Mul => instructions::generate_mul(),
            IRInstruction::Div => instructions::generate_div(),
            IRInstruction::LoadParam(slot) => instructions::generate_load_param(*slot),
            IRInstruction::StoreLocal(slot) => instructions::generate_store_local(*slot, func_info),
            IRInstruction::LoadLocal(slot) => instructions::generate_load_local(*slot, func_info),

            IRInstruction::Call(func_name, arg_count) => {
                let mut code = abi::generate_call_setup(*arg_count);
                let call_code = instructions::generate_call(
                    func_name,
                    &self.function_addresses,
                    self.code.len() + code.len(),
                );
                code.extend(call_code);
                code
            }

            IRInstruction::InitHeap => {
                let current_pos = self.code.len();
                self.generate_heap_init_code(current_pos)
            }

            IRInstruction::Allocate(size) => {
                let current_pos = self.code.len();
                self.generate_allocate_code(*size, current_pos)
            }

            IRInstruction::Return => {
                let mut code = instructions::generate_return();
                code.extend(abi::generate_epilogue());
                code
            }

            IRInstruction::DefineFunction(_, _, _) => Vec::new(),

            _ => Vec::new(),
        };

        self.code.extend(code);
    }

    /// Calculate instruction positions (for old single-function path)
    fn calculate_positions(&mut self, program: &IRProgram) {
        self.instruction_positions.clear();
        let mut position = 0;

        let has_locals = program.instructions.iter().any(|inst| {
            matches!(
                inst,
                IRInstruction::StoreLocal(_) | IRInstruction::LoadLocal(_)
            )
        });

        if has_locals {
            position += 8; // prologue: push rbp + mov rbp,rsp + sub rsp,128
        }

        for instruction in &program.instructions {
            self.instruction_positions.push(position);
            position += sizing::instruction_size(instruction, has_locals);
        }
    }

    /// Generate code for old single-function path
    fn generate_code(&mut self, program: &IRProgram, has_locals: bool) {
        // Always add function prologue since entry stub calls user code as a function
        self.code.push(0x55); // push rbp
        self.code.extend_from_slice(&[0x48, 0x89, 0xe5]); // mov rbp, rsp

        if has_locals {
            self.code.extend_from_slice(&[0x48, 0x83, 0xec, 0x80]); // sub rsp, 128
        }

        for inst in &program.instructions {
            match inst {
                IRInstruction::Push(value) => {
                    self.code.extend(instructions::generate_push(*value));
                }
                IRInstruction::PushString(_index) => {
                    // For now, push placeholder address
                    self.code.extend(instructions::generate_push_string(0));
                }
                IRInstruction::Add => {
                    self.code.extend(instructions::generate_add());
                }
                IRInstruction::Sub => {
                    self.code.extend(instructions::generate_sub());
                }
                IRInstruction::Mul => {
                    self.code.extend(instructions::generate_mul());
                }
                IRInstruction::Div => {
                    self.code.extend(instructions::generate_div());
                }
                IRInstruction::InitHeap => {
                    // Call _heap_init runtime function
                    if let Some(heap_init_addr) = self.runtime_addresses.heap_init {
                        let current_pos = self.code.len();
                        let offset = (heap_init_addr as i32) - ((current_pos + 5) as i32);
                        self.code
                            .extend(instructions::generate_call_heap_init(offset));
                    } else {
                        // Placeholder if heap_init address not yet known
                        self.code.extend(instructions::generate_call_heap_init(0));
                    }
                }
                IRInstruction::Allocate(size) => {
                    // Call _allocate runtime function with size
                    if let Some(allocate_addr) = self.runtime_addresses.allocate {
                        let current_pos = self.code.len();
                        // Account for the size of the mov instruction (7 bytes)
                        let offset = (allocate_addr as i32) - ((current_pos + 7 + 5) as i32);
                        self.code
                            .extend(instructions::generate_allocate_inline(*size, offset));
                    } else {
                        // Placeholder if allocate address not yet known
                        self.code
                            .extend(instructions::generate_allocate_inline(*size, 0));
                    }
                }
                IRInstruction::Return => {
                    self.code.extend(instructions::generate_return());
                    // Always add epilogue since we always add prologue
                    self.code.extend(abi::generate_epilogue());
                }
                _ => {} // Other instructions not needed for simple path
            }
        }
    }
}

/// Implement CodeGenBackend trait for X86CodeGen
impl CodeGenBackend for X86CodeGen {
    fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        self.generate(program)
    }

    fn runtime_addresses(&self) -> RuntimeAddresses {
        self.runtime_addresses.clone()
    }
}

/// Public API: Compile IR program to x86-64 machine code
/// Uses two-pass approach to calculate runtime function addresses
/// Returns (machine_code, heap_init_offset)
pub fn compile_to_executable(program: &IRProgram) -> (Vec<u8>, Option<usize>) {
    // Check if program uses heap allocation
    let needs_heap = program.instructions.iter().any(|inst| {
        matches!(
            inst,
            IRInstruction::InitHeap | IRInstruction::Allocate(_) | IRInstruction::PushString(_)
        )
    });

    if needs_heap {
        // TWO-PASS APPROACH for runtime functions:
        // Pass 1: Generate code to calculate where runtime functions will be
        let mut codegen_pass1 = X86CodeGen::new();
        let code_pass1 = codegen_pass1.generate(program);

        // Calculate runtime function addresses
        let heap_init_offset = code_pass1.len();
        let heap_init_code = runtime::generate_heap_init();
        let allocate_offset = heap_init_offset + heap_init_code.len();

        // Pass 2: Generate code with correct runtime addresses
        let mut codegen_pass2 = X86CodeGen::new();
        codegen_pass2.runtime_addresses.heap_init = Some(heap_init_offset);
        codegen_pass2.runtime_addresses.allocate = Some(allocate_offset);

        let mut code = codegen_pass2.generate(program);

        // Append runtime support functions at the end
        code.extend(heap_init_code);
        code.extend(runtime::generate_allocate());

        // Note: heap_ptr lives in data segment (0x403000), handled by ELF generator

        (code, Some(heap_init_offset))
    } else {
        // No heap allocation needed, single-pass is fine
        let mut codegen = X86CodeGen::new();
        (codegen.generate(program), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit_runner::{JitRunner, JitRunnerTrt};

    #[test]
    fn test_simple_number() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let (machine_code, _heap_offset) = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_basic_arithmetic() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(2));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Return);

        let (machine_code, _heap_offset) = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_heap_allocation_basic() {
        // Test that heap allocation instructions generate correct code and offsets
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(100));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let (machine_code, heap_offset) = compile_to_executable(&program);

        // Verify heap_offset is set when heap instructions are used
        assert!(heap_offset.is_some());

        // Verify machine code is generated (non-empty)
        assert!(!machine_code.is_empty());

        // The machine code should include runtime functions at the end
        // (we can't easily test execution in JIT since it needs proper memory setup)
    }

    #[test]
    fn test_no_heap_when_not_needed() {
        // Test that programs without heap instructions don't get heap setup
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let (machine_code, heap_offset) = compile_to_executable(&program);

        // Verify heap_offset is None when heap instructions are not used
        assert!(heap_offset.is_none());
        assert!(!machine_code.is_empty());
    }
}
