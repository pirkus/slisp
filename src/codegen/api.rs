use super::x86_64_linux::{self, X86_64LinuxBackend};
/// Public API for code generation
///
/// This module provides the high-level functions for compiling IR to machine code
/// and generating executables for different target platforms.
use crate::codegen::backend::{JitArtifact, ObjectArtifact, TargetBackend};
use crate::ir::IRProgram;
use std::io;

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
    fn create_backend(&self) -> Box<dyn TargetBackend> {
        match self {
            Target::X86_64Linux => Box::new(X86_64LinuxBackend::new()),
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

    fn link_object(&self, object_bytes: &[u8], output_path: &str, runtime_staticlib: &str, keep_object: bool) -> io::Result<()> {
        match self {
            Target::X86_64Linux => x86_64_linux::link_with_runtime(object_bytes, output_path, runtime_staticlib, keep_object),
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
pub fn compile_to_executable(program: &IRProgram, target: Target) -> JitArtifact {
    let mut backend = target.create_backend();
    backend.compile_jit(program)
}

pub fn compile_to_object(program: &IRProgram, target: Target) -> ObjectArtifact {
    let mut backend = target.create_backend();
    backend.compile_object(program)
}

pub fn link_executable(target: Target, object_bytes: &[u8], output_path: &str, runtime_staticlib: &str, keep_object: bool) -> io::Result<()> {
    target.link_object(object_bytes, output_path, runtime_staticlib, keep_object)
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
