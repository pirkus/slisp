/// Runtime support functions for heap allocation
/// These functions are embedded in every compiled executable
///
/// Memory layout approach: Use fixed absolute address for heap_ptr
///
/// - heap_ptr lives at 0x403000 (in .data segment)
/// - This is simpler than RIP-relative addressing for MVP
///
/// Fixed address for heap_ptr global variable
pub const HEAP_PTR_ADDRESS: u64 = 0x403000;

/// Generate the heap initialization function
/// Uses mmap syscall to allocate 1MB of memory
/// Stores result in absolute address HEAP_PTR_ADDRESS
pub fn generate_heap_init() -> Vec<u8> {
    let mut code = Vec::new();

    // _heap_init:
    // Save registers we'll use
    code.extend_from_slice(&[0x53]); // push rbx
    code.extend_from_slice(&[0x51]); // push rcx
    code.extend_from_slice(&[0x52]); // push rdx
    code.extend_from_slice(&[0x56]); // push rsi
    code.extend_from_slice(&[0x57]); // push rdi
    code.extend_from_slice(&[0x41, 0x50]); // push r8
    code.extend_from_slice(&[0x41, 0x51]); // push r9
    code.extend_from_slice(&[0x41, 0x52]); // push r10

    // mmap syscall
    // mmap(NULL, 1MB, PROT_READ|PROT_WRITE, MAP_ANONYMOUS|MAP_PRIVATE, -1, 0)
    code.extend_from_slice(&[0x48, 0xc7, 0xc0, 0x09, 0x00, 0x00, 0x00]); // mov rax, 9 (mmap)
    code.extend_from_slice(&[0x48, 0x31, 0xff]); // xor rdi, rdi (addr = NULL)
    code.extend_from_slice(&[0x48, 0xc7, 0xc6, 0x00, 0x00, 0x10, 0x00]); // mov rsi, 1048576 (1MB)
    code.extend_from_slice(&[0x48, 0xc7, 0xc2, 0x03, 0x00, 0x00, 0x00]); // mov rdx, 3 (PROT_READ|PROT_WRITE)
    code.extend_from_slice(&[0x49, 0xc7, 0xc2, 0x22, 0x00, 0x00, 0x00]); // mov r10, 0x22 (MAP_ANONYMOUS|MAP_PRIVATE)
    code.extend_from_slice(&[0x49, 0xc7, 0xc0, 0xff, 0xff, 0xff, 0xff]); // mov r8, -1 (fd)
    code.extend_from_slice(&[0x49, 0x31, 0xc9]); // xor r9, r9 (offset = 0)
    code.extend_from_slice(&[0x0f, 0x05]); // syscall

    // Store result in heap_ptr using absolute addressing
    // movabs rbx, HEAP_PTR_ADDRESS
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, imm64
    code.extend_from_slice(&HEAP_PTR_ADDRESS.to_le_bytes());

    // mov [rbx], rax  ; Store heap base address at heap_ptr
    code.extend_from_slice(&[0x48, 0x89, 0x03]);

    // Restore registers
    code.extend_from_slice(&[0x41, 0x5a]); // pop r10
    code.extend_from_slice(&[0x41, 0x59]); // pop r9
    code.extend_from_slice(&[0x41, 0x58]); // pop r8
    code.extend_from_slice(&[0x5f]); // pop rdi
    code.extend_from_slice(&[0x5e]); // pop rsi
    code.extend_from_slice(&[0x5a]); // pop rdx
    code.extend_from_slice(&[0x59]); // pop rcx
    code.extend_from_slice(&[0x5b]); // pop rbx

    code.extend_from_slice(&[0xc3]); // ret

    code
}

/// Generate the allocate function (bump allocator)
/// Takes size in RDI, returns pointer in RAX
/// Uses absolute addressing to access heap_ptr
pub fn generate_allocate() -> Vec<u8> {
    let mut code = Vec::new();

    // _allocate:
    // movabs rbx, HEAP_PTR_ADDRESS
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, imm64
    code.extend_from_slice(&HEAP_PTR_ADDRESS.to_le_bytes());

    // mov rax, [rbx]  ; Load current heap_ptr value
    code.extend_from_slice(&[0x48, 0x8b, 0x03]);

    // mov rcx, rax    ; Save current pointer in rcx
    code.extend_from_slice(&[0x48, 0x89, 0xc1]);

    // add rax, rdi    ; Add size to heap_ptr
    code.extend_from_slice(&[0x48, 0x01, 0xf8]);

    // mov [rbx], rax  ; Store updated heap_ptr
    code.extend_from_slice(&[0x48, 0x89, 0x03]);

    // mov rax, rcx    ; Return old pointer in rax
    code.extend_from_slice(&[0x48, 0x89, 0xc8]);

    code.extend_from_slice(&[0xc3]); // ret

    code
}
