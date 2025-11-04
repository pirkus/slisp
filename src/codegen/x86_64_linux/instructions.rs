/// x86-64 instruction generation
/// Each function generates machine code for a specific operation
use crate::ir::FunctionInfo;
use std::collections::HashMap;

/// Generate machine code for a push immediate instruction
pub fn generate_push(value: i64) -> Vec<u8> {
    if (-128..=127).contains(&value) {
        vec![0x6a, value as u8] // push imm8
    } else {
        let mut code = vec![0x68]; // push imm32
        code.extend_from_slice(&(value as u32).to_le_bytes());
        code
    }
}

/// Generate machine code to push a string address
/// The address will be filled in during ELF generation
pub fn generate_push_string(address: u64) -> Vec<u8> {
    let mut code = Vec::new();
    // movabs rax, address (10 bytes)
    code.push(0x48); // REX.W prefix
    code.push(0xb8); // mov rax, imm64
    code.extend_from_slice(&address.to_le_bytes());
    // push rax
    code.push(0x50);
    code
}

/// Generate machine code for addition
pub fn generate_add() -> Vec<u8> {
    vec![
        0x58, // pop rax
        0x5b, // pop rbx
        0x48, 0x01, 0xd8, // add rax, rbx
        0x50, // push rax
    ]
}

/// Generate machine code for subtraction
pub fn generate_sub() -> Vec<u8> {
    vec![
        0x58, // pop rax
        0x5b, // pop rbx
        0x48, 0x29, 0xc3, // sub rbx, rax
        0x53, // push rbx
    ]
}

/// Generate machine code for multiplication
pub fn generate_mul() -> Vec<u8> {
    vec![
        0x58, // pop rax
        0x5b, // pop rbx
        0x48, 0x0f, 0xaf, 0xd8, // imul rbx, rax
        0x53, // push rbx
    ]
}

/// Generate machine code for division
pub fn generate_div() -> Vec<u8> {
    vec![
        0x58, // pop rax (divisor)
        0x5b, // pop rbx (dividend)
        0x48, 0x89, 0xd8, // mov rax, rbx
        0x48, 0x89, 0xc1, // mov rcx, rax
        0x48, 0x99, // cqo (sign extend)
        0x48, 0xf7, 0xf9, // idiv rcx
        0x50, // push rax
    ]
}

/// Generate machine code for loading a parameter from stack
pub fn generate_load_param(slot: usize) -> Vec<u8> {
    let offset = 8 * (slot + 1);
    if offset <= 127 {
        vec![
            0xff,
            0x75, // push [rbp+offset]
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

    // Locals stored at [rbp - 8*(param_count + slot + 1)]
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
    let offset = 8 * (func_info.param_count + slot + 1);
    if offset <= 127 {
        vec![
            0xff,
            0x75, // push [rbp+offset]
            ((-(offset as i8)) as u8),
        ]
    } else {
        let mut code = vec![0xff, 0xb5]; // push [rbp+offset]
        code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
        code
    }
}

/// Generate machine code that pushes the address of a local slot onto the stack
pub fn generate_push_local_address(slot: usize, func_info: &FunctionInfo) -> Vec<u8> {
    let offset = 8 * (func_info.param_count + slot + 1);
    let mut code = Vec::new();

    // Load effective address of [rbp - offset] into RAX (LEA), then push it.
    // We mirror the addressing scheme used by Store/Load to stay consistent
    // with how locals are laid out beneath the base pointer.
    if offset <= 127 {
        code.extend_from_slice(&[0x48, 0x8d, 0x45, (-(offset as i8)) as u8]);
    } else {
        code.extend_from_slice(&[0x48, 0x8d, 0x85]);
        code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
    }

    code.push(0x50); // push rax
    code
}

/// Generate machine code for a function call
pub fn generate_call(func_name: &str, function_addresses: &HashMap<String, usize>, current_pos: usize) -> Vec<u8> {
    let mut code = Vec::new();

    if let Some(&func_addr) = function_addresses.get(func_name) {
        let call_offset = (func_addr as i32) - ((current_pos + 5) as i32);
        code.push(0xe8); // call
        code.extend_from_slice(&call_offset.to_le_bytes());
    } else {
        code.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]); // call 0
    }

    code.push(0x50); // push rax

    code
}

