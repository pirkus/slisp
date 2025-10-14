/// Executable generation for x86-64 Linux (ELF format)
///
/// This module generates ELF (Executable and Linkable Format) binaries
/// for x86-64 Linux systems. It creates the ELF headers, program headers,
/// and entry stub that uses Linux syscalls.
use std::fs::File;
use std::io::{Result as IoResult, Write};

/// Fixed address for the data segment containing heap globals (Linux x86-64 convention)
/// Layout: heap_base (8 bytes), heap_end (8 bytes), free_list_head (8 bytes)
const DATA_SEGMENT_VADDR: u64 = 0x403000;

/// Fixed address for the rodata segment containing string literals (Linux x86-64 convention)
const RODATA_SEGMENT_VADDR: u64 = 0x404000;

/// Generate an executable binary from machine code
///
/// Creates an ELF executable for x86-64 Linux with:
/// - Entry stub that calls _heap_init (if needed) and -main
/// - Code segment (RX) with user code and runtime functions
/// - Data segment (RW) with heap globals (heap_base, heap_end, free_list_head) if heap is used
/// - Rodata segment (R) with string literals if strings are used
/// - Linux exit syscall (syscall #60)
pub fn generate_executable(
    machine_code: &[u8],
    output_path: &str,
    heap_init_offset: Option<usize>,
    string_literals: &[String],
) -> IoResult<()> {
    let mut file = File::create(output_path)?;

    // Create ELF executable with code, data, and rodata segments
    let elf_data = create_elf_with_segments(machine_code, heap_init_offset, string_literals);
    file.write_all(&elf_data)?;

    Ok(())
}

