use std::fs::File;
use std::io::{Result as IoResult, Write};

pub fn generate_elf_executable(machine_code: &[u8], output_path: &str) -> IoResult<()> {
    let mut file = File::create(output_path)?;

    // Create a minimal working ELF executable
    let elf_data = create_minimal_elf(machine_code);
    file.write_all(&elf_data)?;

    Ok(())
}

fn create_minimal_elf(machine_code: &[u8]) -> Vec<u8> {
    let mut elf = Vec::new();

    // ELF Header (64 bytes)
    // e_ident
    elf.extend_from_slice(&[0x7f, 0x45, 0x4c, 0x46]); // ELF magic
    elf.push(2); // EI_CLASS: 64-bit
    elf.push(1); // EI_DATA: little endian
    elf.push(1); // EI_VERSION: current
    elf.push(0); // EI_OSABI: System V
    elf.extend_from_slice(&[0; 8]); // EI_PAD: padding

    // e_type, e_machine, e_version
    elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf.extend_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT

    // Addresses and offsets
    let entry_point = 0x401000u64; // Entry point
    let ph_offset = 64u64; // Program header offset

    elf.extend_from_slice(&entry_point.to_le_bytes()); // e_entry
    elf.extend_from_slice(&ph_offset.to_le_bytes()); // e_phoff
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff (no sections)
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    // Program Header (56 bytes)
    let code_file_offset = 0x1000u64; // Start code at page boundary
    let code_vaddr = 0x401000u64; // Virtual address for code
    let entry_stub_size = 5 + 3 + 7 + 2; // call(5) + mov rdi,rax(3) + mov rax,60(7) + syscall(2) = 17 bytes
    let total_code_size = entry_stub_size + machine_code.len();

    elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
    elf.extend_from_slice(&5u32.to_le_bytes()); // p_flags: PF_X | PF_R
    elf.extend_from_slice(&code_file_offset.to_le_bytes()); // p_offset
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_vaddr
    elf.extend_from_slice(&code_vaddr.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&(total_code_size as u64).to_le_bytes()); // p_filesz
    elf.extend_from_slice(&(total_code_size as u64).to_le_bytes()); // p_memsz
    elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

    // Pad to code offset (should be at 0x1000)
    while elf.len() < code_file_offset as usize {
        elf.push(0);
    }

    // Add entry point stub that calls the first function (assumed to be main)
    // and then exits with its return value
    // The call instruction is 5 bytes, followed by 16 bytes of exit code (3+11+2)
    // So after the call (at offset 5), we need to skip 16 bytes to reach machine code
    // But the call offset is relative to the END of the call instruction (offset 5)
    // Machine code starts at offset 5+16 = 21, so we need: offset 21 - offset 5 = 16... wait
    // Actually: call is at 0, ends at 5. Exit code is 5-21. Machine code starts at 21.
    // So from end of call (5), we want to jump to 21: offset = 21 - 5 = 16. But that's wrong.
    // Let me recalculate: exit code is 16 bytes. call+exit = 21 bytes. Code at offset 21.
    // Call at 0x1000, ends at 0x1005. Want to jump to 0x1000+21 = 0x1015.
    // From 0x1005, offset to 0x1015 is 0x10 (16). But code actually starts at 0x1011?
    // Oh! The entry_stub_size calculation is wrong. It's 5+16 = 21, but exit code is 3+11+2 = 16
    // Wait, let me count: call(5) + mov rdi,rax(3) + mov rax,60(11... no wait)
    // mov rdi,rax = 48 89 c7 = 3 bytes
    // mov rax,60 = 48 c7 c0 3c 00 00 00 = 7 bytes (not 11!)
    // syscall = 0f 05 = 2 bytes
    // Total exit code = 3+7+2 = 12 bytes
    // So call goes from 0x1000-0x1005 (5 bytes), exit code is 0x1005-0x1011 (12 bytes)
    // Code starts at 0x1011. From end of call (0x1005) to code (0x1011) = 12 bytes
    elf.extend_from_slice(&[
        0xe8, 0x0c, 0x00, 0x00, 0x00, // call +12 (skip exit code to reach machine code)
        0x48, 0x89, 0xc7, // mov rdi, rax (3 bytes)
        0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00, // mov rax, 60 (7 bytes)
        0x0f, 0x05, // syscall (2 bytes)
    ]);

    // Add the machine code (functions)
    elf.extend_from_slice(machine_code);

    elf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_elf_generation() {
        // Simple function that returns 42
        let machine_code = vec![
            0x55,                                   // push rbp
            0x48, 0x89, 0xe5,                       // mov rbp, rsp
            0x48, 0xc7, 0xc0, 42, 0x00, 0x00, 0x00, // mov rax, 42
            0x48, 0x89, 0xec,                       // mov rsp, rbp
            0x5d,                                   // pop rbp
            0xc3,                                   // ret
        ];

        let output_path = "/tmp/test_slisp_executable";
        generate_elf_executable(&machine_code, output_path).unwrap();

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
                0x55,             // push rbp
                0x48, 0x89, 0xe5, // mov rbp, rsp
                0x48, 0xc7, 0xc0, test_val, 0x00, 0x00, 0x00, // mov rax, test_val
                0x48, 0x89, 0xec, // mov rsp, rbp
                0x5d,             // pop rbp
                0xc3,             // ret
            ];

            let output_path = format!("/tmp/test_slisp_executable_{}", test_val);
            generate_elf_executable(&machine_code, &output_path).unwrap();

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
}
