mod ast_parser;
mod codegen;
mod compiler;
mod domain;
mod elf_gen;
mod evaluator;
mod ir;
mod jit_runner;

use ast_parser::{AstParser, AstParserTrt};
use codegen::compile_to_executable;
use compiler::{compile_to_ir, CompileError};
use domain::{Node, Primitive};
use elf_gen::generate_elf_executable;
use evaluator::{eval_node, EvalError, Value};
use jit_runner::{JitRunner, JitRunnerTrt};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 4 && args[1] == "--compile" && args[2] == "-o" {
        // Compile single expression to executable
        let output_file = &args[3];
        if args.len() > 4 {
            let input = &args[4];
            if input.ends_with(".slisp") || input.ends_with(".lisp") {
                // Compile file with -main function
                match compile_file_to_executable(input, output_file) {
                    Ok(()) => println!(
                        "Successfully compiled file '{}' to '{}'",
                        input, output_file
                    ),
                    Err(e) => println!("Error: {}", e),
                }
            } else {
                // Compile single expression
                match compile_to_file(input, output_file) {
                    Ok(()) => println!("Successfully compiled '{}' to '{}'", input, output_file),
                    Err(e) => println!("Error: {}", e),
                }
            }
        } else {
            // Read from stdin
            println!("Enter expression to compile:");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let expression = input.trim();

            match compile_to_file(expression, output_file) {
                Ok(()) => println!(
                    "Successfully compiled '{}' to '{}'",
                    expression, output_file
                ),
                Err(e) => println!("Error: {}", e),
            }
        }
    } else if args.len() > 1 && args[1] == "--compile" {
        println!("SLisp Compiler REPL v0.1.0");
        println!("Type expressions to compile and execute, or press Ctrl+D to quit.");
        println!();
        repl_loop(ExecutionMode::Compile);
    } else {
        println!("SLisp Interpreter REPL v0.1.0");
        println!("Type expressions to evaluate, or press Ctrl+D to quit.");
        println!("Usage:");
        println!("  slisp                          - Start interpreter REPL");
        println!("  slisp --compile                - Start compiler REPL");
        println!("  slisp --compile -o <file> [expr] - Compile expression to executable");
        println!();
        repl_loop(ExecutionMode::Interpret);
    }
}

fn compile_file_to_executable(input_file: &str, output_file: &str) -> Result<(), String> {
    // Read the file content
    let file_content = fs::read_to_string(input_file)
        .map_err(|e| format!("Failed to read file '{}': {}", input_file, e))?;

    // Parse the file to find the -main function
    let main_expr = extract_main_function(&file_content)?;

    // Compile the -main function body
    compile_to_file(&main_expr, output_file)
}

fn extract_main_function(file_content: &str) -> Result<String, String> {
    // Parse the entire file content
    let mut offset = 0;

    // Simple approach: look for (defn -main ...) pattern
    // This is a basic implementation that assumes the file contains a single -main definition

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

        // Check if this is a defn -main
        if let Node::List { root } = &ast {
            if root.len() >= 3 {
                if let (
                    Node::Symbol { value: op },
                    Node::Symbol { value: name },
                    Node::Vector { root: _params },
                ) = (root[0].as_ref(), root[1].as_ref(), root[2].as_ref())
                {
                    if op == "defn" && name == "-main" {
                        // Found -main function! Extract the body
                        if root.len() == 4 {
                            // Convert the body back to string format for compilation
                            // For now, let's assume the body is a simple expression
                            // This is a simplified approach - in practice we'd want better AST->string conversion
                            return Ok(format_ast_node(&root[3]));
                        } else {
                            return Err(
                                "-main function must have exactly one body expression".to_string()
                            );
                        }
                    }
                }
            }
        }

        // Continue to next expression
    }

    Err("No -main function found in file".to_string())
}

fn format_ast_node(node: &Node) -> String {
    // Simple AST to string conversion for the -main body
    // This is a basic implementation - a full one would be more sophisticated
    match node {
        Node::Primitive { value } => match value {
            Primitive::Number(n) => n.to_string(),
            _ => "0".to_string(), // fallback
        },
        Node::Symbol { value } => value.clone(),
        Node::List { root } => {
            let inner: Vec<String> = root.iter().map(|n| format_ast_node(n)).collect();
            format!("({})", inner.join(" "))
        }
        Node::Vector { root } => {
            let inner: Vec<String> = root.iter().map(|n| format_ast_node(n)).collect();
            format!("[{}]", inner.join(" "))
        }
    }
}

fn compile_to_file(expression: &str, output_file: &str) -> Result<(), String> {
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

    // Make executable
    match Command::new("chmod").args(["+x", output_file]).output() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to make executable: {}", e)),
    }
}

#[derive(Debug, Clone, Copy)]
enum ExecutionMode {
    Interpret,
    Compile,
}

fn repl_loop(mode: ExecutionMode) {
    loop {
        match mode {
            ExecutionMode::Interpret => print!("slisp> "),
            ExecutionMode::Compile => print!("slisp-compile> "),
        }
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF (Ctrl+D) - read_line returns 0 bytes read
                println!("Goodbye!");
                break;
            }
            Ok(_) => {
                let input = input.trim();

                if input.is_empty() {
                    continue;
                }

                match mode {
                    ExecutionMode::Interpret => match parse_and_eval(input) {
                        Ok(value) => println!("{}", format_value(&value)),
                        Err(error) => println!("Error: {}", format_error(&error)),
                    },
                    ExecutionMode::Compile => match parse_compile_and_execute(input) {
                        Ok(result) => println!("{}", result),
                        Err(error) => println!("Error: {}", error),
                    },
                }
            }
            Err(error) => {
                println!("Error reading input: {}", error);
                break;
            }
        }
    }
}

fn parse_and_eval(input: &str) -> Result<Value, EvalError> {
    match std::panic::catch_unwind(|| {
        let ast = AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0);
        eval_node(&ast)
    }) {
        Ok(result) => result,
        Err(_) => Err(EvalError::InvalidOperation(
            "Parse error: malformed expression".to_string(),
        )),
    }
}

fn parse_compile_and_execute(input: &str) -> Result<i64, String> {
    // Parse the expression
    let ast = match std::panic::catch_unwind(|| {
        AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0)
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

    // Execute using JIT
    let result = JitRunner::exec(&machine_code);
    Ok(result as i64)
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Nil => "nil".to_string(),
        Value::Function { params, .. } => {
            format!("#<function/{}>", params.len())
        }
    }
}

fn format_error(error: &EvalError) -> String {
    match error {
        EvalError::UndefinedSymbol(symbol) => format!("Undefined symbol: {}", symbol),
        EvalError::InvalidOperation(msg) => format!("Invalid operation: {}", msg),
        EvalError::ArityError(op, expected, actual) => {
            format!(
                "Arity error in '{}': expected {} arguments, got {}",
                op, expected, actual
            )
        }
        EvalError::TypeError(msg) => format!("Type error: {}", msg),
    }
}

fn format_compile_error(error: &CompileError) -> String {
    match error {
        CompileError::UnsupportedOperation(op) => format!("Unsupported operation: {}", op),
        CompileError::InvalidExpression(msg) => format!("Invalid expression: {}", msg),
        CompileError::ArityError(op, expected, actual) => {
            format!(
                "Arity error in '{}': expected {} arguments, got {}",
                op, expected, actual
            )
        }
        CompileError::UndefinedVariable(var) => format!("Undefined variable: {}", var),
    }
}
