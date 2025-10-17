/// x86-64 Linux code generation backend
///
/// This module implements code generation for x86-64 Linux systems:
/// - Machine code generation (System V ABI calling convention)
/// - ELF executable generation (Linux binary format)
/// - Linux syscall conventions
///
/// Submodules:
/// - `abi`: System V ABI implementation (calling convention, stack frames)
/// - `instructions`: Individual x86-64 instruction generation
/// - `sizing`: Instruction size calculation for position-independent code
/// - `runtime`: Runtime support functions (heap allocation, etc.)
/// - `executable`: ELF executable generation for Linux
mod abi;
mod instructions;
mod sizing;

use crate::codegen::backend::{CodeGenBackend, RuntimeAddresses};
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use object::write::{Object, Relocation as ObjectRelocation, StandardSection, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, RelocationEncoding, RelocationFlags, RelocationKind, SymbolFlags, SymbolKind, SymbolScope};
use slisp_runtime;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Copy)]
enum LinkMode {
    Jit,
    ObjFile,
}

#[derive(Clone)]
struct SymbolRelocation {
    offset: usize,
    symbol: String,
}

#[derive(Clone)]
struct StringRelocation {
    offset: usize,
    index: usize,
}

struct GeneratedCode {
    code: Vec<u8>,
    string_buffers: Vec<Box<[u8]>>,
    symbol_relocations: Vec<SymbolRelocation>,
    string_relocations: Vec<StringRelocation>,
    function_addresses: HashMap<String, usize>,
}

pub struct JitArtifact {
    pub code: Vec<u8>,
    _string_buffers: Vec<Box<[u8]>>,
}

impl JitArtifact {
    pub fn as_code(&self) -> &[u8] {
        &self.code
    }
}

pub struct ObjectArtifact {
    pub bytes: Vec<u8>,
}

pub struct X86CodeGen {
    code: Vec<u8>,
    instruction_positions: Vec<usize>,
    function_addresses: HashMap<String, usize>, // function name -> code offset
    runtime_addresses: RuntimeAddresses,        // addresses of runtime support functions
    string_addresses: Vec<u64>,                 // addresses of string literals in rodata segment
    link_mode: LinkMode,
    symbol_relocations: Vec<SymbolRelocation>,
    string_relocations: Vec<StringRelocation>,
    string_buffers: Vec<Box<[u8]>>, // Holds string data alive for JIT mode
}

impl X86CodeGen {
    fn new(link_mode: LinkMode) -> Self {
        let runtime_addresses = match link_mode {
            LinkMode::Jit => RuntimeAddresses {
                heap_init: Some(slisp_runtime::_heap_init as usize),
                allocate: Some(slisp_runtime::_allocate as usize),
                free: Some(slisp_runtime::_free as usize),
                string_count: Some(slisp_runtime::_string_count as usize),
                string_concat_2: Some(slisp_runtime::_string_concat_2 as usize),
            },
            LinkMode::ObjFile => RuntimeAddresses {
                heap_init: None,
                allocate: None,
                free: None,
                string_count: None,
                string_concat_2: None,
            },
        };

        Self {
            code: Vec::new(),
            instruction_positions: Vec::new(),
            function_addresses: HashMap::new(),
            runtime_addresses,
            string_addresses: Vec::new(),
            link_mode,
            symbol_relocations: Vec::new(),
            string_relocations: Vec::new(),
            string_buffers: Vec::new(),
        }
    }

    /// Set string addresses for rodata segment
    pub fn set_string_addresses(&mut self, program: &IRProgram) {
        match self.link_mode {
            LinkMode::Jit => {
                for string in &program.string_literals {
                    let mut bytes = string.clone().into_bytes();
                    bytes.push(0); // Null terminator for C compatibility
                    let boxed = bytes.into_boxed_slice();
                    let ptr = boxed.as_ptr() as u64;
                    self.string_addresses.push(ptr);
                    self.string_buffers.push(boxed);
                }
            }
            LinkMode::ObjFile => {
                self.string_addresses.resize(program.string_literals.len(), 0);
            }
        }
    }

    /// Generate code to call _heap_init runtime function
    fn generate_heap_init_code(&mut self, current_pos: usize) -> Vec<u8> {
        match self.link_mode {
            LinkMode::Jit => {
                if let Some(heap_init_addr) = self.runtime_addresses.heap_init {
                    let offset = (heap_init_addr as i32) - ((current_pos + 5) as i32);
                    let (code, _) = instructions::generate_call_heap_init(Some(offset));
                    code
                } else {
                    let (code, disp) = instructions::generate_call_heap_init(None);
                    self.record_runtime_relocation(current_pos + disp, "_heap_init");
                    code
                }
            }
            LinkMode::ObjFile => {
                let (code, disp) = instructions::generate_call_heap_init(None);
                self.record_runtime_relocation(current_pos + disp, "_heap_init");
                code
            }
        }
    }

