/// CLI commands for file compilation
use crate::ast::parse_file;
use crate::codegen::{compile_to_object, detect_host_target, link_executable};
use crate::compiler::compile_program;
use crate::repl::format_compile_error;
use std::collections::HashSet;
use std::fs;
use std::process::Command;

#[derive(Hash, Eq, PartialEq)]
pub enum DebugFeature {
    TraceAllocations,
    DumpIr,
}

/// Compile a .slisp file to an executable
pub fn compile_file_to_executable(input_file: &str, output_file: &str, keep_object_file: bool, debug_features: &HashSet<DebugFeature>) -> Result<(), String> {
    let file_content = fs::read_to_string(input_file).map_err(|e| format!("Failed to read file '{}': {}", input_file, e))?;

    let expressions = parse_file(&file_content)?;
    let mut ir_program = compile_program(&expressions).map_err(|e| format_compile_error(&e))?;
    ir_program.telemetry_enabled = debug_features.contains(&DebugFeature::TraceAllocations);

    if debug_features.contains(&DebugFeature::DumpIr) {
        println!("; IR dump for {}", input_file);
        for (idx, inst) in ir_program.instructions.iter().enumerate() {
            println!("{:04}: {:?}", idx, inst);
        }
        return Ok(());
    }

    let target = detect_host_target();
    let object = compile_to_object(&ir_program, target);

    let runtime_staticlib = env!("SLISP_RUNTIME_LIB");

    link_executable(target, &object.bytes, output_file, runtime_staticlib, keep_object_file).map_err(|e| format!("Failed to link executable: {}", e))?;

    Command::new("chmod").args(["+x", output_file]).output().map_err(|e| format!("Failed to chmod executable: {}", e))?;

    Ok(())
}
