/// x86-64 Linux code generation backend
///
/// This module implements code generation for x86-64 Linux systems:
/// - Machine code generation (System V ABI calling convention)
/// - ELF executable generation (Linux binary format)
/// - Linux syscall conventions
///
/// Submodules:
/// - `abi`: System V ABI implementation (calling convention, stack frames)
/// - `codegen`: Core code generator that lowers IR to machine code
/// - `instructions`: Individual x86-64 instruction generation
/// - `runtime`: Runtime support functions (heap allocation, etc.)
/// - `executable`: ELF executable generation for Linux
mod abi;
mod codegen;
mod instructions;

use self::codegen::{generate_entry_stub, LinkMode, X86CodeGen};
use crate::codegen::backend::{JitArtifact, ObjectArtifact, RuntimeRelocation, RuntimeStub, TargetBackend};
use crate::ir::IRProgram;
use object::write::{Object, Relocation as ObjectRelocation, StandardSection, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, RelocationEncoding, RelocationFlags, RelocationKind, SymbolFlags, SymbolKind, SymbolScope};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[derive(Default)]
pub struct X86_64LinuxBackend;

impl X86_64LinuxBackend {
    pub fn new() -> Self {
        Self
    }
}

impl TargetBackend for X86_64LinuxBackend {
    fn compile_jit(&mut self, program: &IRProgram) -> JitArtifact {
        compile_to_executable(program)
    }

    fn compile_object(&mut self, program: &IRProgram) -> ObjectArtifact {
        compile_to_object(program)
    }
}

/// Public API: Compile IR program to x86-64 machine code
pub fn compile_to_executable(program: &IRProgram) -> JitArtifact {
    let mut codegen = X86CodeGen::new(LinkMode::Jit);
    codegen.set_string_addresses(program);
    let _ = codegen.generate(program);
    let generated = codegen.into_generated_code();

    let mut code = generated.code;
    let mut unique_symbols = Vec::new();
    for reloc in &generated.symbol_relocations {
        if !unique_symbols.contains(&reloc.symbol) {
            unique_symbols.push(reloc.symbol.clone());
        }
    }

    let mut runtime_stubs = Vec::new();
    for symbol in &unique_symbols {
        let stub_offset = code.len();
        // movabs rax, imm64
        code.extend_from_slice(&[0x48, 0xb8]);
        code.extend_from_slice(&0u64.to_le_bytes());
        // jmp rax (tail-call into runtime function)
        code.extend_from_slice(&[0xff, 0xe0]);

        runtime_stubs.push(RuntimeStub {
            symbol: symbol.clone(),
            offset: stub_offset,
        });
    }

    let runtime_relocations = generated
        .symbol_relocations
        .into_iter()
        .map(|reloc| RuntimeRelocation {
            offset: reloc.offset,
            symbol: reloc.symbol,
        })
        .collect();
    JitArtifact {
        code,
        runtime_relocations,
        runtime_addresses: generated.runtime_addresses,
        runtime_stubs,
        _string_buffers: generated.string_buffers,
    }
}

