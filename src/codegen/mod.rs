/// Code generation module for x86-64 machine code
///
/// This module is responsible for converting IR (Intermediate Representation)
/// into executable x86-64 machine code following the System V ABI.
///
/// Submodules:
/// - `abi`: System V ABI implementation (calling convention, stack frames)
/// - `instructions`: Individual x86-64 instruction generation
/// - `sizing`: Instruction size calculation for position-independent code

mod abi;
mod instructions;
mod sizing;

use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use std::collections::HashMap;

pub struct X86CodeGen {
    code: Vec<u8>,
    instruction_positions: Vec<usize>,
    function_addresses: HashMap<String, usize>, // function name -> code offset
}

impl X86CodeGen {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            instruction_positions: Vec::new(),
            function_addresses: HashMap::new(),
        }
    }

    /// Generate x86-64 machine code from IR program
    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        // For programs with functions, use multi-function code generation
        if !program.functions.is_empty() && program.entry_point.is_some() {
            return self.generate_multi_function(program);
        }

        // Check if we need a stack frame (if there are local variables)
        let has_locals = program.instructions.iter().any(|inst| {
            matches!(
                inst,
                IRInstruction::StoreLocal(_) | IRInstruction::LoadLocal(_)
            )
        });

        // Pre-calculate instruction positions
        self.calculate_positions(program);

        // Generate code
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

        // Determine generation order: entry point first, then others
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
            self.function_addresses.insert(func_info.name.clone(), current_address);

            // Calculate size by generating to temporary buffer
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
        // Generate function prologue using ABI module
        let prologue = abi::generate_prologue(func_info);
        self.code.extend(prologue);

        // Find the instructions for this function
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

        // Generate code for function body
        for inst in &function_instructions {
            self.generate_instruction(inst, func_info);
        }
    }

    /// Generate code for a single instruction
    fn generate_instruction(&mut self, inst: &IRInstruction, func_info: &FunctionInfo) {
        let code = match inst {
            IRInstruction::Push(value) => instructions::generate_push(*value),
            IRInstruction::Add => instructions::generate_add(),
            IRInstruction::Sub => instructions::generate_sub(),
            IRInstruction::Mul => instructions::generate_mul(),
            IRInstruction::Div => instructions::generate_div(),
            IRInstruction::LoadParam(slot) => instructions::generate_load_param(*slot),
            IRInstruction::StoreLocal(slot) => instructions::generate_store_local(*slot, func_info),
            IRInstruction::LoadLocal(slot) => instructions::generate_load_local(*slot, func_info),

            IRInstruction::Call(func_name, arg_count) => {
                // Setup call arguments
                let mut code = abi::generate_call_setup(*arg_count);
                // Generate call instruction
                let call_code = instructions::generate_call(
                    func_name,
                    &self.function_addresses,
                    self.code.len() + code.len(),
                );
                code.extend(call_code);
                code
            }

            IRInstruction::Return => {
                let mut code = instructions::generate_return();
                code.extend(abi::generate_epilogue());
                code
            }

            IRInstruction::DefineFunction(_, _, _) => Vec::new(), // Metadata only

            // Placeholder for other instructions - will be implemented as needed
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
        // Add prologue if we have local variables
        if has_locals {
            self.code.push(0x55); // push rbp
            self.code.extend_from_slice(&[0x48, 0x89, 0xe5]); // mov rbp, rsp
            self.code.extend_from_slice(&[0x48, 0x83, 0xec, 0x80]); // sub rsp, 128
        }

        // Generate instructions (simplified version for old path)
        // This is kept for backward compatibility with single-expression compilation
        for inst in &program.instructions {
            match inst {
                IRInstruction::Push(value) => {
                    self.code.extend(instructions::generate_push(*value));
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
                IRInstruction::Return => {
                    self.code.extend(instructions::generate_return());
                    if has_locals {
                        self.code.extend(abi::generate_epilogue());
                    }
                }
                _ => {} // Other instructions not needed for simple path
            }
        }
    }
}

/// Public API: Compile IR program to x86-64 machine code
pub fn compile_to_executable(program: &IRProgram) -> Vec<u8> {
    let mut codegen = X86CodeGen::new();
    codegen.generate(program)
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

        let machine_code = compile_to_executable(&program);

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

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 5);
    }
}