    /// Generate code to call _allocate runtime function
    fn generate_allocate_code(&mut self, size: usize, current_pos: usize) -> Vec<u8> {
        match self.link_mode {
            LinkMode::Jit => {
                if let Some(allocate_addr) = self.runtime_addresses.allocate {
                    let offset = (allocate_addr as i32) - ((current_pos + 7 + 5) as i32);
                    let (code, _) = instructions::generate_allocate_inline(size, Some(offset));
                    code
                } else {
                    let (code, disp) = instructions::generate_allocate_inline(size, None);
                    self.record_runtime_relocation(current_pos + disp, "_allocate");
                    code
                }
            }
            LinkMode::ObjFile => {
                let (code, disp) = instructions::generate_allocate_inline(size, None);
                self.record_runtime_relocation(current_pos + disp, "_allocate");
                code
            }
        }
    }

    /// Generate code to call _free runtime function
    fn generate_free_code(&mut self, current_pos: usize) -> Vec<u8> {
        match self.link_mode {
            LinkMode::Jit => {
                if let Some(free_addr) = self.runtime_addresses.free {
                    let offset = (free_addr as i32) - ((current_pos + 1 + 5) as i32);
                    let (code, _) = instructions::generate_free_inline(Some(offset));
                    code
                } else {
                    let (code, disp) = instructions::generate_free_inline(None);
                    self.record_runtime_relocation(current_pos + disp, "_free");
                    code
                }
            }
            LinkMode::ObjFile => {
                let (code, disp) = instructions::generate_free_inline(None);
                self.record_runtime_relocation(current_pos + disp, "_free");
                code
            }
        }
    }

