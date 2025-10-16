/// Architecture-agnostic code generation backend trait
///
/// This trait defines the interface that all code generation backends must implement.
/// It allows supporting multiple target architectures (x86-64, ARM64, RISC-V, etc.)
/// with a unified interface.
use crate::ir::IRProgram;

/// Runtime support function addresses (architecture-agnostic)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeAddresses {
    pub heap_init: Option<usize>,
    pub allocate: Option<usize>,
    pub free: Option<usize>,
    pub string_count: Option<usize>,
    pub string_concat_2: Option<usize>,
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
