/// Architecture-agnostic code generation backend trait
///
/// This trait defines the interface that all code generation backends must implement.
/// It allows supporting multiple target architectures (x86-64, ARM64, RISC-V, etc.)
/// with a unified interface.
use crate::ir::IRProgram;

/// Resulting machine code buffer for in-process execution.
#[derive(Debug)]
pub struct JitArtifact {
    pub code: Vec<u8>,
    #[allow(dead_code)]
    pub _string_buffers: Vec<Box<[u8]>>,
}

impl JitArtifact {
    pub fn as_code(&self) -> &[u8] {
        &self.code
    }
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
