use super::{abi, helpers::compute_local_count, instructions};
use crate::codegen::backend::{CodeGenBackend, RuntimeAddresses};
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use slisp_runtime;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) enum LinkMode {
    Jit,
    ObjFile,
}

#[derive(Clone)]
pub(super) struct SymbolRelocation {
    pub offset: usize,
    pub symbol: String,
}

#[derive(Clone)]
pub(super) struct StringRelocation {
    pub offset: usize,
    pub index: usize,
}

struct PendingJump {
    target: usize,
    patch_offset: usize,
}

pub(super) struct GeneratedCode {
    pub code: Vec<u8>,
    pub string_buffers: Vec<Box<[u8]>>,
    pub symbol_relocations: Vec<SymbolRelocation>,
    pub string_relocations: Vec<StringRelocation>,
    pub function_addresses: HashMap<String, usize>,
    pub runtime_addresses: RuntimeAddresses,
}

pub(super) struct X86CodeGen {
    pub code: Vec<u8>,
    pub function_addresses: HashMap<String, usize>, // function name -> code offset
    pub runtime_addresses: RuntimeAddresses,        // addresses of runtime support functions
    pub string_addresses: Vec<u64>,                 // addresses of string literals in rodata segment
    pub link_mode: LinkMode,
    pub symbol_relocations: Vec<SymbolRelocation>,
    pub string_relocations: Vec<StringRelocation>,
    pub string_buffers: Vec<Box<[u8]>>, // Holds string data alive for JIT mode
}

impl X86CodeGen {
    pub(super) fn new(link_mode: LinkMode) -> Self {
        let runtime_addresses = match link_mode {
            LinkMode::Jit => RuntimeAddresses {
                heap_init: Some(slisp_runtime::_heap_init as usize),
                allocate: Some(slisp_runtime::_allocate as usize),
                free: Some(slisp_runtime::_free as usize),
                string_count: Some(slisp_runtime::_string_count as usize),
                string_concat_n: Some(slisp_runtime::_string_concat_n as usize),
                string_clone: Some(slisp_runtime::_string_clone as usize),
                string_get: Some(slisp_runtime::_string_get as usize),
                string_subs: Some(slisp_runtime::_string_subs as usize),
                string_normalize: Some(slisp_runtime::_string_normalize as usize),
                string_from_number: Some(slisp_runtime::_string_from_number as usize),
                string_from_boolean: Some(slisp_runtime::_string_from_boolean as usize),
                string_equals: Some(slisp_runtime::_string_equals as usize),
                map_value_clone: Some(slisp_runtime::_map_value_clone as usize),
                map_free: Some(slisp_runtime::_map_free as usize),
                set_free: Some(slisp_runtime::_set_free as usize),
                vector_free: Some(slisp_runtime::_vector_free as usize),
            },
            LinkMode::ObjFile => RuntimeAddresses {
                heap_init: None,
                allocate: None,
                free: None,
                string_count: None,
                string_concat_n: None,
                string_clone: None,
                string_get: None,
                string_subs: None,
                string_normalize: None,
                string_from_number: None,
                string_from_boolean: None,
                string_equals: None,
                map_value_clone: None,
                map_free: None,
                set_free: None,
                vector_free: None,
            },
        };

        Self {
            code: Vec::new(),
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
    pub fn generate_heap_init_code(&mut self, current_pos: usize) -> Vec<u8> {
        let (code, disp) = instructions::generate_call_heap_init(None);
        self.record_runtime_relocation(current_pos + disp, "_heap_init");
        code
    }

    /// Generate code to call _allocate runtime function
    pub fn generate_allocate_code(&mut self, size: usize, current_pos: usize) -> Vec<u8> {
        let (code, disp) = instructions::generate_allocate_inline(size, None);
        self.record_runtime_relocation(current_pos + disp, "_allocate");
        code
    }

    /// Generate code to call _free runtime function
    pub fn generate_free_code(&mut self, current_pos: usize) -> Vec<u8> {
        let (code, disp) = instructions::generate_free_inline(None);
        self.record_runtime_relocation(current_pos + disp, "_free");
        code
    }

    pub fn generate_free_local_code(&mut self, slot: usize, func_info: &FunctionInfo, current_pos: usize) -> Vec<u8> {
        let mut code = Vec::new();

        // Save RAX (might contain return value that we need to preserve)
        code.push(0x50); // push rax

        // Load the pointer from local variable into rdi (arg for _free).
        // Mirror the addressing scheme used by store/load helpers so we
        // account for parameters that occupy stack space above the locals.
        let offset = 8 * (func_info.param_count + slot + 1);

        // mov rdi, [rbp - offset]
        if offset <= 128 {
            code.extend_from_slice(&[0x48, 0x8b, 0x7d, (256 - offset) as u8]); // 4 bytes
        } else {
            code.extend_from_slice(&[0x48, 0x8b, 0xbd]); // mov rdi, [rbp - offset]
            code.extend_from_slice(&(-(offset as i32)).to_le_bytes()); // 7 bytes total
        }

        // Call _free (this will clobber RAX, but we saved it above)
        let call_disp_offset = code.len() + 1;
        code.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]);
        self.record_runtime_relocation(current_pos + call_disp_offset, "_free");

        // Restore RAX
        code.push(0x58); // pop rax

        // Zero out the local slot so stale pointers are not freed again if the slot
        // gets reused before another store.
        if offset <= 128 {
            code.extend_from_slice(&[0x48, 0xc7, 0x45, (256 - offset) as u8, 0x00, 0x00, 0x00, 0x00]);
        } else {
            code.extend_from_slice(&[0x48, 0xc7, 0x85]);
            code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
            code.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        }

        code
    }

