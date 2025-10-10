/// System V ABI implementation for x86-64
/// Handles calling convention, stack frame management, and register usage

use crate::ir::FunctionInfo;

/// Generate function prologue following System V ABI
/// - Save old base pointer
/// - Set up new stack frame
/// - Allocate stack space for params + locals + scratch
/// - Save parameter registers to stack
pub fn generate_prologue(func_info: &FunctionInfo) -> Vec<u8> {
    let mut code = Vec::new();

    code.push(0x55); // push rbp
    code.extend_from_slice(&[0x48, 0x89, 0xe5]); // mov rbp, rsp

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
            code.extend_from_slice(&[0x48, 0x83, 0xec, stack_size as u8]); // sub rsp, imm8
        } else {
            code.extend_from_slice(&[0x48, 0x81, 0xec]); // sub rsp, imm32
            code.extend_from_slice(&(stack_size as u32).to_le_bytes());
        }
    }

    // Save parameters to stack (System V ABI: RDI, RSI, RDX, RCX, R8, R9)
    // We need to save them because they'll be used by child function calls
    let param_reg_codes: Vec<&[u8]> = vec![
        &[0x48, 0x89, 0x7d], // mov [rbp+offset], rdi
        &[0x48, 0x89, 0x75], // mov [rbp+offset], rsi
        &[0x48, 0x89, 0x55], // mov [rbp+offset], rdx
        &[0x48, 0x89, 0x4d], // mov [rbp+offset], rcx
        &[0x4c, 0x89, 0x45], // mov [rbp+offset], r8
        &[0x4c, 0x89, 0x4d], // mov [rbp+offset], r9
    ];

    for (i, &reg_code) in param_reg_codes.iter().enumerate().take(func_info.param_count.min(6)) {
        let offset = 8 * (i + 1);
        code.extend_from_slice(reg_code);
        code.push((-(offset as i8)) as u8);
    }

    code
}

/// Generate function epilogue following System V ABI
/// - Restore stack pointer
/// - Restore old base pointer
/// - Return to caller
pub fn generate_epilogue() -> Vec<u8> {
    vec![
        0x48, 0x89, 0xec, // mov rsp, rbp
        0x5d, // pop rbp
        0xc3, // ret
    ]
}

/// Generate code to set up function call arguments
/// Pops values from stack and places them in parameter registers
/// Following System V ABI: RDI, RSI, RDX, RCX, R8, R9
pub fn generate_call_setup(arg_count: usize) -> Vec<u8> {
    let arg_regs: Vec<&[u8]> = vec![
        &[0x5f],             // pop rdi
        &[0x5e],             // pop rsi
        &[0x5a],             // pop rdx
        &[0x59],             // pop rcx
        &[0x41, 0x58],       // pop r8
        &[0x41, 0x59],       // pop r9
    ];

    let mut code = Vec::new();

    // Pop arguments in reverse order (last arg first)
    // Stack has args in order: arg0, arg1, arg2, ...
    // We pop in reverse so arg0 goes to RDI, arg1 to RSI, etc.
    for i in (0..arg_count.min(6)).rev() {
        code.extend_from_slice(arg_regs[i]);
    }

    code
}
