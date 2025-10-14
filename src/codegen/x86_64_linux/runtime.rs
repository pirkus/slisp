/// Runtime support functions for heap allocation
/// These functions are embedded in every compiled executable
///
/// Memory layout: Free list-based malloc implementation
///
/// Global variables in .data segment:
/// - heap_base (0x403000): Start of heap region from mmap
/// - heap_end (0x403008): End of heap region (base + size)
/// - free_list_head (0x403010): Pointer to first free block
///
/// Free block structure:
/// - [size: 8 bytes][next: 8 bytes][data...]
/// - Minimum block size: 16 bytes (header only)
///
/// Allocated block structure:
/// - [size: 8 bytes][data...]
/// - Size has high bit set to indicate allocated
///
pub const HEAP_BASE_ADDRESS: u64 = 0x403000;
pub const HEAP_END_ADDRESS: u64 = 0x403008;
pub const FREE_LIST_HEAD_ADDRESS: u64 = 0x403010;

/// Marker bit for allocated blocks (high bit of size field)
pub const ALLOCATED_BIT: u64 = 0x8000_0000_0000_0000;

/// Generate the heap initialization function
/// Uses mmap syscall to allocate 1MB of memory
/// Initializes heap_base, heap_end, and free_list_head
/// Sets up initial free block covering entire heap
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

    // rax now contains heap base address
    // Save it to heap_base
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, HEAP_BASE_ADDRESS
    code.extend_from_slice(&HEAP_BASE_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x89, 0x03]); // mov [rbx], rax

    // Calculate heap_end = heap_base + 1MB
    code.extend_from_slice(&[0x48, 0x89, 0xc1]); // mov rcx, rax
    code.extend_from_slice(&[0x48, 0x81, 0xc1, 0x00, 0x00, 0x10, 0x00]); // add rcx, 1048576

    // Store heap_end
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, HEAP_END_ADDRESS
    code.extend_from_slice(&HEAP_END_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x89, 0x0b]); // mov [rbx], rcx

    // Initialize first free block at heap_base
    // rax = heap_base, rcx = heap_end
    // Free block header: [size][next]
    // size = heap_end - heap_base - 8 (for size field)
    code.extend_from_slice(&[0x48, 0x89, 0xca]); // mov rdx, rcx
    code.extend_from_slice(&[0x48, 0x29, 0xc2]); // sub rdx, rax
    code.extend_from_slice(&[0x48, 0x83, 0xea, 0x08]); // sub rdx, 8

    // Write size to heap_base[0]
    code.extend_from_slice(&[0x48, 0x89, 0x10]); // mov [rax], rdx

    // Write next=NULL to heap_base[8]
    code.extend_from_slice(&[0x48, 0xc7, 0x40, 0x08, 0x00, 0x00, 0x00, 0x00]); // mov qword [rax+8], 0

    // Set free_list_head = heap_base
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, FREE_LIST_HEAD_ADDRESS
    code.extend_from_slice(&FREE_LIST_HEAD_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x89, 0x03]); // mov [rbx], rax

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