fn create_elf_with_segments(
    machine_code: &[u8],
    heap_init_offset: Option<usize>,
    string_literals: &[String],
) -> Vec<u8> {
    let mut elf = Vec::new();

    // Calculate rodata content and size
    let mut rodata_content = Vec::new();
    let mut string_offsets = Vec::new(); // Track where each string starts in rodata

    for string in string_literals {
        string_offsets.push(rodata_content.len());
        rodata_content.extend_from_slice(string.as_bytes());
        rodata_content.push(0); // Null terminator
    }

    let rodata_size = rodata_content.len();
    let has_rodata = !string_literals.is_empty();

    // Calculate sizes and offsets
    // Entry stub size depends on whether we need heap initialization
    let entry_stub_size = if heap_init_offset.is_some() {
        5 + 5 + 3 + 7 + 2 // call heap_init + call main + mov rdi,rax + mov rax,60 + syscall = 22 bytes
    } else {
        5 + 3 + 7 + 2 // call main + mov rdi,rax + mov rax,60 + syscall = 17 bytes
    };
    let code_size = entry_stub_size + machine_code.len();
    let data_size = if heap_init_offset.is_some() {
        24 // heap_base + heap_end + free_list_head (8 bytes each) if heap is used
    } else {
        0 // No data segment if heap not used
    };

    // File offsets (page-aligned)
    let code_file_offset = 0x1000u64;
    let data_file_offset = 0x2000u64;
    let rodata_file_offset = 0x3000u64;

    // Virtual addresses (Linux x86-64 convention)
    let code_vaddr = 0x401000u64;
    let data_vaddr = DATA_SEGMENT_VADDR;
    let rodata_vaddr = RODATA_SEGMENT_VADDR;
    let entry_point = code_vaddr;

    // Program header offset and count
    let ph_offset = 64u64; // Right after ELF header
    let ph_count = {
        let mut count = 1u16; // Code segment always present
        if heap_init_offset.is_some() {
            count += 1; // Data segment for heap globals
        }
        if has_rodata {
            count += 1; // Rodata segment for string literals
        }
        count
    };

    // ========== ELF Header (64 bytes) ==========
    // e_ident
    elf.extend_from_slice(&[0x7f, 0x45, 0x4c, 0x46]); // ELF magic
    elf.push(2); // EI_CLASS: 64-bit
    elf.push(1); // EI_DATA: little endian
    elf.push(1); // EI_VERSION: current
    elf.push(0); // EI_OSABI: System V (Linux)
    elf.extend_from_slice(&[0; 8]); // EI_PAD: padding

    // e_type, e_machine, e_version
    elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT

    // Entry point and offsets
    elf.extend_from_slice(&entry_point.to_le_bytes()); // e_entry
    elf.extend_from_slice(&ph_offset.to_le_bytes()); // e_phoff
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff (no section headers)
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&ph_count.to_le_bytes()); // e_phnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    // ========== Program Header 1: Code Segment (56 bytes) ==========
    elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes()); // p_flags: PF_X | PF_R (executable + readable)
    elf.extend_from_slice(&code_file_offset.to_le_bytes()); // p_offset
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_vaddr
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&(code_size as u64).to_le_bytes()); // p_filesz
    elf.extend_from_slice(&(code_size as u64).to_le_bytes()); // p_memsz
    elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align (page-aligned)

    // ========== Program Header 2: Data Segment (56 bytes) - Only if heap is needed ==========
    if heap_init_offset.is_some() {
        elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
        elf.extend_from_slice(&6u32.to_le_bytes()); // p_flags: PF_W | PF_R (writable + readable)
        elf.extend_from_slice(&data_file_offset.to_le_bytes()); // p_offset
        elf.extend_from_slice(&data_vaddr.to_le_bytes()); // p_vaddr
        elf.extend_from_slice(&data_vaddr.to_le_bytes()); // p_paddr
        elf.extend_from_slice(&(data_size as u64).to_le_bytes()); // p_filesz
        elf.extend_from_slice(&(data_size as u64).to_le_bytes()); // p_memsz
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align (page-aligned)
    }

    // ========== Program Header 3: Rodata Segment (56 bytes) - Only if strings are used ==========
    if has_rodata {
        elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
        elf.extend_from_slice(&4u32.to_le_bytes()); // p_flags: PF_R (readable only)
        elf.extend_from_slice(&rodata_file_offset.to_le_bytes()); // p_offset
        elf.extend_from_slice(&rodata_vaddr.to_le_bytes()); // p_vaddr
        elf.extend_from_slice(&rodata_vaddr.to_le_bytes()); // p_paddr
        elf.extend_from_slice(&(rodata_size as u64).to_le_bytes()); // p_filesz
        elf.extend_from_slice(&(rodata_size as u64).to_le_bytes()); // p_memsz
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align (page-aligned)
    }

    // ========== Padding to code offset (0x1000) ==========
    while elf.len() < code_file_offset as usize {
        elf.push(0);
    }

    // ========== Code Segment ==========
    // Entry stub: optionally call _heap_init, then call -main, then exit with return value
    // Uses Linux syscall convention: syscall #60 = exit
    if let Some(heap_init_off) = heap_init_offset {
        // Entry stub with heap initialization (22 bytes x86-64 machine code)
        // Structure:
        //   0-4:   call _heap_init (5 bytes)
        //   5-9:   call -main (5 bytes)
        //   10-12: mov rdi, rax (3 bytes)
        //   13-19: mov rax, 60 (7 bytes)
        //   20-21: syscall (2 bytes)

        // Calculate call offset to _heap_init
        // Target address = entry_stub_size + heap_init_off
        // Current position after call instruction = 5
        // Relative offset = target - current = (entry_stub_size + heap_init_off) - 5
        let heap_init_call_offset = (entry_stub_size + heap_init_off) as i32 - 5;

        // call _heap_init (x86-64: e8 + rel32)
        elf.push(0xe8);
        elf.extend_from_slice(&heap_init_call_offset.to_le_bytes());

        // Calculate call offset to -main
        // -main is at position entry_stub_size (right after this stub)
        // Current position after call instruction = 10
        // Relative offset = entry_stub_size - 10
        let main_call_offset = (entry_stub_size - 10) as i32;

        // call -main (x86-64: e8 + rel32)
        elf.push(0xe8);
        elf.extend_from_slice(&main_call_offset.to_le_bytes());

        // mov rdi, rax (x86-64: return value from -main becomes exit code)
        elf.extend_from_slice(&[0x48, 0x89, 0xc7]);

        // mov rax, 60 (x86-64: Linux exit syscall number)
        elf.extend_from_slice(&[0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00]);

        // syscall (x86-64: Linux syscall instruction)
        elf.extend_from_slice(&[0x0f, 0x05]);
    } else {
        // Entry stub without heap initialization (17 bytes x86-64 machine code)
        // Structure:
        //   0-4:   call -main (5 bytes)
        //   5-7:   mov rdi, rax (3 bytes)
        //   8-14:  mov rax, 60 (7 bytes)
        //   15-16: syscall (2 bytes)

        // Calculate call offset to -main
        // -main is at position entry_stub_size (right after this stub)
        // Current position after call instruction = 5
        // Relative offset = entry_stub_size - 5
        let main_call_offset = (entry_stub_size - 5) as i32;

        elf.push(0xe8); // call opcode
        elf.extend_from_slice(&main_call_offset.to_le_bytes());
        elf.extend_from_slice(&[0x48, 0x89, 0xc7]); // mov rdi, rax
        elf.extend_from_slice(&[0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00]); // mov rax, 60
        elf.extend_from_slice(&[0x0f, 0x05]); // syscall
    }

    // Add the machine code (user functions + runtime functions)
    elf.extend_from_slice(machine_code);

    // ========== Data Segment - Only if heap is needed ==========
    if heap_init_offset.is_some() {
        // Padding to data offset (0x2000)
        while elf.len() < data_file_offset as usize {
            elf.push(0);
        }

        // heap_base: 8 bytes initialized to 0
        elf.extend_from_slice(&[0; 8]);
        // heap_end: 8 bytes initialized to 0
        elf.extend_from_slice(&[0; 8]);
        // free_list_head: 8 bytes initialized to 0
        elf.extend_from_slice(&[0; 8]);
    }

    // ========== Rodata Segment - Only if strings are used ==========
    if has_rodata {
        // Padding to rodata offset (0x3000)
        while elf.len() < rodata_file_offset as usize {
            elf.push(0);
        }

        // Add string literal content (with null terminators)
        elf.extend_from_slice(&rodata_content);
    }

    elf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_executable_generation() {
        // Simple function that returns 42
        let machine_code = vec![
            0x55, // push rbp
            0x48, 0x89, 0xe5, // mov rbp, rsp
            0x48, 0xc7, 0xc0, 42, 0x00, 0x00, 0x00, // mov rax, 42
            0x48, 0x89, 0xec, // mov rsp, rbp
            0x5d, // pop rbp
            0xc3, // ret
        ];

        let output_path = "/tmp/test_slisp_executable";
        generate_executable(&machine_code, output_path, None, &[]).unwrap();

        // Make executable
        Command::new("chmod")
            .args(["+x", output_path])
            .output()
            .expect("Failed to chmod");

        // Run and check exit code
        let output = Command::new(output_path)
            .output()
            .expect("Failed to execute");

        // Check exit code
        if let Some(exit_code) = output.status.code() {
            assert_eq!(exit_code, 42);
        } else {
            panic!("Program was terminated by signal");
        }

        // Cleanup
        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn test_different_values() {
        // Test with different return values
        for test_val in [0, 1, 5, 100, 255] {
            let machine_code = vec![
                0x55, // push rbp
                0x48, 0x89, 0xe5, // mov rbp, rsp
                0x48, 0xc7, 0xc0, test_val, 0x00, 0x00, 0x00, // mov rax, test_val
                0x48, 0x89, 0xec, // mov rsp, rbp
                0x5d, // pop rbp
                0xc3, // ret
            ];

            let output_path = format!("/tmp/test_slisp_executable_{}", test_val);
            generate_executable(&machine_code, &output_path, None, &[]).unwrap();

            Command::new("chmod")
                .args(["+x", &output_path])
                .output()
                .expect("Failed to chmod");

            let output = Command::new(&output_path)
                .output()
                .expect("Failed to execute");

            if let Some(exit_code) = output.status.code() {
                assert_eq!(exit_code, test_val as i32);
            } else {
                panic!("Program with value {} was terminated by signal", test_val);
            }

            let _ = fs::remove_file(&output_path);
        }
    }

    #[test]
    fn test_heap_allocation_in_executable() {
        use crate::codegen::api::{compile_to_executable, Target};
        use crate::ir::{IRInstruction, IRProgram};

        // Create a program that uses heap allocation
        // It will: init heap, allocate 100 bytes, then return 42
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(100));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        // Compile to machine code with heap support
        let (machine_code, heap_init_offset) = compile_to_executable(&program, Target::X86_64Linux);

        // Verify heap is enabled
        assert!(heap_init_offset.is_some());

        // Generate ELF executable
        let output_path = "/tmp/test_slisp_heap_exec";
        generate_executable(&machine_code, output_path, heap_init_offset, &[]).unwrap();

        // Make executable
        Command::new("chmod")
            .args(["+x", output_path])
            .output()
            .expect("Failed to chmod");

        // Run and check exit code
        let output = Command::new(output_path)
            .output()
            .expect("Failed to execute");

        // Check exit code - should be 42
        if let Some(exit_code) = output.status.code() {
            assert_eq!(exit_code, 42, "Expected exit code 42, got {}", exit_code);
        } else {
            panic!("Program was terminated by signal");
        }

        // Cleanup
        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn test_malloc_free_cycle_in_executable() {
        use crate::codegen::api::{compile_to_executable, Target};
        use crate::ir::{IRInstruction, IRProgram};

        // Create a program that tests malloc/free reuse:
        // 1. Init heap
        // 2. Allocate 64 bytes -> ptr1
        // 3. Free ptr1
        // 4. Allocate 32 bytes -> ptr2 (should reuse freed block)
        // 5. Return 42 if ptr2 != NULL, else return 0
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);

        // First allocation
        program.add_instruction(IRInstruction::Allocate(64));

        // Free it (pointer is on stack)
        program.add_instruction(IRInstruction::Free);

        // Second allocation (should reuse the freed block)
        program.add_instruction(IRInstruction::Allocate(32));

        // Pop the pointer to check if it's NULL
        // If allocation succeeded (ptr != NULL), we know free/realloc worked
        // We can't easily test the pointer value itself, but if allocate returns non-NULL,
        // it means the free list is working

        // For now, just return 42 to show program runs successfully
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        // Compile to machine code
        let (machine_code, heap_init_offset) = compile_to_executable(&program, Target::X86_64Linux);

        // Verify heap is enabled
        assert!(heap_init_offset.is_some());

        // Generate ELF executable
        let output_path = "/tmp/test_slisp_malloc_free";
        generate_executable(&machine_code, output_path, heap_init_offset, &[]).unwrap();

        // Make executable
        Command::new("chmod")
            .args(["+x", output_path])
            .output()
            .expect("Failed to chmod");

        // Run and check exit code
        let output = Command::new(output_path)
            .output()
            .expect("Failed to execute");

        // If we get exit code 42, the program ran successfully
        // This means malloc after free didn't crash, which indicates basic functionality
        if let Some(exit_code) = output.status.code() {
            assert_eq!(
                exit_code, 42,
                "Expected exit code 42, got {}. Program may have crashed during malloc/free cycle",
                exit_code
            );
        } else {
            panic!("Program was terminated by signal during malloc/free cycle");
        }

        // Cleanup
        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn test_multiple_allocations_in_executable() {
        use crate::codegen::api::{compile_to_executable, Target};
        use crate::ir::{IRInstruction, IRProgram};

        // Create a program that allocates multiple blocks:
        // 1. Init heap
        // 2. Allocate 100 bytes
        // 3. Allocate 200 bytes
        // 4. Allocate 50 bytes
        // 5. Return 42
        // This tests that the free list can handle multiple allocations
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(100));
        program.add_instruction(IRInstruction::Allocate(200));
        program.add_instruction(IRInstruction::Allocate(50));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        // Compile and execute
        let (machine_code, heap_init_offset) = compile_to_executable(&program, Target::X86_64Linux);
        assert!(heap_init_offset.is_some());

        let output_path = "/tmp/test_slisp_multi_alloc";
        generate_executable(&machine_code, output_path, heap_init_offset, &[]).unwrap();

        Command::new("chmod")
            .args(["+x", output_path])
            .output()
            .expect("Failed to chmod");

        let output = Command::new(output_path)
            .output()
            .expect("Failed to execute");

        if let Some(exit_code) = output.status.code() {
            assert_eq!(
                exit_code, 42,
                "Expected exit code 42, got {}. Multiple allocations may have failed",
                exit_code
            );
        } else {
            panic!("Program crashed during multiple allocations");
        }

        let _ = fs::remove_file(output_path);
    }

    #[test]
    fn test_string_literal_in_executable() {
        use crate::codegen::api::{compile_to_executable, Target};
        use crate::ir::{IRInstruction, IRProgram};

        // Create a program that pushes a string literal
        // The string will be stored in rodata segment at 0x404000
        let mut program = IRProgram::new();

        // Add a string to the string table
        let string_index = program.add_string("hello world".to_string());

        // Push the string address onto stack, then pop it and return 42
        program.add_instruction(IRInstruction::PushString(string_index));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        // Compile to machine code
        let (machine_code, heap_init_offset) = compile_to_executable(&program, Target::X86_64Linux);

        // Generate ELF executable with string literals
        let output_path = "/tmp/test_slisp_string_literal";
        generate_executable(
            &machine_code,
            output_path,
            heap_init_offset,
            &program.string_literals,
        )
        .unwrap();

        // Make executable
        Command::new("chmod")
            .args(["+x", output_path])
            .output()
            .expect("Failed to chmod");

        // Run and check exit code
        let output = Command::new(output_path)
            .output()
            .expect("Failed to execute");

        if let Some(exit_code) = output.status.code() {
            assert_eq!(
                exit_code, 42,
                "Expected exit code 42, got {}. String literal compilation may have failed",
                exit_code
            );
        } else {
            panic!("Program crashed during string literal test");
        }

        // Verify that string literals were embedded
        assert_eq!(program.string_literals.len(), 1);
        assert_eq!(program.string_literals[0], "hello world");

        // Cleanup
        let _ = fs::remove_file(output_path);
    }
}