    pub fn generate_free_local_with_runtime_code(&mut self, slot: usize, runtime: &str, func_info: &FunctionInfo, current_pos: usize) -> Vec<u8> {
        let mut code = Vec::new();

        // Preserve RAX as the runtime helper may clobber it.
        code.push(0x50); // push rax

        let offset = 8 * (func_info.param_count + slot + 1);

        // Load the pointer stored in the target slot into RDI (first arg).
        if offset <= 128 {
            code.extend_from_slice(&[0x48, 0x8b, 0x7d, (256 - offset) as u8]);
        } else {
            code.extend_from_slice(&[0x48, 0x8b, 0xbd]);
            code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
        }

        // Call the runtime free helper.
        let call_disp_offset = code.len() + 1;
        code.extend_from_slice(&[0xe8, 0x00, 0x00, 0x00, 0x00]);
        self.record_runtime_relocation(current_pos + call_disp_offset, runtime);

        // Restore RAX so callers see the original value.
        code.push(0x58); // pop rax

        // Zero the slot to avoid double-free on reuse.
        if offset <= 128 {
            code.extend_from_slice(&[0x48, 0xc7, 0x45, (256 - offset) as u8, 0x00, 0x00, 0x00, 0x00]);
        } else {
            code.extend_from_slice(&[0x48, 0xc7, 0x85]);
            code.extend_from_slice(&(-(offset as i32)).to_le_bytes());
            code.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        }

        code
    }

    pub fn record_runtime_relocation(&mut self, offset: usize, symbol: &str) {
        self.symbol_relocations.push(SymbolRelocation { offset, symbol: symbol.to_string() });
    }

    pub fn record_string_relocation(&mut self, offset: usize, index: usize) {
        if let LinkMode::ObjFile = self.link_mode {
            self.string_relocations.push(StringRelocation { offset, index });
        }
    }

    pub fn into_generated_code(self) -> GeneratedCode {
        GeneratedCode {
            code: self.code,
            string_buffers: self.string_buffers,
            symbol_relocations: self.symbol_relocations,
            string_relocations: self.string_relocations,
            function_addresses: self.function_addresses,
            runtime_addresses: self.runtime_addresses,
        }
    }

    /// Generate code to call a runtime function
    pub fn generate_runtime_call_code(&mut self, func_name: &str, arg_count: usize, current_pos: usize) -> Vec<u8> {
        let (code, disp) = instructions::generate_runtime_call(None, arg_count);
        self.record_runtime_relocation(current_pos + disp, func_name);
        code
    }

    /// Generate x86-64 machine code from IR program
    pub fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        if !program.functions.is_empty() && program.entry_point.is_some() {
            return self.generate_multi_function(program);
        }

