use crate::ir::{IRInstruction, IRProgram};

pub struct X86CodeGen {
    code: Vec<u8>,
}

impl X86CodeGen {
    pub fn new() -> Self {
        Self { code: Vec::new() }
    }

    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        // Stack-based evaluation using CPU stack
        // This approach can handle unlimited operands and nested expressions

        for instruction in &program.instructions {
            match instruction {
                IRInstruction::Push(value) => {
                    // Push immediate value onto CPU stack
                    if *value <= 127 && *value >= -128 {
                        // Use 8-bit immediate for small values
                        self.emit(&[0x6a]); // push imm8
                        self.emit(&[*value as u8]);
                    } else {
                        // Use 32-bit immediate for larger values
                        self.emit(&[0x68]); // push imm32
                        self.emit(&(*value as u32).to_le_bytes());
                    }
                }

                IRInstruction::Add => {
                    // Pop two values, add them, push result
                    self.emit(&[0x58]); // pop rax (second operand)
                    self.emit(&[0x5b]); // pop rbx (first operand)
                    self.emit(&[0x48, 0x01, 0xd8]); // add rax, rbx
                    self.emit(&[0x50]); // push rax (result)
                }

                IRInstruction::Sub => {
                    // Pop two values, subtract them, push result
                    self.emit(&[0x58]); // pop rax (second operand)
                    self.emit(&[0x5b]); // pop rbx (first operand)
                    self.emit(&[0x48, 0x29, 0xc3]); // sub rbx, rax (rbx = rbx - rax)
                    self.emit(&[0x53]); // push rbx (result)
                }

                IRInstruction::Mul => {
                    // Pop two values, multiply them, push result
                    self.emit(&[0x58]); // pop rax (second operand)
                    self.emit(&[0x5b]); // pop rbx (first operand)
                    self.emit(&[0x48, 0x0f, 0xaf, 0xd8]); // imul rbx, rax
                    self.emit(&[0x53]); // push rbx (result)
                }

                IRInstruction::Div => {
                    // Pop two values, divide them, push result
                    self.emit(&[0x58]); // pop rax (divisor)
                    self.emit(&[0x5b]); // pop rbx (dividend)
                    self.emit(&[0x48, 0x89, 0xd8]); // mov rax, rbx (dividend to rax)
                    self.emit(&[0x48, 0x89, 0xc1]); // mov rcx, rax (save divisor)
                    self.emit(&[0x48, 0x99]); // cqo (sign extend rax to rdx:rax)
                    self.emit(&[0x48, 0xf7, 0xf9]); // idiv rcx
                    self.emit(&[0x50]); // push rax (quotient)
                }

                IRInstruction::Equal => {
                    // Pop two values, compare for equality, push result (1 or 0)
                    self.emit(&[0x58]); // pop rax
                    self.emit(&[0x5b]); // pop rbx
                    self.emit(&[0x48, 0x39, 0xd8]); // cmp rax, rbx
                    self.emit(&[0x0f, 0x94, 0xc0]); // sete al (set al=1 if equal, 0 otherwise)
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al (zero-extend al to rax)
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Less => {
                    // Pop two values, compare if first < second, push result
                    self.emit(&[0x58]); // pop rax (second)
                    self.emit(&[0x5b]); // pop rbx (first)
                    self.emit(&[0x48, 0x39, 0xc3]); // cmp rbx, rax (first cmp second)
                    self.emit(&[0x0f, 0x9c, 0xc0]); // setl al (set al=1 if first < second)
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Greater => {
                    // Pop two values, compare if first > second, push result
                    self.emit(&[0x58]); // pop rax (second)
                    self.emit(&[0x5b]); // pop rbx (first)
                    self.emit(&[0x48, 0x39, 0xc3]); // cmp rbx, rax
                    self.emit(&[0x0f, 0x9f, 0xc0]); // setg al
                    self.emit(&[0x48, 0x0f, 0xb6, 0xc0]); // movzx rax, al
                    self.emit(&[0x50]); // push rax
                }

                IRInstruction::Return => {
                    // Pop final result from stack into rax
                    self.emit(&[0x58]); // pop rax
                }

                _ => {
                    // Unimplemented operations - for now just ignore
                }
            }
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
    fn test_basic_arithmetic() {
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
    fn test_multi_operand_arithmetic() {
        // Test: 1 + 2 + 3 + 4 = 10
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(1));
        program.add_instruction(IRInstruction::Push(2));
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Push(4));
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 10);
    }

    #[test]
    fn test_nested_expression() {
        // Test: 2 + (3 * 4) = 14
        // IR: push 2, push 3, push 4, mul, add
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(2));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Push(4));
        program.add_instruction(IRInstruction::Mul);
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 14);
    }

    #[test]
    fn test_comparison() {
        // Test: 5 > 3 = 1 (true)
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(5));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Greater);
        program.add_instruction(IRInstruction::Return);

        let machine_code = compile_to_executable(&program);

        let mut jit_code = machine_code;
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 1); // true
    }

    #[test]
    fn test_subtraction() {
        // Test: 10 - 3 = 7
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