/// Generate the free function
/// Takes pointer in RDI (pointer to data, not block header)
/// Returns nothing
///
/// Algorithm:
/// 1. Get block header (ptr - 8)
/// 2. Clear allocated bit from size
/// 3. Insert block at head of free list
/// 4. TODO: Coalesce adjacent free blocks (future optimization)
pub fn generate_free() -> Vec<u8> {
    let mut code = Vec::new();

    // _free:
    // Save callee-saved registers
    code.extend_from_slice(&[0x53]); // push rbx
    code.extend_from_slice(&[0x55]); // push rbp

    // Check if pointer is NULL
    code.extend_from_slice(&[0x48, 0x85, 0xff]); // test rdi, rdi
    code.extend_from_slice(&[0x74, 0x00]); // je .return (offset filled later)
    let return_jump_offset = code.len() - 1;

    // Get block header: rbp = rdi - 8
    code.extend_from_slice(&[0x48, 0x89, 0xfd]); // mov rbp, rdi
    code.extend_from_slice(&[0x48, 0x83, 0xed, 0x08]); // sub rbp, 8

    // Clear allocated bit: [rbp] &= ~ALLOCATED_BIT
    code.extend_from_slice(&[0x48, 0x8b, 0x45, 0x00]); // mov rax, [rbp]
    code.extend_from_slice(&[0x48, 0xb9]); // movabs rcx, ~ALLOCATED_BIT
    code.extend_from_slice(&(!ALLOCATED_BIT).to_le_bytes());
    code.extend_from_slice(&[0x48, 0x21, 0xc8]); // and rax, rcx
    code.extend_from_slice(&[0x48, 0x89, 0x45, 0x00]); // mov [rbp], rax

    // Load current free_list_head into rbx
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, FREE_LIST_HEAD_ADDRESS
    code.extend_from_slice(&FREE_LIST_HEAD_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x8b, 0x03]); // mov rax, [rbx]

    // Set this block's next = old free_list_head
    code.extend_from_slice(&[0x48, 0x89, 0x45, 0x08]); // mov [rbp+8], rax

    // Update free_list_head = this block
    code.extend_from_slice(&[0x48, 0x89, 0x2b]); // mov [rbx], rbp

    // .return:
    let return_label = code.len();
    code[return_jump_offset] = (return_label - return_jump_offset - 1) as u8;

    // Restore registers
    code.extend_from_slice(&[0x5d]); // pop rbp
    code.extend_from_slice(&[0x5b]); // pop rbx

    code.extend_from_slice(&[0xc3]); // ret

    code
}