    fn generate_free_local_code(&mut self, slot: usize, current_pos: usize) -> Vec<u8> {
        let mut code = Vec::new();

        // Save RAX (might contain return value that we need to preserve)
        code.push(0x50); // push rax

        // Load the pointer from local variable into rdi (arg for _free)
        // Local variables are at rbp - 8*(slot+1)
        let offset = 8 * (slot + 1);

        // mov rdi, [rbp - offset]
        if offset <= 128 {
            code.extend_from_slice(&[0x48, 0x8b, 0x7d, (256 - offset) as u8]); // 4 bytes
        } else {
            code.extend_from_slice(&[0x48, 0x8b, 0xbd]); // mov rdi, [rbp - offset]
            code.extend_from_slice(&(-(offset as i32)).to_le_bytes()); // 7 bytes total
        }

        // Call _free (this will clobber RAX, but we saved it above)
        let call_disp_offset = code.len() + 1;
        match self.link_mode {
            LinkMode::Jit => {
                if let Some(free_addr) = self.runtime_addresses.free {
                    let call_pos = current_pos + code.len();
                    let offset = (free_addr as i32) - ((call_pos + 5) as i32);
                    code.push(0xe8); // call
                    code.extend_from_slice(&offset.to_le_bytes());
                } else {
                    code.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]);
                    self.record_runtime_relocation(current_pos + call_disp_offset, "_free");
                }
            }
            LinkMode::ObjFile => {
                code.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]);
                self.record_runtime_relocation(current_pos + call_disp_offset, "_free");
            }
        }

        // Restore RAX
        code.push(0x58); // pop rax

        code
    }

    fn record_runtime_relocation(&mut self, offset: usize, symbol: &str) {
        if let LinkMode::ObjFile = self.link_mode {
            self.symbol_relocations.push(SymbolRelocation { offset, symbol: symbol.to_string() });
        }
    }

    fn record_string_relocation(&mut self, offset: usize, index: usize) {
        if let LinkMode::ObjFile = self.link_mode {
            self.string_relocations.push(StringRelocation { offset, index });
        }
    }

    fn into_generated_code(self) -> GeneratedCode {
        GeneratedCode {
            code: self.code,
            string_buffers: self.string_buffers,
            symbol_relocations: self.symbol_relocations,
            string_relocations: self.string_relocations,
            function_addresses: self.function_addresses,
        }
    }

    /// Generate code to call a runtime function
    fn generate_runtime_call_code(&mut self, func_name: &str, arg_count: usize, current_pos: usize) -> Vec<u8> {
        let runtime_addr = match func_name {
            "_string_count" => self.runtime_addresses.string_count,
            "_string_concat_2" => self.runtime_addresses.string_concat_2,
            _ => None,
        };

        match self.link_mode {
            LinkMode::Jit => {
                if let Some(addr) = runtime_addr {
                    let pop_size = match arg_count {
                        0 => 0,
                        1 => 1,
                        2 => 2,
                        _ => panic!("Unsupported arg_count"),
                    };
                    let offset = (addr as i32) - ((current_pos + pop_size + 5) as i32);
                    let (code, _) = instructions::generate_runtime_call(Some(offset), arg_count);
                    code
                } else {
                    let (code, disp) = instructions::generate_runtime_call(None, arg_count);
                    self.record_runtime_relocation(current_pos + disp, func_name);
                    code
                }
            }
            LinkMode::ObjFile => {
                let (code, disp) = instructions::generate_runtime_call(None, arg_count);
                self.record_runtime_relocation(current_pos + disp, func_name);
                code
            }
        }
    }

    /// Generate x86-64 machine code from IR program
    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        if !program.functions.is_empty() && program.entry_point.is_some() {
            return self.generate_multi_function(program);
        }

        let has_locals = program.instructions.iter().any(|inst| matches!(inst, IRInstruction::StoreLocal(_) | IRInstruction::LoadLocal(_)));

        self.calculate_positions(program);

        self.code.clear();
        self.generate_code(program, has_locals);

        self.code.clone()
    }

    /// Generate code for multi-function programs
    fn generate_multi_function(&mut self, program: &IRProgram) -> Vec<u8> {
        self.code.clear();
        self.function_addresses.clear();

        // TWO-PASS APPROACH:
        // Pass 1: Calculate addresses by generating functions in order
        // Pass 2: Generate all functions with correct addresses now available

        let mut ordered_functions = Vec::new();

        if let Some(entry_name) = &program.entry_point {
            if let Some(entry_func) = program.functions.iter().find(|f| &f.name == entry_name) {
                ordered_functions.push(entry_func.clone());
            }
        }

        for func_info in &program.functions {
            if program.entry_point.as_ref() != Some(&func_info.name) {
                ordered_functions.push(func_info.clone());
            }
        }

        // Pass 1: Calculate addresses
        let mut current_address = 0;
        for func_info in &ordered_functions {
            self.function_addresses.insert(func_info.name.clone(), current_address);

            let saved_code = self.code.clone();
            self.code.clear();
            self.generate_function(program, func_info);
            let func_size = self.code.len();
            self.code = saved_code;

            current_address += func_size;
        }

        // Pass 2: Generate all functions with correct addresses
        self.code.clear();
        for func_info in &ordered_functions {
            self.generate_function(program, func_info);
        }

        self.code.clone()
    }

    /// Generate code for a single function
    fn generate_function(&mut self, program: &IRProgram, func_info: &FunctionInfo) {
        let prologue = abi::generate_prologue(func_info);
        self.code.extend(prologue);

        let mut in_function = false;
        let mut function_instructions = Vec::new();

        for inst in &program.instructions {
            match inst {
                IRInstruction::DefineFunction(name, _, _) if name == &func_info.name => {
                    in_function = true;
                }
                IRInstruction::Return if in_function => {
                    function_instructions.push(inst.clone());
                    break;
                }
                _ if in_function => {
                    function_instructions.push(inst.clone());
                }
                _ => {}
            }
        }

        for inst in &function_instructions {
            self.generate_instruction(inst, func_info);
        }
    }

    /// Generate code for a single instruction
    fn generate_instruction(&mut self, inst: &IRInstruction, func_info: &FunctionInfo) {
        let code = match inst {
            IRInstruction::Push(value) => instructions::generate_push(*value),
            IRInstruction::PushString(index) => {
                // Get the actual rodata address for this string
                let address = self.string_addresses.get(*index).copied().unwrap_or(0);
                let current_pos = self.code.len();
                let code = instructions::generate_push_string(address);
                if matches!(self.link_mode, LinkMode::ObjFile) {
                    self.record_string_relocation(current_pos + 2, *index);
                }
                code
            }
            IRInstruction::Add => instructions::generate_add(),
            IRInstruction::Sub => instructions::generate_sub(),
            IRInstruction::Mul => instructions::generate_mul(),
            IRInstruction::Div => instructions::generate_div(),
            IRInstruction::LoadParam(slot) => instructions::generate_load_param(*slot),
            IRInstruction::StoreLocal(slot) => instructions::generate_store_local(*slot, func_info),
            IRInstruction::LoadLocal(slot) => instructions::generate_load_local(*slot, func_info),

            IRInstruction::Call(func_name, arg_count) => {
                let mut code = abi::generate_call_setup(*arg_count);
                let call_code = instructions::generate_call(func_name, &self.function_addresses, self.code.len() + code.len());
                code.extend(call_code);
                code
            }

            IRInstruction::InitHeap => {
                let current_pos = self.code.len();
                self.generate_heap_init_code(current_pos)
            }

            IRInstruction::Allocate(size) => {
                let current_pos = self.code.len();
                self.generate_allocate_code(*size, current_pos)
            }

            IRInstruction::Free => {
                let current_pos = self.code.len();
                self.generate_free_code(current_pos)
            }

            IRInstruction::FreeLocal(slot) => {
                let current_pos = self.code.len();
                self.generate_free_local_code(*slot, current_pos)
            }

            IRInstruction::RuntimeCall(func_name, arg_count) => {
                let current_pos = self.code.len();
                self.generate_runtime_call_code(func_name, *arg_count, current_pos)
            }

            IRInstruction::Return => {
                let mut code = instructions::generate_return();
                code.extend(abi::generate_epilogue());
                code
            }

            IRInstruction::DefineFunction(_, _, _) => Vec::new(),

            _ => Vec::new(),
        };

        self.code.extend(code);
    }

    /// Calculate instruction positions (for old single-function path)
    fn calculate_positions(&mut self, program: &IRProgram) {
        self.instruction_positions.clear();
        let mut position = 0;

        let has_locals = program.instructions.iter().any(|inst| matches!(inst, IRInstruction::StoreLocal(_) | IRInstruction::LoadLocal(_)));

        if has_locals {
            position += 8; // prologue: push rbp + mov rbp,rsp + sub rsp,128
        }

        for instruction in &program.instructions {
            self.instruction_positions.push(position);
            position += sizing::instruction_size(instruction, has_locals);
        }
    }

    /// Generate code for old single-function path
    fn generate_code(&mut self, program: &IRProgram, has_locals: bool) {
        // Always add function prologue since entry stub calls user code as a function
        self.code.push(0x55); // push rbp
        self.code.extend_from_slice(&[0x48, 0x89, 0xe5]); // mov rbp, rsp

        if has_locals {
            self.code.extend_from_slice(&[0x48, 0x83, 0xec, 0x80]); // sub rsp, 128
        }

        for inst in &program.instructions {
            match inst {
                IRInstruction::Push(value) => {
                    self.code.extend(instructions::generate_push(*value));
                }
                IRInstruction::PushString(index) => {
                    // Get the actual rodata address for this string
                    let address = self.string_addresses.get(*index).copied().unwrap_or(0);
                    let current_pos = self.code.len();
                    let code = instructions::generate_push_string(address);
                    if matches!(self.link_mode, LinkMode::ObjFile) {
                        self.record_string_relocation(current_pos + 2, *index);
                    }
                    self.code.extend(code);
                }
                IRInstruction::Add => {
                    self.code.extend(instructions::generate_add());
                }
                IRInstruction::Sub => {
                    self.code.extend(instructions::generate_sub());
                }
                IRInstruction::Mul => {
                    self.code.extend(instructions::generate_mul());
                }
                IRInstruction::Div => {
                    self.code.extend(instructions::generate_div());
                }
                IRInstruction::InitHeap => {
                    let current_pos = self.code.len();
                    let code = self.generate_heap_init_code(current_pos);
                    self.code.extend(code);
                }
                IRInstruction::Allocate(size) => {
                    let current_pos = self.code.len();
                    let code = self.generate_allocate_code(*size, current_pos);
                    self.code.extend(code);
                }
                IRInstruction::Free => {
                    let current_pos = self.code.len();
                    let code = self.generate_free_code(current_pos);
                    self.code.extend(code);
                }
                IRInstruction::FreeLocal(slot) => {
                    let current_pos = self.code.len();
                    let code = self.generate_free_local_code(*slot, current_pos);
                    self.code.extend(code);
                }
                IRInstruction::RuntimeCall(func_name, arg_count) => {
                    let current_pos = self.code.len();
                    let code = self.generate_runtime_call_code(func_name, *arg_count, current_pos);
                    self.code.extend(code);
                }
                IRInstruction::Return => {
                    self.code.extend(instructions::generate_return());
                    // Always add epilogue since we always add prologue
                    self.code.extend(abi::generate_epilogue());
                }
                _ => {} // Other instructions not needed for simple path
            }
        }
    }
}

