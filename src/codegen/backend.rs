/// Architecture-agnostic code generation backend trait
///
/// This trait defines the interface that all code generation backends must implement.
/// It allows supporting multiple target architectures (x86-64, ARM64, RISC-V, etc.)
/// with a unified interface.
use crate::ir::IRProgram;

/// Resulting machine code buffer for in-process execution.
#[derive(Debug, Clone)]
pub struct RuntimeRelocation {
    pub offset: usize,
    pub symbol: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeStub {
    pub symbol: String,
    pub offset: usize,
}

#[derive(Debug)]
pub struct JitArtifact {
    pub code: Vec<u8>,
    pub runtime_relocations: Vec<RuntimeRelocation>,
    pub runtime_addresses: RuntimeAddresses,
    pub runtime_stubs: Vec<RuntimeStub>,
    #[allow(dead_code)]
    pub _string_buffers: Vec<Box<[u8]>>,
}

/// Serialized object file bytes suitable for further linking.
#[derive(Debug)]
pub struct ObjectArtifact {
    pub bytes: Vec<u8>,
}

/// High-level interface each target backend must implement.
pub trait TargetBackend {
    fn compile_jit(&mut self, program: &IRProgram) -> JitArtifact;
    fn compile_object(&mut self, program: &IRProgram) -> ObjectArtifact;
}

/// Runtime support function addresses (architecture-agnostic)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeAddresses {
    pub heap_init: Option<usize>,
    pub allocate: Option<usize>,
    pub free: Option<usize>,
    pub string_count: Option<usize>,
    pub string_concat_n: Option<usize>,
    pub string_clone: Option<usize>,
    pub string_get: Option<usize>,
    pub string_subs: Option<usize>,
    pub string_normalize: Option<usize>,
    pub string_from_number: Option<usize>,
    pub string_from_boolean: Option<usize>,
    pub string_equals: Option<usize>,
    pub count_debug_enable: Option<usize>,
    pub map_value_clone: Option<usize>,
    pub map_free: Option<usize>,
    pub set_free: Option<usize>,
    pub vector_free: Option<usize>,
}

/// Code generation backend trait for different target architectures
#[allow(dead_code)]
pub trait CodeGenBackend {
    /// Generate machine code from IR program
    /// Returns the generated machine code as a byte vector
    fn generate(&mut self, program: &IRProgram) -> Vec<u8>;

    /// Get runtime function addresses (if any)
    /// Used for heap allocation and other runtime support
    fn runtime_addresses(&self) -> RuntimeAddresses;
}