/// Generate machine code for return (just pop return value into RAX)
/// Epilogue is generated separately
pub fn generate_return() -> Vec<u8> {
    vec![0x58] // pop rax (return value)
}

/// Generate machine code to call _heap_init runtime function
/// Returns (code bytes, offset within code where 32-bit displacement resides)
pub fn generate_call_heap_init(heap_init_offset: Option<i32>) -> (Vec<u8>, usize) {
    let mut code = Vec::with_capacity(5);
    code.push(0xe8); // call relative
    code.extend_from_slice(&heap_init_offset.unwrap_or(0).to_le_bytes());
    (code, 1)
}

/// Generate machine code for Allocate instruction
/// Takes size as immediate, puts it in RDI, calls _allocate
pub fn generate_allocate_inline(size: usize, allocate_offset: Option<i32>) -> (Vec<u8>, usize) {
    let mut code = Vec::with_capacity(13);

    // mov rdi, size (put size in first argument register)
    code.push(0x48); // REX.W
    code.push(0xc7); // mov r/m64, imm32
    code.push(0xc7); // ModR/M byte for RDI
    code.extend_from_slice(&(size as u32).to_le_bytes());

    let call_disp_offset = code.len() + 1;

    // call _allocate
    code.push(0xe8);
    code.extend_from_slice(&allocate_offset.unwrap_or(0).to_le_bytes());
    code.push(0x50); // push rax (save allocated pointer on stack)

    (code, call_disp_offset)
}

/// Generate machine code for Free instruction
/// Pops address from stack, puts it in RDI, calls _free
pub fn generate_free_inline(free_offset: Option<i32>) -> (Vec<u8>, usize) {
    let mut code = Vec::with_capacity(6);

    // pop rdi (get address from stack into first argument register)
    code.push(0x5f);

    // call _free
    code.push(0xe8); // call relative
    code.extend_from_slice(&free_offset.unwrap_or(0).to_le_bytes());

    (code, 2)
}

/// Generate machine code for a runtime function call
/// Pops arguments from stack into registers (System V ABI), calls function, pushes result
pub fn generate_runtime_call(runtime_offset: Option<i32>, arg_count: usize) -> (Vec<u8>, usize) {
    let mut code = Vec::new();

    // Pop arguments from stack into registers (right-to-left for System V ABI)
    // System V ABI integer argument registers: RDI, RSI, RDX, RCX, R8, R9
    if arg_count > 6 {
        panic!("Runtime calls with more than 6 arguments not yet supported");
    }

    if arg_count > 0 {
        const POP_SEQUENCES: [&[u8]; 6] = [
            &[0x5f],       // pop rdi
            &[0x5e],       // pop rsi
            &[0x5a],       // pop rdx
            &[0x59],       // pop rcx
            &[0x41, 0x58], // pop r8
            &[0x41, 0x59], // pop r9
        ];

        for index in (0..arg_count).rev() {
            code.extend_from_slice(POP_SEQUENCES[index]);
        }
    }

    // call runtime_function
    let call_disp_offset = code.len() + 1;
    code.push(0xe8); // call relative
    code.extend_from_slice(&runtime_offset.unwrap_or(0).to_le_bytes());

    // push rax (return value onto stack)
    code.push(0x50);

    (code, call_disp_offset)
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

    #[test]
    fn runtime_call_supports_up_to_six_args() {
        let (code, _) = generate_runtime_call(Some(0), 5);
        let expected_prefix = [0x41, 0x58, 0x59, 0x5a, 0x5e, 0x5f];
        assert_eq!(&code[..6], &expected_prefix);
        assert_eq!(code[6], 0xe8);
        assert_eq!(code[11], 0x50);

        let (code_six, _) = generate_runtime_call(Some(0), 6);
        let expected_prefix_six = [0x41, 0x59, 0x41, 0x58, 0x59, 0x5a, 0x5e, 0x5f];
        assert_eq!(&code_six[..8], &expected_prefix_six);
        assert_eq!(code_six[8], 0xe8);
        assert_eq!(code_six[13], 0x50);
    }
}
