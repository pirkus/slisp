use super::x86_64_linux;
/// Public API for code generation
///
/// This module provides the high-level functions for compiling IR to machine code
/// and generating executables for different target platforms.
use crate::ir::IRProgram;

/// Supported target platforms (architecture + OS)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Target {
    X86_64Linux,
    X86_64Windows,
    X86_64MacOS,
    ARM64Linux,
    RISCV64Linux,
}

impl Target {
    /// Compile IR program to machine code for this target
    ///
    /// # Arguments
    /// * `program` - The IR program to compile
    ///
    /// # Returns
    /// * `(machine_code, heap_init_offset)` - Generated machine code and optional heap init offset
    pub fn compile(&self, program: &IRProgram) -> x86_64_linux::JitArtifact {
        match self {
            Target::X86_64Linux => x86_64_linux::compile_to_executable(program),
            Target::X86_64Windows => {
                unimplemented!("x86-64 Windows (PE format) not yet implemented")
            }
            Target::X86_64MacOS => {
                unimplemented!("x86-64 macOS (Mach-O format) not yet implemented")
            }
            Target::ARM64Linux => {
                unimplemented!("ARM64 Linux code generation not yet implemented")
            }
            Target::RISCV64Linux => {
                unimplemented!("RISC-V Linux code generation not yet implemented")
            }
        }
    }

    /// Generate an executable file for this target from machine code
    ///
    /// # Arguments
    /// * `machine_code` - The compiled machine code
    /// * `output_path` - Path to write the executable
    /// * `heap_init_offset` - Optional offset to heap initialization function
    /// * `string_literals` - String literals to embed in rodata segment
    ///
    /// # Returns
    /// * Result indicating success or IO error
    pub fn compile_to_jit(&self, program: &IRProgram) -> x86_64_linux::JitArtifact {
        self.compile(program)
    }

    pub fn compile_object(&self, program: &IRProgram) -> x86_64_linux::ObjectArtifact {
        match self {
            Target::X86_64Linux => x86_64_linux::compile_to_object(program),
            Target::X86_64Windows => {
                unimplemented!("x86-64 Windows (PE format) not yet implemented")
            }
            Target::X86_64MacOS => {
                unimplemented!("x86-64 macOS (Mach-O format) not yet implemented")
            }
            Target::ARM64Linux => {
                unimplemented!("ARM64 Linux code generation not yet implemented")
            }
            Target::RISCV64Linux => {
                unimplemented!("RISC-V Linux code generation not yet implemented")
            }
        }
    }

    pub fn link_executable(&self, object_bytes: &[u8], output_path: &str, runtime_staticlib: &str) -> std::io::Result<()> {
        match self {
            Target::X86_64Linux => x86_64_linux::link_with_runtime(object_bytes, output_path, runtime_staticlib),
            Target::X86_64Windows => {
                unimplemented!("x86-64 Windows (PE format) not yet implemented")
            }
            Target::X86_64MacOS => {
                unimplemented!("x86-64 macOS (Mach-O format) not yet implemented")
            }
            Target::ARM64Linux => {
                unimplemented!("ARM64 Linux executable generation not yet implemented")
            }
            Target::RISCV64Linux => {
                unimplemented!("RISC-V Linux executable generation not yet implemented")
            }
        }
    }
}

/// Compile IR program to machine code suitable for in-process JIT execution
pub fn compile_to_executable(program: &IRProgram, target: Target) -> x86_64_linux::JitArtifact {
    target.compile_to_jit(program)
}

pub fn compile_to_object(program: &IRProgram, target: Target) -> x86_64_linux::ObjectArtifact {
    target.compile_object(program)
}

pub fn link_executable(target: Target, object_bytes: &[u8], output_path: &str, runtime_staticlib: &str) -> std::io::Result<()> {
    target.link_executable(object_bytes, output_path, runtime_staticlib)
}

/// Detect the host target platform
///
/// Currently always returns X86_64Linux. In the future, this will use
/// conditional compilation to detect the actual host platform.
///
/// # Returns
/// * The detected target platform
#[allow(dead_code)]
pub fn detect_host_target() -> Target {
    // Future: use conditional compilation to detect target
    // #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    // return Target::X86_64Linux;
    // #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
    // return Target::X86_64Windows;
    // #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    // return Target::ARM64Linux;

    Target::X86_64Linux
}
