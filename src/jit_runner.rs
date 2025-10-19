use crate::codegen::{JitArtifact, RuntimeAddresses, RuntimeRelocation, RuntimeStub};
use memmap2::MmapMut;

pub struct JitRunner;

pub trait JitRunnerTrt {
    fn exec(instructions: &[u8]) -> u8;
}

impl JitRunnerTrt for JitRunner {
    fn exec(instructions: &[u8]) -> u8 {
        let mut m = MmapMut::map_anon(instructions.len()).unwrap();
        m.copy_from_slice(instructions);
        let m = m.make_exec().unwrap();
        let func_ptr = m.as_ptr();

        unsafe {
            let func: extern "C" fn() -> u8 = std::mem::transmute(func_ptr);
            func()
        }
    }
}

impl JitRunner {
    pub fn exec_artifact(artifact: &JitArtifact) -> u8 {
        let mut m = MmapMut::map_anon(artifact.code.len()).unwrap();
        m.copy_from_slice(&artifact.code);

        if !artifact.runtime_relocations.is_empty() {
            let base_ptr = m.as_mut_ptr() as usize;
            apply_runtime_relocations(&artifact.runtime_relocations, &artifact.runtime_stubs, base_ptr, &mut m);
            patch_runtime_stubs(&artifact.runtime_stubs, &artifact.runtime_addresses, &mut m);
        }

        let m = m.make_exec().unwrap();
        let func_ptr = m.as_ptr();

        unsafe {
            let func: extern "C" fn() -> u8 = std::mem::transmute(func_ptr);
            func()
        }
    }
}

fn apply_runtime_relocations(relocations: &[RuntimeRelocation], stubs: &[RuntimeStub], base_ptr: usize, buffer: &mut [u8]) {
    for reloc in relocations {
        let stub_offset = stubs
            .iter()
            .find(|stub| stub.symbol == reloc.symbol)
            .map(|stub| stub.offset)
            .unwrap_or_else(|| panic!("Missing runtime stub for symbol {}", reloc.symbol));
        let target = base_ptr + stub_offset;

        let displacement_site = base_ptr + reloc.offset;
        let next_instruction = displacement_site + 4;

        let relative = target as i64 - next_instruction as i64;
        assert!(relative >= i32::MIN as i64 && relative <= i32::MAX as i64, "Call relocation for {} out of range", reloc.symbol);
        let bytes = (relative as i32).to_le_bytes();

        buffer[reloc.offset..reloc.offset + 4].copy_from_slice(&bytes);
    }
}

fn patch_runtime_stubs(stubs: &[RuntimeStub], addresses: &RuntimeAddresses, buffer: &mut [u8]) {
    for stub in stubs {
        let target = resolve_runtime_symbol(addresses, &stub.symbol).unwrap_or_else(|| panic!("Missing runtime address for symbol {}", stub.symbol));
        let immediate_offset = stub.offset + 2; // skip mov opcode (0x48, 0xb8)
        buffer[immediate_offset..immediate_offset + 8].copy_from_slice(&(target as u64).to_le_bytes());

        // Ensure the tail bytes encode `jmp rax`
        let jump_opcode = &mut buffer[stub.offset + 10..stub.offset + 12];
        if jump_opcode != [0xff, 0xe0] {
            jump_opcode.copy_from_slice(&[0xff, 0xe0]);
        }
    }
}

fn resolve_runtime_symbol(addresses: &RuntimeAddresses, symbol: &str) -> Option<usize> {
    match symbol {
        "_heap_init" => addresses.heap_init,
        "_allocate" => addresses.allocate,
        "_free" => addresses.free,
        "_string_count" => addresses.string_count,
        "_string_concat_n" => addresses.string_concat_n,
        "_string_clone" => addresses.string_clone,
        "_string_get" => addresses.string_get,
        "_string_subs" => addresses.string_subs,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec() {
        let ret_code: u8 = 0x2c;
        let instructions: [u8; 6] = [
            0xb8, ret_code, 0x00, 0x00, 0x00, // mov eax, 42 (0x2a)
            0xc3, // ret
        ];
        let result = JitRunner::exec(&instructions);
        println!("Result: {:#?}", result);

        assert_eq!(result, ret_code);
    }
}
