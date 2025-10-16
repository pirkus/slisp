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
    string_addresses: Vec<u64>,                 // addresses of string literals in rodata segment
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
                free: None,
                string_count: None,
                string_concat_2: None,
            },
            string_addresses: Vec::new(),
        }
    }

    /// Set string addresses for rodata segment
    pub fn set_string_addresses(&mut self, program: &IRProgram) {
        const RODATA_VADDR: u64 = 0x404000;
        let mut offset = 0u64;

        for string in &program.string_literals {
            self.string_addresses.push(RODATA_VADDR + offset);
            offset += string.len() as u64 + 1; // +1 for null terminator
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

    /// Generate code to call _free runtime function
    fn generate_free_code(&self, current_pos: usize) -> Vec<u8> {
        if let Some(free_addr) = self.runtime_addresses.free {
            // Account for the size of the pop instruction (1 byte)
            let offset = (free_addr as i32) - ((current_pos + 1 + 5) as i32);
            instructions::generate_free_inline(offset)
        } else {
            // Placeholder if free address not yet known
            instructions::generate_free_inline(0)
        }
    }

    /// Generate code to call a runtime function
    fn generate_runtime_call_code(
        &self,
        func_name: &str,
        arg_count: usize,
        current_pos: usize,
    ) -> Vec<u8> {
        let runtime_addr = match func_name {
            "_string_count" => self.runtime_addresses.string_count,
            "_string_concat_2" => self.runtime_addresses.string_concat_2,
            _ => None,
        };

        if let Some(addr) = runtime_addr {
            // Calculate offset based on arg_count (number of pop instructions before call)
            let pop_size = match arg_count {
                0 => 0,
                1 => 1, // pop rdi
                2 => 2, // pop rsi + pop rdi
                _ => panic!("Unsupported arg_count"),
            };
            let offset = (addr as i32) - ((current_pos + pop_size + 5) as i32);
            instructions::generate_runtime_call(offset, arg_count)
        } else {
            // Placeholder if runtime address not yet known
            instructions::generate_runtime_call(0, arg_count)
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
            IRInstruction::PushString(index) => {
                // Get the actual rodata address for this string
                let address = self.string_addresses.get(*index).copied().unwrap_or(0);
                instructions::generate_push_string(address)
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

            IRInstruction::Free => {
                let current_pos = self.code.len();
                self.generate_free_code(current_pos)
            }

            IRInstruction::RuntimeCall(func_name, arg_count) => {
                let current_pos = self.code.len();
                self.generate_runtime_call_code(func_name, *arg_count, current_pos)
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
                IRInstruction::PushString(index) => {
                    // Get the actual rodata address for this string
                    let address = self.string_addresses.get(*index).copied().unwrap_or(0);
                    self.code
                        .extend(instructions::generate_push_string(address));
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
                IRInstruction::Free => {
                    // Call _free runtime function
                    if let Some(free_addr) = self.runtime_addresses.free {
                        let current_pos = self.code.len();
                        // Account for the size of the pop instruction (1 byte)
                        let offset = (free_addr as i32) - ((current_pos + 1 + 5) as i32);
                        self.code.extend(instructions::generate_free_inline(offset));
                    } else {
                        // Placeholder if free address not yet known
                        self.code.extend(instructions::generate_free_inline(0));
                    }
                }
                IRInstruction::RuntimeCall(func_name, arg_count) => {
                    let current_pos = self.code.len();
                    self.code.extend(self.generate_runtime_call_code(
                        func_name,
                        *arg_count,
                        current_pos,
                    ));
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
        ) || matches!(inst, IRInstruction::RuntimeCall(name, _) if name == "_string_concat_2")
    });

    // Check if program uses string operations
    let needs_string_ops = program
        .instructions
        .iter()
        .any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, _) if name == "_string_count" || name == "_string_concat_2"));

    if needs_heap || needs_string_ops {
        // TWO-PASS APPROACH for runtime functions:
        // Pass 1: Generate code to calculate where runtime functions will be
        let mut codegen_pass1 = X86CodeGen::new();
        codegen_pass1.set_string_addresses(program);
        let code_pass1 = codegen_pass1.generate(program);

        // Calculate runtime function addresses
        let mut current_offset = code_pass1.len();
        eprintln!(
            "Pass 1: user code size = {}, runtime functions start at {}",
            code_pass1.len(),
            current_offset
        );
        let mut heap_init_offset = None;
        let mut allocate_offset = None;
        let mut free_offset = None;
        let mut string_count_offset = None;
        let mut string_concat_2_offset = None;

        // Add heap functions if needed
        if needs_heap {
            heap_init_offset = Some(current_offset);
            let heap_init_code = runtime::generate_heap_init();
            eprintln!(
                "Pass 1: heap_init at {}, size={}",
                current_offset,
                heap_init_code.len()
            );
            current_offset += heap_init_code.len();

            allocate_offset = Some(current_offset);
            let allocate_code = runtime::generate_allocate();
            eprintln!(
                "Pass 1: allocate at {}, size={}",
                current_offset,
                allocate_code.len()
            );
            current_offset += allocate_code.len();

            free_offset = Some(current_offset);
            let free_code = runtime::generate_free();
            eprintln!(
                "Pass 1: free at {}, size={}",
                current_offset,
                free_code.len()
            );
            current_offset += free_code.len();
        }

        // Add string functions if needed
        if needs_string_ops {
            string_count_offset = Some(current_offset);
            let string_count_code = runtime::generate_string_count();
            eprintln!(
                "Pass 1: string_count at {}, size={}",
                current_offset,
                string_count_code.len()
            );
            current_offset += string_count_code.len();

            string_concat_2_offset = Some(current_offset);
            eprintln!("Pass 1: string_concat_2 will be at {}", current_offset);

            // Calculate tentative offset for concat_2 to call allocate
            // This is needed so concat_2 generates the same size in both passes
            let tentative_allocate_offset = if let Some(alloc_addr) = allocate_offset {
                Some((alloc_addr as i32) - (current_offset as i32))
            } else {
                None
            };

            // Generate concat_2 with the offset to get its size
            let string_concat_2_code = runtime::generate_string_concat_2(tentative_allocate_offset);
            eprintln!(
                "Pass 1: concat_2 offset={:?}, size={}",
                tentative_allocate_offset,
                string_concat_2_code.len()
            );
            current_offset += string_concat_2_code.len();
        }

        // Pass 2: Generate code with correct runtime addresses
        let mut codegen_pass2 = X86CodeGen::new();
        codegen_pass2.set_string_addresses(program);
        codegen_pass2.runtime_addresses.heap_init = heap_init_offset;
        codegen_pass2.runtime_addresses.allocate = allocate_offset;
        codegen_pass2.runtime_addresses.free = free_offset;
        codegen_pass2.runtime_addresses.string_count = string_count_offset;
        codegen_pass2.runtime_addresses.string_concat_2 = string_concat_2_offset;

        let mut code = codegen_pass2.generate(program);
        eprintln!("Pass 2: user code size = {}", code.len());

        // Append runtime support functions at the end
        if needs_heap {
            code.extend(runtime::generate_heap_init());
            code.extend(runtime::generate_allocate());
            code.extend(runtime::generate_free());
        }
        if needs_string_ops {
            code.extend(runtime::generate_string_count());

            // Calculate offset from _string_concat_2 to _allocate
            // Must use same calculation as pass 1!
            let allocate_relative_offset = if let (Some(alloc_addr), Some(concat_addr)) =
                (allocate_offset, string_concat_2_offset)
            {
                // Relative offset: allocate_addr - concat_2_start
                Some((alloc_addr as i32) - (concat_addr as i32))
            } else {
                None
            };

            let concat2_code = runtime::generate_string_concat_2(allocate_relative_offset);
            eprintln!(
                "Pass 2: concat_2 offset={:?}, size={}",
                allocate_relative_offset,
                concat2_code.len()
            );
            code.extend(concat2_code);
        }

        // Note: heap globals (heap_base, heap_end, free_list_head) live in data segment (0x403000-0x403018), handled by ELF generator

        (code, heap_init_offset)
    } else {
        // No runtime functions needed, single-pass is fine
        let mut codegen = X86CodeGen::new();
        codegen.set_string_addresses(program);
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

    #[test]
    fn test_free_instruction_included() {
        // Test that Free instruction generates code
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(64));
        program.add_instruction(IRInstruction::Free); // Free the allocated block
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let (machine_code, heap_offset) = compile_to_executable(&program);

        // Verify heap_offset is set
        assert!(heap_offset.is_some());

        // Verify machine code is generated and includes all three runtime functions
        assert!(!machine_code.is_empty());

        // The code should be longer than without Free instruction
        let mut program_without_free = IRProgram::new();
        program_without_free.add_instruction(IRInstruction::InitHeap);
        program_without_free.add_instruction(IRInstruction::Allocate(64));
        program_without_free.add_instruction(IRInstruction::Push(42));
        program_without_free.add_instruction(IRInstruction::Return);

        let (code_without_free, _) = compile_to_executable(&program_without_free);

        // Code with Free should be longer (includes free instruction + _free runtime function)
        assert!(machine_code.len() > code_without_free.len());
    }
}
