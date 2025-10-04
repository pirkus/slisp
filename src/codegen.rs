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

    // Optimized single-pass code generation with size pre-calculation
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

        // Pre-calculate instruction positions without generating code
        self.calculate_positions(program);

        // Single pass: generate code with known positions
        self.code.clear();
        self.generate_code(program, has_locals);

        self.code.clone()
    }

    // Generate code for multi-function programs
    fn generate_multi_function(&mut self, program: &IRProgram) -> Vec<u8> {
        self.code.clear();
        self.function_addresses.clear();

        // TWO-PASS APPROACH:
        // Pass 1: Generate all functions in separate buffers to know their sizes and addresses
        // Pass 2: Combine them with correct call addresses

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

        // Pass 1: Calculate addresses by generating functions in order
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

        // Pass 2: Generate all functions with correct addresses now available
        self.code.clear();
        for func_info in &ordered_functions {
            self.generate_function(program, func_info);
        }

        self.code.clone()
    }

    // Generate code for a single function
    fn generate_function(&mut self, program: &IRProgram, func_info: &FunctionInfo) {
        // Note: function address should already be in function_addresses from pass 1
        // We don't update it here to avoid changing addresses during generation

        // Generate function prologue
        self.generate_prologue(func_info);

        // Find the instructions for this function
        // Instructions are between DefineFunction and Return
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

    // Generate function prologue (System V ABI)
    fn generate_prologue(&mut self, func_info: &FunctionInfo) {
        self.emit(&[0x55]); // push rbp
        self.emit(&[0x48, 0x89, 0xe5]); // mov rbp, rsp

        // Allocate stack space for parameters + locals + scratch space
        // We need space for:
        // - Parameters: param_count * 8 bytes
        // - Local variables: local_count * 8 bytes
        // - Red zone / scratch space: 128 bytes (for safety during calls)
        let param_space = func_info.param_count * 8;
        let local_space = func_info.local_count * 8;
        let stack_size = param_space + local_space + 128;

        if stack_size > 0 {
            if stack_size <= 127 {
                self.emit(&[0x48, 0x83, 0xec, stack_size as u8]); // sub rsp, imm8
            } else {
                self.emit(&[0x48, 0x81, 0xec]); // sub rsp, imm32
                self.emit(&(stack_size as u32).to_le_bytes());
            }
        }

        // Save parameters to stack (System V ABI: RDI, RSI, RDX, RCX, R8, R9)
        // We need to save them because they'll be used by child function calls
        let param_regs: Vec<u8> = vec![0x7f, 0x77, 0x75, 0x4d, 0x45, 0x4d]; // RDI, RSI, RDX, RCX, R8, R9 offsets
        let param_reg_codes: Vec<&[u8]> = vec![
            &[0x48, 0x89, 0x7d], // mov [rbp+offset], rdi
            &[0x48, 0x89, 0x75], // mov [rbp+offset], rsi
            &[0x48, 0x89, 0x55], // mov [rbp+offset], rdx
            &[0x48, 0x89, 0x4d], // mov [rbp+offset], rcx
            &[0x4c, 0x89, 0x45], // mov [rbp+offset], r8
            &[0x4c, 0x89, 0x4d], // mov [rbp+offset], r9
        ];

        for i in 0..func_info.param_count.min(6) {
            // Store param at [rbp - 8*(i+1)]
            let offset = 8 * (i + 1);
            self.emit(param_reg_codes[i]);
            self.emit(&[((-(offset as i8)) as u8)]);
        }
    }

    // Generate function epilogue (System V ABI)
    fn generate_epilogue(&mut self) {
        self.emit(&[0x48, 0x89, 0xec]); // mov rsp, rbp
        self.emit(&[0x5d]); // pop rbp
        self.emit(&[0xc3]); // ret
    }

    // Generate code for a single instruction in function context
    fn generate_instruction(&mut self, inst: &IRInstruction, func_info: &FunctionInfo) {
        match inst {
            IRInstruction::Push(value) => {
                if *value <= 127 && *value >= -128 {
                    self.emit(&[0x6a]);
                    self.emit(&[*value as u8]);
                } else {
                    self.emit(&[0x68]);
                    self.emit(&(*value as u32).to_le_bytes());
                }
            }

            IRInstruction::Add => {
                self.emit(&[0x58]); // pop rax
                self.emit(&[0x5b]); // pop rbx
                self.emit(&[0x48, 0x01, 0xd8]); // add rax, rbx
                self.emit(&[0x50]); // push rax
            }

            IRInstruction::Sub => {
                self.emit(&[0x58]); // pop rax
                self.emit(&[0x5b]); // pop rbx
                self.emit(&[0x48, 0x29, 0xc3]); // sub rbx, rax
                self.emit(&[0x53]); // push rbx
            }

            IRInstruction::Mul => {
                self.emit(&[0x58]); // pop rax
                self.emit(&[0x5b]); // pop rbx
                self.emit(&[0x48, 0x0f, 0xaf, 0xd8]); // imul rbx, rax
                self.emit(&[0x53]); // push rbx
            }

            IRInstruction::Div => {
                self.emit(&[0x58]); // pop rax (divisor)
                self.emit(&[0x5b]); // pop rbx (dividend)
                self.emit(&[0x48, 0x89, 0xd8]); // mov rax, rbx
                self.emit(&[0x48, 0x89, 0xc1]); // mov rcx, rax
                self.emit(&[0x48, 0x99]); // cqo
                self.emit(&[0x48, 0xf7, 0xf9]); // idiv rcx
                self.emit(&[0x50]); // push rax
            }

            IRInstruction::LoadParam(slot) => {
                // Load parameter from stack location [rbp - 8*(slot+1)]
                let offset = 8 * (slot + 1);
                if offset <= 127 {
                    self.emit(&[0xff, 0x75]); // push [rbp+offset]
                    self.emit(&[((-(offset as i8)) as u8)]);
                } else {
                    self.emit(&[0xff, 0xb5]); // push [rbp+offset]
                    self.emit(&(-(offset as i32)).to_le_bytes());
                }
            }

            IRInstruction::StoreLocal(slot) => {
                self.emit(&[0x58]); // pop rax
                // Store at [rbp - 8*(func_info.param_count + slot + 1)]
                // Locals come after parameters
                let offset = 8 * (func_info.param_count + slot + 1);
                if offset <= 127 {
                    self.emit(&[0x48, 0x89, 0x45]);
                    self.emit(&[((-(offset as i8)) as u8)]);
                } else {
                    self.emit(&[0x48, 0x89, 0x85]);
                    self.emit(&(-(offset as i32)).to_le_bytes());
                }
            }

            IRInstruction::LoadLocal(slot) => {
                // Load from [rbp - 8*(func_info.param_count + slot + 1)]
                let offset = 8 * (func_info.param_count + slot + 1);
                if offset <= 127 {
                    self.emit(&[0xff, 0x75]);
                    self.emit(&[((-(offset as i8)) as u8)]);
                } else {
                    self.emit(&[0xff, 0xb5]);
                    self.emit(&(-(offset as i32)).to_le_bytes());
                }
            }

            IRInstruction::Call(func_name, arg_count) => {
                // Pop arguments from stack and place in registers (System V ABI)
                // RDI, RSI, RDX, RCX, R8, R9
                let arg_regs: Vec<&[u8]> = vec![
                    &[0x5f], // pop rdi
                    &[0x5e], // pop rsi
                    &[0x5a], // pop rdx
                    &[0x59], // pop rcx
                    &[0x41, 0x58], // pop r8
                    &[0x41, 0x59], // pop r9
                ];

                // Pop arguments in reverse order (last arg first)
                for i in (0..*arg_count.min(&6)).rev() {
                    self.emit(arg_regs[i]);
                }

                // Call the function
                if let Some(&func_addr) = self.function_addresses.get(func_name) {
                    let current_pos = self.code.len();
                    let call_offset = (func_addr as i32) - ((current_pos + 5) as i32);
                    self.emit(&[0xe8]); // call
                    self.emit(&call_offset.to_le_bytes());
                } else {
                    // Function not yet generated, emit placeholder
                    // This will be fixed in a second pass
                    self.emit(&[0xe8, 0x00, 0x00, 0x00, 0x00]); // call 0 (placeholder)
                }

                // Result is in RAX, push it onto stack
                self.emit(&[0x50]); // push rax
            }

            IRInstruction::Return => {
                self.emit(&[0x58]); // pop rax (return value)
                self.generate_epilogue();
            }

            // Ignore DefineFunction in instruction generation
            IRInstruction::DefineFunction(_, _, _) => {}

            // Other instructions - basic implementations or placeholders
            _ => {
                // Placeholder for unimplemented instructions
            }
        }
    }

    // Fast position calculation without code generation
    fn calculate_positions(&mut self, program: &IRProgram) {
        self.instruction_positions.clear();
        let mut position = 0;

        // Account for prologue if we have local variables
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
            position += self.instruction_size_with_context(instruction, has_locals);
        }
    }

    fn instruction_size_with_context(
        &self,
        instruction: &IRInstruction,
        has_locals: bool,
    ) -> usize {
        match instruction {
            IRInstruction::Push(value) => {
                if *value <= 127 && *value >= -128 {
                    2
                } else {
                    5
                }
            }
            IRInstruction::Add => 6, // 1+1+3+1: pop rax + pop rbx + add rax,rbx + push rax
            IRInstruction::Sub => 6, // 1+1+3+1: pop rax + pop rbx + sub rbx,rax + push rbx
            IRInstruction::Mul => 7, // 1+1+4+1: pop rax + pop rbx + imul rbx,rax + push rbx
            IRInstruction::Div => 14, // 1+1+3+3+2+3+1: pop + pop + mov + mov + cqo + idiv + push
            IRInstruction::Equal => 13, // 1+1+3+3+4+1: pop + pop + cmp + sete + movzx + push
            IRInstruction::Less => 13, // 1+1+3+3+4+1: pop + pop + cmp + setl + movzx + push
            IRInstruction::Greater => 13, // 1+1+3+3+4+1: pop + pop + cmp + setg + movzx + push
            IRInstruction::LessEqual => 13, // 1+1+3+3+4+1: pop + pop + cmp + setle + movzx + push
            IRInstruction::GreaterEqual => 13, // 1+1+3+3+4+1: pop + pop + cmp + setge + movzx + push
            IRInstruction::Not => 12,          // 1+4+3+4+1: pop + cmp rax,0 + sete + movzx + push
            IRInstruction::JumpIfZero(_) => 10, // pop + cmp + je with 32-bit offset
            IRInstruction::Jump(_) => 5,       // jmp with 32-bit offset
            IRInstruction::StoreLocal(_) => 11, // mov [rbp-offset], rax; pop rax
            IRInstruction::LoadLocal(_) => 10, // push [rbp-offset]
            IRInstruction::Return => {
                if has_locals {
                    5 // pop rax + mov rsp,rbp + pop rbp
                } else {
                    1 // pop rax
                }
            }
            // Function operations (placeholder sizes for now)
            IRInstruction::DefineFunction(_, _, _) => 0, // No code generated, just metadata
            IRInstruction::Call(_, _) => 5,              // call instruction placeholder
            IRInstruction::CallIndirect(_) => 5,         // call instruction placeholder
            IRInstruction::PushFrame(_) => 8,            // stack frame setup placeholder
            IRInstruction::PopFrame => 4,                // stack frame teardown placeholder
            IRInstruction::StoreParam(_) => 8,           // parameter store placeholder
            IRInstruction::LoadParam(_) => 8,            // parameter load placeholder
        }
    }

    // Single code generation pass with known positions
    fn generate_code(&mut self, program: &IRProgram, has_locals: bool) {
        // Add prologue if we have local variables
        if has_locals {
            self.emit(&[0x55]); // push rbp
            self.emit(&[0x48, 0x89, 0xe5]); // mov rbp, rsp
                                            // Reserve space for locals (we'll use a fixed amount)
            self.emit(&[0x48, 0x83, 0xec, 0x80]); // sub rsp, 128 (space for 16 locals)
        }
        for (_i, instruction) in program.instructions.iter().enumerate() {
            match instruction {
                IRInstruction::Push(value) => {
                    if *value <= 127 && *value >= -128 {
                        self.emit(&[0x6a]);
                        self.emit(&[*value as u8]);
                    } else {
                        self.emit(&[0x68]);
                        self.emit(&(*value as u32).to_le_bytes());
                    }
                }

                IRInstruction::Add => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x5b]); // pop rbx
                    self.emit(&[0x48, 0x01, 0xd8]); // add rax, rbx
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Sub => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x5b]); // pop rbx
                    self.emit(&[0x48, 0x29, 0xc3]); // sub rbx, rax
                    self.emit(&[0x53]); // push rbx
                }

                IRInstruction::Mul => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x5b]); // pop rbx
                    self.emit(&[0x48, 0x0f, 0xaf, 0xd8]); // imul rbx, rax
                    self.emit(&[0x53]); // push rbx
                }

                IRInstruction::Div => {
                    self.emit(&[0x58]); // pop rax (divisor)
                    self.emit(&[0x5b]); // pop rbx (dividend)
                    self.emit(&[0x48, 0x89, 0xd8]); // mov rax, rbx
                    self.emit(&[0x48, 0x89, 0xc1]); // mov rcx, rax
                    self.emit(&[0x48, 0x99]); // cqo
                    self.emit(&[0x48, 0xf7, 0xf9]); // idiv rcx
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Equal => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x5b]); // pop rbx
                    self.emit(&[0x48, 0x39, 0xd8]); // cmp rax, rbx
                    self.emit(&[0x0f, 0x94, 0xc0]); // sete al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Less => {
                    self.emit(&[0x58]); // pop rax (second)
                    self.emit(&[0x5b]); // pop rbx (first)
                    self.emit(&[0x48, 0x39, 0xc3]); // cmp rbx, rax
                    self.emit(&[0x0f, 0x9c, 0xc0]); // setl al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Greater => {
                    self.emit(&[0x58]); // pop rax (second)
                    self.emit(&[0x5b]); // pop rbx (first)
                    self.emit(&[0x48, 0x39, 0xc3]); // cmp rbx, rax
                    self.emit(&[0x0f, 0x9f, 0xc0]); // setg al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::LessEqual => {
                    self.emit(&[0x58]); // pop rax (second)
                    self.emit(&[0x5b]); // pop rbx (first)
                    self.emit(&[0x48, 0x39, 0xc3]); // cmp rbx, rax
                    self.emit(&[0x0f, 0x9e, 0xc0]); // setle al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::GreaterEqual => {
                    self.emit(&[0x58]); // pop rax (second)
                    self.emit(&[0x5b]); // pop rbx (first)
                    self.emit(&[0x48, 0x39, 0xc3]); // cmp rbx, rax
                    self.emit(&[0x0f, 0x9d, 0xc0]); // setge al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Not => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x48, 0x83, 0xf8, 0x00]); // cmp rax, 0
                    self.emit(&[0x0f, 0x94, 0xc0]); // sete al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::JumpIfZero(target) => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x48, 0x83, 0xf8, 0x00]); // cmp rax, 0

                    if *target >= self.instruction_positions.len() {
                        panic!(
                            "JumpIfZero target {} out of bounds (max {})",
                            *target,
                            self.instruction_positions.len() - 1
                        );
                    }

                    let current_instruction_pos = self.instruction_positions[_i];
                    let current_pos = current_instruction_pos + 10; // Position after complete JumpIfZero instruction
                    let target_pos = self.instruction_positions[*target];
                    let offset = (target_pos as i32) - (current_pos as i32);

                    self.emit(&[0x0f, 0x84]); // je (2 bytes)
                    self.emit(&offset.to_le_bytes()); // 4-byte offset
                }

                IRInstruction::Jump(target) => {
                    if *target >= self.instruction_positions.len() {
                        panic!(
                            "Jump target {} out of bounds (max {})",
                            *target,
                            self.instruction_positions.len() - 1
                        );
                    }

                    let current_instruction_pos = self.instruction_positions[_i];
                    let current_pos = current_instruction_pos + 5; // Position after jmp instruction (1+4)
                    let target_pos = self.instruction_positions[*target];
                    let offset = (target_pos as i32) - (current_pos as i32);

                    self.emit(&[0xe9]); // jmp
                    self.emit(&offset.to_le_bytes());
                }

                IRInstruction::StoreLocal(slot) => {
                    self.emit(&[0x58]); // pop rax
                                        // mov [rbp - 8*(slot+1)], rax
                    let offset = 8 * (slot + 1);
                    if offset <= 127 {
                        self.emit(&[0x48, 0x89, 0x45]);
                        self.emit(&[((-(offset as i8)) as u8)]);
                    } else {
                        self.emit(&[0x48, 0x89, 0x85]);
                        self.emit(&(-(offset as i32)).to_le_bytes());
                    }
                }

                IRInstruction::LoadLocal(slot) => {
                    // push [rbp - 8*(slot+1)]
                    let offset = 8 * (slot + 1);
                    if offset <= 127 {
                        self.emit(&[0xff, 0x75]);
                        self.emit(&[((-(offset as i8)) as u8)]);
                    } else {
                        self.emit(&[0xff, 0xb5]);
                        self.emit(&(-(offset as i32)).to_le_bytes());
                    }
                }

                IRInstruction::Return => {
                    self.emit(&[0x58]); // pop rax
                                        // Add epilogue if we have local variables
                    if has_locals {
                        self.emit(&[0x48, 0x89, 0xec]); // mov rsp, rbp
                        self.emit(&[0x5d]); // pop rbp
                    }
                }

                // Function operations (placeholders for Phase 4.2)
                IRInstruction::DefineFunction(_, _, _) => {
                    // No code generation needed - this is metadata only
                }
                IRInstruction::Call(_, _) => {
                    // TODO: Implement function call in Phase 4.3
                    // For now, just push 0 as placeholder
                    self.emit(&[0x6a, 0x00]); // push 0
                }
                IRInstruction::CallIndirect(_) => {
                    // TODO: Implement indirect call in Phase 4.3
                    // For now, just push 0 as placeholder
                    self.emit(&[0x6a, 0x00]); // push 0
                }
                IRInstruction::PushFrame(_) => {
                    // TODO: Implement stack frame management in Phase 4.3
                    // For now, no-op
                }
                IRInstruction::PopFrame => {
                    // TODO: Implement stack frame management in Phase 4.3
                    // For now, no-op
                }
                IRInstruction::StoreParam(_) => {
                    // TODO: Implement parameter handling in Phase 4.3
                    // For now, just pop the value
                    self.emit(&[0x58]); // pop rax
                }
                IRInstruction::LoadParam(slot) => {
                    // Load parameter from stack location [rbp - 8*(slot+1)]
                    // Parameters are stored after prologue, right after rbp
                    let offset = 8 * (slot + 1);
                    if offset <= 127 {
                        self.emit(&[0xff, 0x75]); // push [rbp+offset]
                        self.emit(&[((-(offset as i8)) as u8)]);
                    } else {
                        self.emit(&[0xff, 0xb5]); // push [rbp+offset]
                        self.emit(&(-(offset as i32)).to_le_bytes());
                    }
                }
            }
        }
    }

    fn emit(&mut self, bytes: &[u8]) {
        self.code.extend_from_slice(bytes);
    }
}

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

    #[test]
    fn test_conditional_ir() {
        // Test: if (> 5 3) 42 0 = 42
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(5));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Greater);
        program.add_instruction(IRInstruction::JumpIfZero(6));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Jump(7));
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_conditional_false() {
        // Test: if (> 3 5) 42 0 = 0
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Push(5));
        program.add_instruction(IRInstruction::Greater);
        program.add_instruction(IRInstruction::JumpIfZero(6));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Jump(7));
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_not_operation() {
        // Test: (not 0) = 1
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Not);
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 1);
    }
}
