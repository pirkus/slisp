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
use elf_gen::generate_elf_executable;
use evaluator::{eval_node, EvalError, Value};
use jit_runner::{JitRunner, JitRunnerTrt};
use std::env;
use std::io::{self, Write};
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 4 && args[1] == "--compile" && args[2] == "-o" {
        // Compile single expression to executable
        let output_file = &args[3];
        let expression = if args.len() > 4 {
            args[4].clone()
        } else {
            // Read from stdin
            println!("Enter expression to compile:");
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            input.trim().to_string()
        };

        match compile_to_file(&expression, output_file) {
            Ok(()) => println!(
                "Successfully compiled '{}' to '{}'",
                expression, output_file
            ),
            Err(e) => println!("Error: {}", e),
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
    }
}
