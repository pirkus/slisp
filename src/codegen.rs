use crate::ir::{IRInstruction, IRProgram};

pub struct X86CodeGen {
    code: Vec<u8>,
    instruction_positions: Vec<usize>,
}

impl X86CodeGen {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            instruction_positions: Vec::new(),
        }
    }

    // Optimized single-pass code generation with size pre-calculation
    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
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

    // Calculate size of a single instruction in bytes
    fn instruction_size(&self, instruction: &IRInstruction) -> usize {
        self.instruction_size_with_context(instruction, false)
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
            IRInstruction::Pop => 1,           // pop rax
            IRInstruction::StoreLocal(_) => 11, // mov [rbp-offset], rax; pop rax
            IRInstruction::LoadLocal(_) => 10, // push [rbp-offset]
            IRInstruction::Return => {
                if has_locals {
                    5 // pop rax + mov rsp,rbp + pop rbp
                } else {
                    1 // pop rax
                }
            }
            IRInstruction::And | IRInstruction::Or => 0, // handled in compiler logic
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

                IRInstruction::Pop => {
                    self.emit(&[0x58]); // pop rax (discard)
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

                IRInstruction::And | IRInstruction::Or => {
                    // These are handled in compiler logic
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
