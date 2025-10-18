mod ast;
mod cli;
mod codegen;
mod compiler;
mod evaluator;
mod ir;
mod jit_runner;
mod repl;

use cli::compile_file_to_executable;
use repl::{repl_loop, ExecutionMode};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--compile" {
        let mut keep_object = false;
        let mut output_file: Option<String> = None;
        let mut input_file: Option<String> = None;

        let mut i = 2;
        while i < args.len() {
            match args[i].as_str() {
                "--keep-obj" => {
                    keep_object = true;
                    i += 1;
                }
                "-o" => {
                    if i + 1 >= args.len() {
                        println!("Error: -o requires an output path");
                        return;
                    }
                    output_file = Some(args[i + 1].clone());
                    i += 2;
                }
                value => {
                    if input_file.is_none() {
                        input_file = Some(value.to_string());
                    } else {
                        println!("Error: multiple input files provided");
                        return;
                    }
                    i += 1;
                }
            }
        }

        if let (Some(output), Some(input)) = (output_file, input_file) {
            if !input.ends_with(".slisp") && !input.ends_with(".lisp") {
                println!("Error: Input must be a .slisp or .lisp file");
                println!("For quick expression testing, use the REPL: slisp --compile");
                return;
            }

            match compile_file_to_executable(&input, &output, keep_object) {
                Ok(()) => println!("Successfully compiled file '{}' to '{}'", input, output),
                Err(e) => println!("Error: {}", e),
            }
            return;
        }

        println!("SLisp Compiler REPL v0.1.0");
        println!("Type expressions to compile and execute, or press Ctrl+D to quit.");
        println!();
        repl_loop(ExecutionMode::Compile);
    } else {
        println!("SLisp Interpreter REPL v0.1.0");
        println!("Type expressions to evaluate, or press Ctrl+D to quit.");
        println!();
        println!("Usage:");
        println!("  slisp                              - Start interpreter REPL");
        println!("  slisp --compile                    - Start compiler REPL (JIT execution)");
        println!("  slisp --compile [--keep-obj] -o <out> <file>    - Compile .slisp file to executable");
        println!();
        println!("Note: Executable compilation requires a .slisp/.lisp file with a -main function.");
        println!("      For quick expression testing, use the compiler REPL instead.");
        println!();
        repl_loop(ExecutionMode::Interpret);
    }
}
