mod ast_parser;
mod cli;
mod codegen;
mod compiler;
mod domain;
mod elf_gen;
mod evaluator;
mod ir;
mod jit_runner;
mod repl;

use cli::{compile_file_to_executable, compile_to_file};
use repl::{repl_loop, ExecutionMode};
use std::env;
use std::io;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 4 && args[1] == "--compile" && args[2] == "-o" {
        // Compile expression or file to executable
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
