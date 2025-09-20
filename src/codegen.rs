use crate::ir::{IRInstruction, IRProgram};

pub struct X86CodeGen {
    code: Vec<u8>,
}

impl X86CodeGen {
    pub fn new() -> Self {
        Self { code: Vec::new() }
    }

    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        // Use register-based evaluation for ELF compatibility
        // RAX = accumulator, RBX = temp register

        let mut has_value = false;

        for instruction in &program.instructions {
            match instruction {
                IRInstruction::Push(value) => {
                    if !has_value {
                        // First value goes to RAX
                        self.emit(&[0x48, 0xc7, 0xc0]); // mov rax, imm32
                        self.emit(&(*value as u32).to_le_bytes());
                        has_value = true;
                    } else {
                        // Second value goes to RBX
                        self.emit(&[0x48, 0xc7, 0xc3]); // mov rbx, imm32
                        self.emit(&(*value as u32).to_le_bytes());
                    }
                }

                IRInstruction::Add => {
                    // add rax, rbx
                    self.emit(&[0x48, 0x01, 0xd8]);
                }

                IRInstruction::Sub => {
                    // sub rax, rbx
                    self.emit(&[0x48, 0x29, 0xd8]);
                }

                IRInstruction::Mul => {
                    // imul rax, rbx
                    self.emit(&[0x48, 0x0f, 0xaf, 0xc3]);
                }

                IRInstruction::Div => {
                    // cqo (sign extend rax to rdx:rax)
                    self.emit(&[0x48, 0x99]);
                    // idiv rbx (divide rdx:rax by rbx, quotient in rax)
                    self.emit(&[0x48, 0xf7, 0xfb]);
                }

                IRInstruction::Return => {
                    // Result is already in RAX, nothing to do
                }

                _ => {
                    // Unimplemented operations
                }
            }
        }

        // If no operations were performed, ensure RAX has a value
        if !has_value {
            self.emit(&[0x48, 0xc7, 0xc0, 0x00, 0x00, 0x00, 0x00]); // mov rax, 0
        }

        self.code.clone()
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

        // For JIT testing, we need to add a ret instruction
        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_arithmetic() {
        // Test addition: 2 + 3 = 5
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
    fn test_multiplication() {
        // Test multiplication: 4 * 5 = 20
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(4));
        program.add_instruction(IRInstruction::Push(5));
        program.add_instruction(IRInstruction::Mul);
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 20);
    }

    #[test]
    fn test_subtraction() {
        // Test subtraction: 10 - 3 = 7
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(10));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Sub);
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 7);
    }
}
