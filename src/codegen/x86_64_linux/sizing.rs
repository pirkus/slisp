/// Instruction size calculation for position-independent code generation
/// Used in the first pass to determine instruction addresses
use crate::ir::IRInstruction;

/// Calculate the size of an instruction in bytes
/// Used for pre-calculating instruction positions before code generation
pub fn instruction_size(instruction: &IRInstruction, has_locals: bool) -> usize {
    match instruction {
        IRInstruction::Push(value) => {
            if *value <= 127 && *value >= -128 {
                2 // push imm8
            } else {
                5 // push imm32
            }
        }
        IRInstruction::Add => 6,
        IRInstruction::Sub => 6,
        IRInstruction::Mul => 7,
        IRInstruction::Div => 14,
        IRInstruction::Equal => 13,
        IRInstruction::Less => 13,
        IRInstruction::Greater => 13,
        IRInstruction::LessEqual => 13,
        IRInstruction::GreaterEqual => 13,
        IRInstruction::Not => 12,
        IRInstruction::JumpIfZero(_) => 10, // pop + cmp + je with 32-bit offset
        IRInstruction::Jump(_) => 5,        // jmp with 32-bit offset
        IRInstruction::StoreLocal(_) => 11, // pop rax + mov [rbp-offset], rax (varies)
        IRInstruction::LoadLocal(_) => 10,  // push [rbp-offset] (varies)
        IRInstruction::LoadParam(_) => 3,   // push [rbp-offset] (short form)
        IRInstruction::Return => {
            if has_locals {
                5 // pop rax + epilogue (mov rsp,rbp + pop rbp)
            } else {
                1 // pop rax only
            }
        }
        // Function operations
        IRInstruction::DefineFunction(_, _, _) => 0, // No code generated, just metadata
        IRInstruction::Call(_, _) => 6,              // call + push rax (varies with offset)
        // String operations
        IRInstruction::PushString(_) => 10, // movabs rax, <address> + push rax
        // Memory allocation
        IRInstruction::InitHeap => 0,      // Generated as runtime function, not inline
        IRInstruction::Allocate(_) => 10,  // mov rdi, size + call _allocate + push rax
        IRInstruction::Free => 6,          // pop rdi + call _free
        IRInstruction::FreeLocal(_) => 11, // push rax (1) + mov rdi,[rbp-N] (4) + call _free (5) + pop rax (1)
        // Runtime function calls
        IRInstruction::RuntimeCall(_, arg_count) => {
            let pop_size = match arg_count {
                0 => 0,
                1 => 1, // pop rdi
                2 => 2, // pop rsi + pop rdi
                _ => 3, // conservative estimate
            };
            pop_size + 5 + 1 // pops + call (5 bytes) + push rax (1 byte)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_sizes() {
        assert_eq!(instruction_size(&IRInstruction::Push(42), false), 2);
        assert_eq!(instruction_size(&IRInstruction::Push(1000), false), 5);
    }

    #[test]
    fn test_arithmetic_sizes() {
        assert_eq!(instruction_size(&IRInstruction::Add, false), 6);
        assert_eq!(instruction_size(&IRInstruction::Sub, false), 6);
        assert_eq!(instruction_size(&IRInstruction::Mul, false), 7);
        assert_eq!(instruction_size(&IRInstruction::Div, false), 14);
    }

    #[test]
    fn test_return_size() {
        assert_eq!(instruction_size(&IRInstruction::Return, false), 1);
        assert_eq!(instruction_size(&IRInstruction::Return, true), 5);
    }
}