        self.generate_single_function(program)
    }

    /// Generate code for multi-function programs
    fn generate_multi_function(&mut self, program: &IRProgram) -> Vec<u8> {
        self.code.clear();
        self.function_addresses.clear();

        // TWO-PASS APPROACH:
        // Pass 1: Calculate addresses by generating functions in order
        // Pass 2: Generate all functions with correct addresses now available

        let mut ordered_functions = Vec::new();
        let entry_name = program.entry_point.clone();

        if let Some(entry_name) = &entry_name {
            if let Some(entry_func) = program.functions.iter().find(|f| &f.name == entry_name) {
                ordered_functions.push(entry_func.clone());
            }
        }

        for func_info in &program.functions {
            if entry_name.as_ref() != Some(&func_info.name) {
                ordered_functions.push(func_info.clone());
            }
        }

        // Pass 1: Calculate addresses
        let mut current_address = 0;
        for func_info in &ordered_functions {
            self.function_addresses.insert(func_info.name.clone(), current_address);

            let saved_code = self.code.clone();
            let saved_symbol_relocs_len = self.symbol_relocations.len();
            let saved_string_relocs_len = self.string_relocations.len();
            self.code.clear();
            self.generate_function(program, func_info);
            let func_size = self.code.len();
            self.code = saved_code;
            self.symbol_relocations.truncate(saved_symbol_relocs_len);
            self.string_relocations.truncate(saved_string_relocs_len);

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
        let mut function_instructions: Vec<(usize, IRInstruction)> = Vec::new();
        let mut last_index = None;

        for (idx, inst) in program.instructions.iter().enumerate() {
            match inst {
                IRInstruction::DefineFunction(name, _, _) if name == &func_info.name => {
                    in_function = true;
                }
                IRInstruction::Return if in_function => {
                    function_instructions.push((idx, inst.clone()));
                    last_index = Some(idx);
                    break;
                }
                _ if in_function => {
                    function_instructions.push((idx, inst.clone()));
                    last_index = Some(idx);
                }
                _ => {}
            }
        }

        let function_end_index = last_index.map_or(0, |idx| idx + 1);
        let mut instruction_offsets = std::collections::HashMap::new();
        let mut pending_jumps = Vec::new();

        for (idx, inst) in &function_instructions {
            instruction_offsets.insert(*idx, self.code.len());
            let code = self.generate_instruction(inst, func_info, &mut pending_jumps);
            self.code.extend(code);
        }

        self.patch_pending_jumps(&pending_jumps, &instruction_offsets, function_end_index, self.code.len());
    }

    /// Generate code for a single instruction
    fn generate_instruction(&mut self, inst: &IRInstruction, func_info: &FunctionInfo, pending_jumps: &mut Vec<PendingJump>) -> Vec<u8> {
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
            IRInstruction::Equal => instructions::generate_equal(),
            IRInstruction::Less => instructions::generate_less(),
            IRInstruction::Greater => instructions::generate_greater(),
            IRInstruction::LessEqual => instructions::generate_less_equal(),
            IRInstruction::GreaterEqual => instructions::generate_greater_equal(),
            IRInstruction::Not => instructions::generate_not(),
            IRInstruction::LoadParam(slot) => instructions::generate_load_param(*slot),
            IRInstruction::StoreLocal(slot) => instructions::generate_store_local(*slot, func_info),
            IRInstruction::LoadLocal(slot) => instructions::generate_load_local(*slot, func_info),
            IRInstruction::PushLocalAddress(slot) => instructions::generate_push_local_address(*slot, func_info),

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
                self.generate_free_local_code(*slot, func_info, current_pos)
            }

            IRInstruction::FreeLocalWithRuntime(slot, runtime) => {
                let current_pos = self.code.len();
                self.generate_free_local_with_runtime_code(*slot, runtime, func_info, current_pos)
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

            IRInstruction::JumpIfZero(target) => {
                let (code, disp_offset) = instructions::generate_jump_if_zero();
                let patch_offset = self.code.len() + disp_offset;
                pending_jumps.push(PendingJump { target: *target, patch_offset });
                code
            }

            IRInstruction::Jump(target) => {
                let (code, disp_offset) = instructions::generate_jump();
                let patch_offset = self.code.len() + disp_offset;
                pending_jumps.push(PendingJump { target: *target, patch_offset });
                code
            }
        };

        code
    }

    fn patch_pending_jumps(&mut self, pending_jumps: &[PendingJump], instruction_offsets: &std::collections::HashMap<usize, usize>, end_index: usize, end_offset: usize) {
        for pending in pending_jumps {
            let target_offset = instruction_offsets
                .get(&pending.target)
                .copied()
                .or_else(|| if pending.target == end_index { Some(end_offset) } else { None })
                .unwrap_or_else(|| panic!("Invalid jump target {}", pending.target));

            let branch_end = pending.patch_offset + 4;
            let rel = (target_offset as isize) - (branch_end as isize);
            let rel_bytes = (rel as i32).to_le_bytes();
            let patch_end = pending.patch_offset + 4;
            self.code[pending.patch_offset..patch_end].copy_from_slice(&rel_bytes);
        }
    }

    fn generate_single_function(&mut self, program: &IRProgram) -> Vec<u8> {
        self.code.clear();

        let local_count = compute_local_count(&program.instructions);
        let stub_info = FunctionInfo {
            name: String::new(),
            param_count: 0,
            start_address: 0,
            local_count,
        };

        self.code.extend(abi::generate_prologue(&stub_info));

        let mut instruction_offsets = std::collections::HashMap::new();
        let mut pending_jumps = Vec::new();

        for (idx, inst) in program.instructions.iter().enumerate() {
            instruction_offsets.insert(idx, self.code.len());
            let code = self.generate_instruction(inst, &stub_info, &mut pending_jumps);
            self.code.extend(code);
        }

        self.patch_pending_jumps(&pending_jumps, &instruction_offsets, program.instructions.len(), self.code.len());

        self.code.clone()
    }
}

