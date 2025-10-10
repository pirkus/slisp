/// CLI commands for file compilation

use crate::ast_parser::{AstParser, AstParserTrt};
use crate::codegen::compile_to_executable;
use crate::compiler::{compile_program, compile_to_ir};
use crate::domain::Node;
use crate::elf_gen::generate_elf_executable;
use crate::repl::format_compile_error;
use std::fs;
use std::process::Command;

/// Compile a .slisp file to an executable
pub fn compile_file_to_executable(input_file: &str, output_file: &str) -> Result<(), String> {
    // Read the file content
    let file_content = fs::read_to_string(input_file)
        .map_err(|e| format!("Failed to read file '{}': {}", input_file, e))?;

    // Parse all expressions in the file
    let expressions = parse_all_expressions(&file_content)?;

    // Compile the entire program
    let ir_program = match compile_program(&expressions) {
        Ok(program) => program,
        Err(error) => return Err(format_compile_error(&error)),
    };

    // Generate machine code
    let machine_code = compile_to_executable(&ir_program);

    // Generate ELF executable
    match generate_elf_executable(&machine_code, output_file) {
        Ok(()) => {}
        Err(e) => return Err(format!("Failed to write executable: {}", e)),
    }

    // Make executable on Unix systems
    #[cfg(unix)]
    {
        Command::new("chmod")
            .args(["+x", output_file])
            .output()
            .map_err(|e| format!("Failed to chmod executable: {}", e))?;
    }

    Ok(())
}

/// Compile a single expression to an executable file
pub fn compile_to_file(expression: &str, output_file: &str) -> Result<(), String> {
    // Parse the expression
    let ast = match std::panic::catch_unwind(|| {
        AstParser::parse_sexp_new_domain(expression.as_bytes(), &mut 0)
    }) {
        Ok(ast) => ast,
        Err(_) => return Err("Parse error: malformed expression".to_string()),
    };

    // Compile to IR
    let ir_program = match compile_to_ir(&ast) {
        Ok(program) => program,
        Err(error) => return Err(format_compile_error(&error)),
    };

    // Generate machine code
    let machine_code = compile_to_executable(&ir_program);

    // Generate ELF executable
    match generate_elf_executable(&machine_code, output_file) {
        Ok(()) => {}
        Err(e) => return Err(format!("Failed to write executable: {}", e)),
    }

    // Make executable on Unix systems
    #[cfg(unix)]
    {
        Command::new("chmod")
            .args(["+x", output_file])
            .output()
            .map_err(|e| format!("Failed to chmod executable: {}", e))?;
    }

    Ok(())
}

/// Parse all top-level expressions from file content
fn parse_all_expressions(file_content: &str) -> Result<Vec<Node>, String> {
    let mut expressions = Vec::new();
    let mut offset = 0;

    loop {
        // Skip whitespace
        while offset < file_content.len()
            && file_content.chars().nth(offset).unwrap().is_whitespace()
        {
            offset += 1;
        }

        if offset >= file_content.len() {
            break;
        }

        // Find the end of this S-expression by tracking parentheses depth
        let start_offset = offset;
        let mut depth = 0;
        let mut in_string = false;
        let mut found_expression = false;

        while offset < file_content.len() {
            let c = file_content.chars().nth(offset).unwrap();

            match c {
                '(' if !in_string => {
                    depth += 1;
                    found_expression = true;
                }
                ')' if !in_string => {
                    depth -= 1;
                    if depth == 0 && found_expression {
                        offset += 1; // Include the closing paren
                        break;
                    }
                }
                '"' => in_string = !in_string,
                _ if !in_string && !c.is_whitespace() && depth == 0 => {
                    // Single atom at top level
                    found_expression = true;
                    while offset < file_content.len() {
                        let next_c = file_content.chars().nth(offset).unwrap();
                        if next_c.is_whitespace() || next_c == '(' || next_c == ')' {
                            break;
                        }
                        offset += 1;
                    }
                    break;
                }
                _ => {}
            }
            offset += 1;
        }

        if !found_expression {
            break;
        }

        // Parse this single expression
        let expression_text = &file_content[start_offset..offset];
        let mut parse_offset = 0;
        let ast = AstParser::parse_sexp_new_domain(expression_text.as_bytes(), &mut parse_offset);
        expressions.push(ast);
    }

    if expressions.is_empty() {
        Err("No expressions found in file".to_string())
    } else {
        Ok(expressions)
    }
}
