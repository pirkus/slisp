use super::codegen::SymbolRelocation;
use crate::ir::IRInstruction;

pub(super) fn compute_local_count(instructions: &[IRInstruction]) -> usize {
    let mut max_slot: Option<usize> = None;
    for inst in instructions {
        let slot_opt = match inst {
            IRInstruction::StoreLocal(slot) | IRInstruction::LoadLocal(slot) | IRInstruction::PushLocalAddress(slot) | IRInstruction::FreeLocal(slot) => Some(*slot),
            _ => None,
        };

        if let Some(slot) = slot_opt {
            max_slot = Some(max_slot.map_or(slot, |current| current.max(slot)));
        }
    }

    max_slot.map_or(0, |slot| slot + 1)
}

fn append_runtime_call(code: &mut Vec<u8>, relocations: &mut Vec<SymbolRelocation>, symbol: &str) {
    let call_site = code.len();
    code.push(0xe8);
    code.extend_from_slice(&0i32.to_le_bytes());
    relocations.push(SymbolRelocation {
        offset: call_site + 1,
        symbol: symbol.to_string(),
    });
}

pub(super) fn generate_entry_stub(entry_symbol: &str, telemetry_enabled: bool) -> (Vec<u8>, Vec<SymbolRelocation>) {
    let mut code = Vec::new();
    let mut relocations = Vec::new();

    if telemetry_enabled {
        append_runtime_call(&mut code, &mut relocations, "_allocator_telemetry_reset");
        code.extend_from_slice(&[0xbf, 0x01, 0x00, 0x00, 0x00]); // mov edi, 1
        append_runtime_call(&mut code, &mut relocations, "_allocator_telemetry_enable");
    }

    append_runtime_call(&mut code, &mut relocations, "_heap_init");
    append_runtime_call(&mut code, &mut relocations, entry_symbol);

    if telemetry_enabled {
        // Preserve exit code in RBX, dump telemetry, and move exit code into RDI.
        code.extend_from_slice(&[0x48, 0x89, 0xc3]); // mov rbx, rax
        append_runtime_call(&mut code, &mut relocations, "_allocator_telemetry_dump_stdout");
        code.extend_from_slice(&[0x48, 0x89, 0xdf]); // mov rdi, rbx
    } else {
        code.extend_from_slice(&[0x48, 0x89, 0xc7]); // mov rdi, rax
    }

    // mov rax, 60  (sys_exit)
    code.extend_from_slice(&[0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00]);

    // syscall
    code.extend_from_slice(&[0x0f, 0x05]);

    (code, relocations)
}