impl CodeGenBackend for X86CodeGen {
    fn generate(&mut self, program: &IRProgram) -> Vec<u8> {
        self.generate(program)
    }

    fn runtime_addresses(&self) -> RuntimeAddresses {
        self.runtime_addresses.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::x86_64_linux::{compile_to_executable, compile_to_object};
    use crate::ir::{IRInstruction, IRProgram};
    use crate::jit_runner::JitRunner;

    #[test]
    fn jit_compiles_simple_number() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        let result = JitRunner::exec_artifact(&artifact);
        assert_eq!(result, 42);
    }

    #[test]
    fn jit_handles_basic_arithmetic() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(2));
        program.add_instruction(IRInstruction::Push(3));
        program.add_instruction(IRInstruction::Add);
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);

        let result = JitRunner::exec_artifact(&artifact);
        assert_eq!(result, 5);
    }

    #[test]
    fn jit_keeps_heap_setup_when_requested() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(100));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);
        assert!(!artifact.code.is_empty());
    }

    #[test]
    fn jit_skips_heap_when_unused() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);
        assert!(!artifact.code.is_empty());
    }

    #[test]
    fn jit_emits_free_instruction() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(64));
        program.add_instruction(IRInstruction::Free);
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);
        assert!(!artifact.code.is_empty());

        let mut program_without_free = IRProgram::new();
        program_without_free.add_instruction(IRInstruction::InitHeap);
        program_without_free.add_instruction(IRInstruction::Allocate(64));
        program_without_free.add_instruction(IRInstruction::Push(42));
        program_without_free.add_instruction(IRInstruction::Return);

        let artifact_without_free = compile_to_executable(&program_without_free);
        assert!(artifact.code.len() > artifact_without_free.code.len());
    }

    #[test]
    fn jit_executes_runtime_calls() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::InitHeap);
        program.add_instruction(IRInstruction::Allocate(32));
        program.add_instruction(IRInstruction::Free);
        program.add_instruction(IRInstruction::Push(7));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);
        let result = JitRunner::exec_artifact(&artifact);
        assert_eq!(result, 7);
    }

    #[test]
    fn jit_executes_if_true_branch() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(1));
        program.add_instruction(IRInstruction::Push(1));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::JumpIfZero(6));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Jump(7));
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);
        let result = JitRunner::exec_artifact(&artifact);
        assert_eq!(result, 42);
    }

    #[test]
    fn jit_executes_if_false_branch() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(1));
        program.add_instruction(IRInstruction::Push(2));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::JumpIfZero(6));
        program.add_instruction(IRInstruction::Push(42));
        program.add_instruction(IRInstruction::Jump(7));
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Return);

        let artifact = compile_to_executable(&program);
        let result = JitRunner::exec_artifact(&artifact);
        assert_eq!(result, 0);
    }

    #[test]
    fn object_compilation_produces_bytes() {
        let mut program = IRProgram::new();
        program.add_instruction(IRInstruction::Push(1));
        program.add_instruction(IRInstruction::Return);

        let object = compile_to_object(&program);
        assert!(!object.bytes.is_empty());
    }
}
