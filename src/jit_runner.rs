use crate::codegen::{JitArtifact, RuntimeAddresses, RuntimeRelocation, RuntimeStub};
use memmap2::MmapMut;

pub struct JitRunner;

impl JitRunner {
    #[cfg(test)]
    pub fn exec(instructions: &[u8]) -> u8 {
        let mut m = MmapMut::map_anon(instructions.len()).unwrap();
        m.copy_from_slice(instructions);
        let m = m.make_exec().unwrap();
        let func_ptr = m.as_ptr();

        unsafe {
            let func: extern "C" fn() -> u8 = std::mem::transmute(func_ptr);
            func()
        }
    }

    pub fn exec_artifact(artifact: &JitArtifact) -> u8 {
        let patched_code = if artifact.runtime_relocations.is_empty() && artifact.runtime_stubs.is_empty() {
            artifact.code.clone()
        } else {
            let relocated = apply_runtime_relocations(artifact.code.clone(), &artifact.runtime_relocations, &artifact.runtime_stubs);
            patch_runtime_stubs(relocated, &artifact.runtime_stubs, &artifact.runtime_addresses)
        };

        let mut m = MmapMut::map_anon(patched_code.len()).unwrap();
        m.copy_from_slice(&patched_code);

        let m = m.make_exec().unwrap();
        let func_ptr = m.as_ptr();

        unsafe {
            let func: extern "C" fn() -> u8 = std::mem::transmute(func_ptr);
            func()
        }
    }
}

fn apply_runtime_relocations(code: Vec<u8>, relocations: &[RuntimeRelocation], stubs: &[RuntimeStub]) -> Vec<u8> {
    if relocations.is_empty() {
        return code;
    }

    let mut new_code = code;

    relocations.iter().for_each(|reloc| {
        let stub_offset = stubs
            .iter()
            .find(|stub| stub.symbol == reloc.symbol)
            .map(|stub| stub.offset)
            .unwrap_or_else(|| panic!("Missing runtime stub for symbol {}", reloc.symbol));

        let next_instruction = reloc.offset + 4;
        let relative = stub_offset as i64 - next_instruction as i64;
        assert!(relative >= i32::MIN as i64 && relative <= i32::MAX as i64, "Call relocation for {} out of range", reloc.symbol);
        let bytes = (relative as i32).to_le_bytes();

        new_code[reloc.offset..reloc.offset + 4].copy_from_slice(&bytes);
    });

    new_code
}

fn patch_runtime_stubs(code: Vec<u8>, stubs: &[RuntimeStub], addresses: &RuntimeAddresses) -> Vec<u8> {
    if stubs.is_empty() {
        return code;
    }

    let mut new_code = code;

    stubs.iter().for_each(|stub| {
        let target = resolve_runtime_symbol(addresses, &stub.symbol).unwrap_or_else(|| panic!("Missing runtime address for symbol {}", stub.symbol));
        let immediate_offset = stub.offset + 2; // skip mov opcode (0x48, 0xb8)
        new_code[immediate_offset..immediate_offset + 8].copy_from_slice(&(target as u64).to_le_bytes());

        // Ensure the tail bytes encode `jmp rax`
        let jump_slice = &mut new_code[stub.offset + 10..stub.offset + 12];
        if jump_slice != [0xff, 0xe0] {
            jump_slice.copy_from_slice(&[0xff, 0xe0]);
        }
    });

    new_code
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
        "_string_normalize" => addresses.string_normalize,
        "_string_from_number" => addresses.string_from_number,
        "_string_from_boolean" => addresses.string_from_boolean,
        "_string_equals" => addresses.string_equals,
        "_print_values" => addresses.print_values,
        "_printf_values" => addresses.printf_values,
        "_map_value_clone" => addresses.map_value_clone,
        "_map_free" => addresses.map_free,
        "_set_free" => addresses.set_free,
        "_vector_free" => addresses.vector_free,
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
