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

    // Two-pass code generation for accurate jump offsets
    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        // First pass: generate code and record instruction positions
        self.first_pass(program);

        // Second pass: regenerate with correct jump offsets
        self.code.clear();
        self.second_pass(program);

        self.code.clone()
    }

    fn first_pass(&mut self, program: &IRProgram) {
        self.instruction_positions.clear();
        self.code.clear();

        for instruction in &program.instructions {
            self.instruction_positions.push(self.code.len());

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

                IRInstruction::JumpIfZero(_) => {
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x48, 0x83, 0xf8, 0x00]); // cmp rax, 0
                    self.emit(&[0x0f, 0x84]); // je
                    self.emit(&[0x00, 0x00, 0x00, 0x00]); // placeholder offset
                }

                IRInstruction::Jump(_) => {
                    self.emit(&[0xe9]); // jmp
                    self.emit(&[0x00, 0x00, 0x00, 0x00]); // placeholder offset
                }

                IRInstruction::Pop => {
                    self.emit(&[0x58]); // pop rax (discard)
                }

                IRInstruction::Return => {
                    self.emit(&[0x58]); // pop rax
                }

                IRInstruction::And | IRInstruction::Or => {
                    // These are handled in compiler logic
                }
            }
        }
    }

    fn second_pass(&mut self, program: &IRProgram) {
        for (i, instruction) in program.instructions.iter().enumerate() {
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

                    let current_pos = self.code.len() + 6; // Position after this instruction
                    let target_pos = self.instruction_positions[*target];
                    let offset = (target_pos as i32) - (current_pos as i32);

                    self.emit(&[0x0f, 0x84]); // je
                    self.emit(&offset.to_le_bytes());
                }

                IRInstruction::Jump(target) => {
                    let current_pos = self.code.len() + 5; // Position after this instruction
                    let target_pos = self.instruction_positions[*target];
                    let offset = (target_pos as i32) - (current_pos as i32);

                    self.emit(&[0xe9]); // jmp
                    self.emit(&offset.to_le_bytes());
                }

                IRInstruction::Pop => {
                    self.emit(&[0x58]); // pop rax (discard)
                }

                IRInstruction::Return => {
                    self.emit(&[0x58]); // pop rax
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