pub fn compile_to_object(program: &IRProgram) -> ObjectArtifact {
    let mut codegen = X86CodeGen::new(LinkMode::ObjFile);
    codegen.set_string_addresses(program);
    let _ = codegen.generate(program);
    let generated = codegen.into_generated_code();

    let entry_symbol_name = program.entry_point.clone().unwrap_or_else(|| "__slisp_main".to_string());

    let entry_offset = generated.function_addresses.get(&entry_symbol_name).copied().unwrap_or(0);

    let (stub_code, mut stub_relocs) = generate_entry_stub(&entry_symbol_name, program.telemetry_enabled);
    let stub_len = stub_code.len();

    let mut text = Vec::new();
    text.extend_from_slice(&stub_code);
    text.extend_from_slice(&generated.code);

    let mut rodata = Vec::new();
    let mut string_offsets = Vec::new();
    for literal in &program.string_literals {
        string_offsets.push(rodata.len());
        rodata.extend_from_slice(literal.as_bytes());
        rodata.push(0);
    }

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
    let text_section = obj.section_id(StandardSection::Text);
    let rodata_section = obj.section_id(StandardSection::ReadOnlyData);

    obj.append_section_data(text_section, &text, 16);
    if !rodata.is_empty() {
        obj.append_section_data(rodata_section, &rodata, 1);
    }

    let mut symbol_map: HashMap<String, object::write::SymbolId> = HashMap::new();

    // _start symbol at beginning of stub
    let start_id = obj.add_symbol(Symbol {
        name: b"_start".to_vec(),
        value: 0,
        size: 0,
        kind: SymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(text_section),
        flags: SymbolFlags::None,
    });
    symbol_map.insert("_start".to_string(), start_id);

    // Entry function symbol
    let entry_id = obj.add_symbol(Symbol {
        name: entry_symbol_name.as_bytes().to_vec(),
        value: (stub_len + entry_offset) as u64,
        size: 0,
        kind: SymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(text_section),
        flags: SymbolFlags::None,
    });
    symbol_map.insert(entry_symbol_name.clone(), entry_id);

    // Additional function symbols
    for (name, offset) in &generated.function_addresses {
        if name == &entry_symbol_name {
            continue;
        }
        let id = obj.add_symbol(Symbol {
            name: name.as_bytes().to_vec(),
            value: (stub_len + offset) as u64,
            size: 0,
            kind: SymbolKind::Text,
            scope: SymbolScope::Linkage,
            weak: false,
            section: SymbolSection::Section(text_section),
            flags: SymbolFlags::None,
        });
        symbol_map.insert(name.clone(), id);
    }

    // External runtime symbols
    let mut runtime_symbols = vec![
        "_heap_init",
        "_allocate",
        "_free",
        "_string_count",
        "_string_concat_n",
        "_string_clone",
        "_string_get",
        "_string_subs",
        "_string_normalize",
        "_string_from_number",
        "_string_from_boolean",
        "_string_equals",
    ];

    if program.telemetry_enabled {
        runtime_symbols.push("_allocator_telemetry_reset");
        runtime_symbols.push("_allocator_telemetry_enable");
        runtime_symbols.push("_allocator_telemetry_dump_stdout");
    }

    for runtime_symbol in runtime_symbols {
        let id = obj.add_symbol(Symbol {
            name: runtime_symbol.as_bytes().to_vec(),
            value: 0,
            size: 0,
            kind: SymbolKind::Text,
            scope: SymbolScope::Dynamic,
            weak: false,
            section: SymbolSection::Undefined,
            flags: SymbolFlags::None,
        });
        symbol_map.insert(runtime_symbol.to_string(), id);
    }

    // String symbols for rodata references
    let mut string_symbol_ids = Vec::new();
    for (index, offset) in string_offsets.iter().enumerate() {
        let name = format!(".Lstr{}", index);
        let id = obj.add_symbol(Symbol {
            name: name.into_bytes(),
            value: *offset as u64,
            size: (program.string_literals[index].len() + 1) as u64,
            kind: SymbolKind::Data,
            scope: SymbolScope::Compilation,
            weak: false,
            section: SymbolSection::Section(rodata_section),
            flags: SymbolFlags::None,
        });
        string_symbol_ids.push(id);
    }

    // Apply relocations for entry stub
    for reloc in stub_relocs.drain(..) {
        if let Some(symbol_id) = symbol_map.get(&reloc.symbol) {
            obj.add_relocation(
                text_section,
                ObjectRelocation {
                    offset: reloc.offset as u64,
                    symbol: *symbol_id,
                    addend: -4,
                    flags: RelocationFlags::Generic {
                        kind: RelocationKind::Relative,
                        encoding: RelocationEncoding::Generic,
                        size: 32,
                    },
                },
            )
            .expect("failed to add relocation");
        }
    }

    // Apply relocations from generated code
    for reloc in generated.symbol_relocations {
        if let Some(symbol_id) = symbol_map.get(&reloc.symbol) {
            obj.add_relocation(
                text_section,
                ObjectRelocation {
                    offset: (reloc.offset + stub_len) as u64,
                    symbol: *symbol_id,
                    addend: -4,
                    flags: RelocationFlags::Generic {
                        kind: RelocationKind::Relative,
                        encoding: RelocationEncoding::Generic,
                        size: 32,
                    },
                },
            )
            .expect("failed to add relocation");
        }
    }

    for reloc in generated.string_relocations {
        if let Some(symbol_id) = string_symbol_ids.get(reloc.index) {
            obj.add_relocation(
                text_section,
                ObjectRelocation {
                    offset: (reloc.offset + stub_len) as u64,
                    symbol: *symbol_id,
                    addend: 0,
                    flags: RelocationFlags::Generic {
                        kind: RelocationKind::Absolute,
                        encoding: RelocationEncoding::Generic,
                        size: 64,
                    },
                },
            )
            .expect("failed to add string relocation");
        }
    }

    let bytes = obj.write().expect("failed to serialize ELF object for slisp program");

    ObjectArtifact { bytes }
}

pub fn link_with_runtime(object_bytes: &[u8], output_path: &str, runtime_staticlib: &str, keep_object: bool) -> io::Result<()> {
    let mut obj_path = PathBuf::from(output_path);
    obj_path.set_extension("o");

    fs::write(&obj_path, object_bytes)?;

    let obj_path_str = obj_path.to_str().ok_or_else(|| io::Error::other("Invalid object path"))?.to_string();

    let status = Command::new("ld").args(["-o", output_path, &obj_path_str, runtime_staticlib, "-static", "-nostdlib"]).status()?;

    if !keep_object {
        let _ = fs::remove_file(&obj_path);
    }

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("ld failed with status: {}", status)))
    }
}