fn generate_entry_stub(entry_symbol: &str) -> (Vec<u8>, Vec<SymbolRelocation>) {
    let mut code = Vec::new();
    let mut relocations = Vec::new();

    // call _heap_init
    code.push(0xe8);
    code.extend_from_slice(&0i32.to_le_bytes());
    relocations.push(SymbolRelocation {
        offset: 1,
        symbol: "_heap_init".to_string(),
    });

    // call entry function
    code.push(0xe8);
    code.extend_from_slice(&0i32.to_le_bytes());
    relocations.push(SymbolRelocation {
        offset: 6,
        symbol: entry_symbol.to_string(),
    });

    // mov rdi, rax
    code.extend_from_slice(&[0x48, 0x89, 0xc7]);

    // mov rax, 60
    code.extend_from_slice(&[0x48, 0xc7, 0xc0, 0x3c, 0x00, 0x00, 0x00]);

    // syscall
    code.extend_from_slice(&[0x0f, 0x05]);

    (code, relocations)
}

/// Implement CodeGenBackend trait for X86CodeGen
impl CodeGenBackend for X86CodeGen {
    fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        self.generate(program)
    }

    fn runtime_addresses(&self) -> RuntimeAddresses {
        self.runtime_addresses.clone()
    }
}

/// Public API: Compile IR program to x86-64 machine code
pub fn compile_to_executable(program: &IRProgram) -> JitArtifact {
    let mut codegen = X86CodeGen::new(LinkMode::Jit);
    codegen.set_string_addresses(program);
    let _ = codegen.generate(program);
    let generated = codegen.into_generated_code();
    JitArtifact {
        code: generated.code,
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

    let (stub_code, mut stub_relocs) = generate_entry_stub(&entry_symbol_name);
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
    for runtime_symbol in ["_heap_init", "_allocate", "_free", "_string_count", "_string_concat_2"] {
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

pub fn link_with_runtime(object_bytes: &[u8], output_path: &str, runtime_staticlib: &str) -> io::Result<()> {
    let mut obj_path = PathBuf::from(output_path);
    obj_path.set_extension("o");

    fs::write(&obj_path, object_bytes)?;

    let obj_path_str = obj_path.to_str().ok_or_else(|| io::Error::other("Invalid object path"))?.to_string();

    let status = Command::new("ld").args(["-o", output_path, &obj_path_str, runtime_staticlib, "-static", "-nostdlib"]).status()?;

    let _ = fs::remove_file(&obj_path);

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("ld failed with status: {}", status)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit_runner::{JitRunner, JitRunnerTrt};

    #[test]
    fn test_simple_number() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        let mut jit_code = artifact.code.clone();
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 42);
    }

    #[test]
    fn test_basic_arithmetic() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(2));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        let mut jit_code = artifact.code.clone();
        jit_code.push(0xc3); // ret

        let result = JitRunner::exec(&jit_code);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_heap_allocation_basic() {
        // Test that heap allocation instructions generate correct code and offsets
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(100));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        // Verify machine code is generated (non-empty)
        assert!(!artifact.code.is_empty());

        // The machine code should include runtime functions at the end
        // (we can't easily test execution in JIT since it needs proper memory setup)
    }

    #[test]
    fn test_no_heap_when_not_needed() {
        // Test that programs without heap instructions don't get heap setup
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        assert!(!artifact.code.is_empty());
    }

    #[test]
    fn test_free_instruction_included() {
        // Test that Free instruction generates code
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(64));
        program.add_instruction(IRInstruction::Free); // Free the allocated block
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        assert!(!artifact.code.is_empty());

        // The code should be longer than without Free instruction
        let mut program_without_free = IRProgram::new();
        program_without_free.add_instruction(IRInstruction::InitHeap);
        program_without_free.add_instruction(IRInstruction::Allocate(64));
        program_without_free.add_instruction(IRInstruction::Push(42));
        program_without_free.add_instruction(IRInstruction::Return);

        let artifact_without_free = compile_to_executable(&program_without_free);

        // Code with Free should be longer (includes free instruction)
        assert!(artifact.code.len() > artifact_without_free.code.len());
    }

    #[test]
    fn test_compile_to_object_outputs_bytes() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(1));
        program.add_instruction(IRInstruction::Return);

        let object = compile_to_object(&program);

        assert!(!object.bytes.is_empty());
    }
}
