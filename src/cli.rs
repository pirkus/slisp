/// CLI commands for file compilation
use crate::ast::parse_file;
use crate::codegen::{compile_to_executable, detect_host_target};
use crate::compiler::compile_program;
use crate::repl::format_compile_error;
use std::fs;
use std::process::Command;

/// Compile a .slisp file to an executable
pub fn compile_file_to_executable(input_file: &str, output_file: &str) -> Result<(), String> {
    let file_content = fs::read_to_string(input_file)
        .map_err(|e| format!("Failed to read file '{}': {}", input_file, e))?;

    let expressions = parse_file(&file_content)?;
    let ir_program = compile_program(&expressions).map_err(|e| format_compile_error(&e))?;

    let target = detect_host_target();
    let (machine_code, heap_init_offset) = compile_to_executable(&ir_program, target);

    target
        .generate_executable(&machine_code, output_file, heap_init_offset)
        .map_err(|e| format!("Failed to write executable: {}", e))?;

    Command::new("chmod")
        .args(["+x", output_file])
        .output()
        .map_err(|e| format!("Failed to chmod executable: {}", e))?;

    Ok(())
}
