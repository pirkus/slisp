mod ast_parser;
mod cli;
mod codegen;
mod compiler;
mod domain;
mod evaluator;
mod ir;
mod jit_runner;
mod repl;

use cli::compile_file_to_executable;
use repl::{repl_loop, ExecutionMode};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 5 && args[1] == "--compile" && args[2] == "-o" {
        // Compile file to executable (requires .slisp/.lisp file with -main function)
        let output_file = &args[3];
        let input_file = &args[4];

        if !input_file.ends_with(".slisp") && !input_file.ends_with(".lisp") {
            println!("Error: Input must be a .slisp or .lisp file");
            println!("For quick expression testing, use the REPL: slisp --compile");
            return;
        }

        match compile_file_to_executable(input_file, output_file) {
            Ok(()) => println!(
                "Successfully compiled file '{}' to '{}'",
                input_file, output_file
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
        println!();
        println!("Usage:");
        println!("  slisp                              - Start interpreter REPL");
        println!("  slisp --compile                    - Start compiler REPL (JIT execution)");
        println!("  slisp --compile -o <out> <file>    - Compile .slisp file to executable");
        println!();
        println!(
            "Note: Executable compilation requires a .slisp/.lisp file with a -main function."
        );
        println!("      For quick expression testing, use the compiler REPL instead.");
        println!();
        repl_loop(ExecutionMode::Interpret);
    }
}
