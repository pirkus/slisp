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
    let total_code_size = machine_code.len() + 12; // code + exit syscall

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

    // Add the machine code
    elf.extend_from_slice(machine_code);

    // Add exit syscall to properly terminate
    elf.extend_from_slice(&[
        0x48, 0x89, 0xc7, // mov rdi, rax (move return value to exit code)
        0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00, // mov rax, 60 (sys_exit)
        0x0f, 0x05, // syscall
    ]);

    elf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_elf_generation() {
        // Simple program that returns 42
        let machine_code = vec![
            0x48, 0xc7, 0xc0, 42, 0x00, 0x00, 0x00, // mov rax, 42
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
                0x48, 0xc7, 0xc0, test_val, 0x00, 0x00, 0x00, // mov rax, test_val
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
