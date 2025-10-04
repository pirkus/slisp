/// x86-64 instruction generation
/// Each function generates machine code for a specific operation

use crate::ir::FunctionInfo;
use std::collections::HashMap;

/// Generate machine code for a push immediate instruction
pub fn generate_push(value: i64) -> Vec<u8> {
    if value <= 127 && value >= -128 {
        vec![0x6a, value as u8] // push imm8
    } else {
        let mut code = vec![0x68]; // push imm32
        code.extend_from_slice(&(value as u32).to_le_bytes());
        code
    }
}

/// Generate machine code for addition
pub fn generate_add() -> Vec<u8> {
    vec![
        0x58,             // pop rax
        0x5b,             // pop rbx
        0x48, 0x01, 0xd8, // add rax, rbx
        0x50,             // push rax
    ]
}

/// Generate machine code for subtraction
pub fn generate_sub() -> Vec<u8> {
    vec![
        0x58,             // pop rax
        0x5b,             // pop rbx
        0x48, 0x29, 0xc3, // sub rbx, rax
        0x53,             // push rbx
    ]
}

/// Generate machine code for multiplication
pub fn generate_mul() -> Vec<u8> {
    vec![
        0x58,                   // pop rax
        0x5b,                   // pop rbx
        0x48, 0x0f, 0xaf, 0xd8, // imul rbx, rax
        0x53,                   // push rbx
    ]
}

/// Generate machine code for division
pub fn generate_div() -> Vec<u8> {
    vec![
        0x58,             // pop rax (divisor)
        0x5b,             // pop rbx (dividend)
        0x48, 0x89, 0xd8, // mov rax, rbx
        0x48, 0x89, 0xc1, // mov rcx, rax
        0x48, 0x99,       // cqo (sign extend)
        0x48, 0xf7, 0xf9, // idiv rcx
        0x50,             // push rax
    ]
}

/// Generate machine code for loading a parameter from stack
pub fn generate_load_param(slot: usize) -> Vec<u8> {
    let offset = 8 * (slot + 1);
    if offset <= 127 {
        vec![
            0xff, 0x75,                    // push [rbp+offset]
            ((-(offset as i8)) as u8),
        ]
    } else {
        let mut code = vec![0xff, 0xb5]; // push [rbp+offset]
        code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
        code
    }
}

/// Generate machine code for storing a local variable
pub fn generate_store_local(slot: usize, func_info: &FunctionInfo) -> Vec<u8> {
    let mut code = vec![0x58]; // pop rax

    // Store at [rbp - 8*(func_info.param_count + slot + 1)]
    // Locals come after parameters
    let offset = 8 * (func_info.param_count + slot + 1);
    if offset <= 127 {
        code.extend_from_slice(&[0x48, 0x89, 0x45]);
        code.push((-(offset as i8)) as u8);
    } else {
        code.extend_from_slice(&[0x48, 0x89, 0x85]);
        code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
    }

    code
}

/// Generate machine code for loading a local variable
pub fn generate_load_local(slot: usize, func_info: &FunctionInfo) -> Vec<u8> {
    // Load from [rbp - 8*(func_info.param_count + slot + 1)]
    let offset = 8 * (func_info.param_count + slot + 1);
    if offset <= 127 {
        vec![
            0xff, 0x75,                    // push [rbp+offset]
            ((-(offset as i8)) as u8),
        ]
    } else {
        let mut code = vec![0xff, 0xb5]; // push [rbp+offset]
        code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
        code
    }
}

/// Generate machine code for a function call
pub fn generate_call(
    func_name: &str,
    function_addresses: &HashMap<String, usize>,
    current_pos: usize,
) -> Vec<u8> {
    let mut code = Vec::new();

    if let Some(&func_addr) = function_addresses.get(func_name) {
        let call_offset = (func_addr as i32) - ((current_pos + 5) as i32);
        code.push(0xe8); // call
        code.extend_from_slice(&call_offset.to_le_bytes());
    } else {
        // Function not yet generated, emit placeholder
        code.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]); // call 0 (placeholder)
    }

    // Result is in RAX, push it onto stack
    code.push(0x50); // push rax

    code
}

/// Generate machine code for return (just pop return value into RAX)
/// Epilogue is generated separately
pub fn generate_return() -> Vec<u8> {
    vec![0x58] // pop rax (return value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_small() {
        assert_eq!(generate_push(42), vec![0x6a, 42]);
    }

    #[test]
    fn test_push_large() {
        let code = generate_push(1000);
        assert_eq!(code[0], 0x68);
        assert_eq!(code.len(), 5);
    }

    #[test]
    fn test_arithmetic_ops() {
        assert_eq!(generate_add().len(), 6);
        assert_eq!(generate_sub().len(), 6);
        assert_eq!(generate_mul().len(), 7);
        assert_eq!(generate_div().len(), 14);
    }
}
