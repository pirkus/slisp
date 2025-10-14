/// Code generation module - architecture-agnostic interface
///
/// This module provides a clean, architecture-agnostic API for code generation.
/// It supports multiple target architectures and operating systems through a trait-based design.
///
/// Current supported targets:
/// - x86-64 Linux (ELF, System V ABI, Linux syscalls)
///
/// Future targets:
/// - x86-64 Windows (PE format, Windows API)
/// - x86-64 macOS (Mach-O format, macOS syscalls)
/// - ARM64 Linux (ELF, AAPCS64, Linux syscalls)
/// - RISC-V Linux
/// - WebAssembly
///
/// ## Module Structure
/// - `api`: Public API functions for compilation
/// - `backend`: Trait definitions for architecture-independent interface
/// - `x86_64_linux`: x86-64 Linux target implementation
mod api;
mod backend;
pub mod x86_64_linux;

// Re-export public API
pub use api::{compile_to_executable, detect_host_target};

// Export backend trait for future use with multiple architecture backends
#[allow(unused_imports)]
pub use backend::{CodeGenBackend, RuntimeAddresses};