/// Generate the malloc function (first-fit free list allocator)
/// Takes size in RDI, returns pointer in RAX
/// Returns NULL (0) if no suitable block found
///
/// Algorithm:
/// 1. Round up size to 8-byte alignment
/// 2. Search free list for block >= size + 8 (need room for size header)
/// 3. If found, remove from free list, mark as allocated, return data pointer
/// 4. If not found, return NULL
pub fn generate_allocate() -> Vec<u8> {
    let mut code = Vec::new();

    // _allocate:
    // Save callee-saved registers
    code.extend_from_slice(&[0x53]); // push rbx
    code.extend_from_slice(&[0x55]); // push rbp
    code.extend_from_slice(&[0x41, 0x54]); // push r12
    code.extend_from_slice(&[0x41, 0x55]); // push r13

    // Round up size to 8-byte alignment
    // size = (size + 7) & ~7
    code.extend_from_slice(&[0x48, 0x83, 0xc7, 0x07]); // add rdi, 7
    code.extend_from_slice(&[0x48, 0x83, 0xe7, 0xf8]); // and rdi, ~7
                                                       // rdi now has aligned size

    // Load free_list_head
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, FREE_LIST_HEAD_ADDRESS
    code.extend_from_slice(&FREE_LIST_HEAD_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x4c, 0x8b, 0x23]); // mov r12, [rbx] ; r12 = current block
    code.extend_from_slice(&[0x48, 0x31, 0xed]); // xor rbp, rbp ; rbp = previous block (NULL initially)

    // Loop through free list
    // .search_loop:
    let search_loop_start = code.len();

    // Check if current block is NULL
    code.extend_from_slice(&[0x4d, 0x85, 0xe4]); // test r12, r12
    code.extend_from_slice(&[0x74, 0x00]); // je .not_found (offset filled later)
    let not_found_jump_offset = code.len() - 1;

    // Load block size: rcx = [r12]
    code.extend_from_slice(&[0x49, 0x8b, 0x0c, 0x24]); // mov rcx, [r12]

    // Check if size >= requested_size + 8
    code.extend_from_slice(&[0x48, 0x8d, 0x47, 0x08]); // lea rax, [rdi+8]
    code.extend_from_slice(&[0x48, 0x39, 0xc1]); // cmp rcx, rax
    code.extend_from_slice(&[0x73, 0x00]); // jae .found (offset filled later)
    let found_jump_offset = code.len() - 1;

    // Block too small, try next
    // rbp = r12 (save previous)
    code.extend_from_slice(&[0x4c, 0x89, 0xe5]); // mov rbp, r12
                                                 // r12 = [r12+8] (load next pointer)
    code.extend_from_slice(&[0x4d, 0x8b, 0x64, 0x24, 0x08]); // mov r12, [r12+8]
                                                             // Jump back to search_loop
    let back_jump_offset = -(((code.len() + 2) - search_loop_start) as i8);
    code.extend_from_slice(&[0xeb, back_jump_offset as u8]); // jmp .search_loop

    // .found:
    let found_label = code.len();
    code[found_jump_offset] = (found_label - found_jump_offset - 1) as u8;

    // Remove block from free list
    // Load next pointer: r13 = [r12+8]
    code.extend_from_slice(&[0x4d, 0x8b, 0x6c, 0x24, 0x08]); // mov r13, [r12+8]

    // If rbp == NULL, update free_list_head
    code.extend_from_slice(&[0x48, 0x85, 0xed]); // test rbp, rbp
    code.extend_from_slice(&[0x75, 0x00]); // jne .update_prev (offset filled later)
    let update_prev_jump_offset = code.len() - 1;

    // Update head: free_list_head = r13
    code.extend_from_slice(&[0x48, 0xbb]); // movabs rbx, FREE_LIST_HEAD_ADDRESS
    code.extend_from_slice(&FREE_LIST_HEAD_ADDRESS.to_le_bytes());
    code.extend_from_slice(&[0x4c, 0x89, 0x2b]); // mov [rbx], r13
    code.extend_from_slice(&[0xeb, 0x00]); // jmp .mark_allocated (offset filled later)
    let mark_allocated_jump1_offset = code.len() - 1;

    // .update_prev:
    let update_prev_label = code.len();
    code[update_prev_jump_offset] = (update_prev_label - update_prev_jump_offset - 1) as u8;

    // prev->next = r13
    code.extend_from_slice(&[0x4c, 0x89, 0x6d, 0x08]); // mov [rbp+8], r13

    // .mark_allocated:
    let mark_allocated_label = code.len();
    code[mark_allocated_jump1_offset] =
        (mark_allocated_label - mark_allocated_jump1_offset - 1) as u8;

    // Mark block as allocated: [r12] |= ALLOCATED_BIT
    code.extend_from_slice(&[0x49, 0x8b, 0x04, 0x24]); // mov rax, [r12]
    code.extend_from_slice(&[0x48, 0xb9]); // movabs rcx, ALLOCATED_BIT
    code.extend_from_slice(&ALLOCATED_BIT.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x09, 0xc8]); // or rax, rcx
    code.extend_from_slice(&[0x49, 0x89, 0x04, 0x24]); // mov [r12], rax

    // Return data pointer: rax = r12 + 8
    code.extend_from_slice(&[0x4c, 0x89, 0xe0]); // mov rax, r12
    code.extend_from_slice(&[0x48, 0x83, 0xc0, 0x08]); // add rax, 8
    code.extend_from_slice(&[0xeb, 0x00]); // jmp .return (offset filled later)
    let return_jump_offset = code.len() - 1;

    // .not_found:
    let not_found_label = code.len();
    code[not_found_jump_offset] = (not_found_label - not_found_jump_offset - 1) as u8;

    // Return NULL
    code.extend_from_slice(&[0x48, 0x31, 0xc0]); // xor rax, rax

    // .return:
    let return_label = code.len();
    code[return_jump_offset] = (return_label - return_jump_offset - 1) as u8;

    // Restore registers
    code.extend_from_slice(&[0x41, 0x5d]); // pop r13
    code.extend_from_slice(&[0x41, 0x5c]); // pop r12
    code.extend_from_slice(&[0x5d]); // pop rbp
    code.extend_from_slice(&[0x5b]); // pop rbx

    code.extend_from_slice(&[0xc3]); // ret

    code
}
