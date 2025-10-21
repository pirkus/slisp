mod allocator_trace;
mod ast;
mod cli;
mod codegen;
mod compiler;
mod evaluator;
mod ir;
mod jit_runner;
mod repl;

use cli::compile_file_to_executable;
use repl::{repl_loop, ExecutionMode, ReplOptions};
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut compile_mode = false;
    let mut keep_object = false;
    let mut output_file: Option<String> = None;
    let mut input_file: Option<String> = None;
    let mut trace_alloc = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--compile" => {
                compile_mode = true;
                i += 1;
            }
            "--keep-obj" => {
                keep_object = true;
                i += 1;
            }
            "--trace-alloc" => {
                trace_alloc = true;
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

    if trace_alloc && !allocator_trace::is_available() {
        eprintln!("Warning: allocator telemetry requested but this build lacks '--features allocator-telemetry'. Ignoring --trace-alloc.");
        trace_alloc = false;
    }

    if compile_mode {
        if let (Some(output), Some(input)) = (output_file.as_deref(), input_file.as_deref()) {
            if !input.ends_with(".slisp") && !input.ends_with(".lisp") {
                println!("Error: Input must be a .slisp or .lisp file");
                println!("For quick expression testing, use the REPL: slisp --compile");
                return;
            }

            match compile_file_to_executable(input, output, keep_object, trace_alloc) {
                Ok(()) => println!("Successfully compiled file '{}' to '{}'", input, output),
                Err(e) => println!("Error: {}", e),
            }
            return;
        }

        println!("SLisp Compiler REPL v0.1.0");
        println!("Type expressions to compile and execute, or press Ctrl+D to quit.");
        println!();
        let options = ReplOptions { trace_allocations: trace_alloc };
        repl_loop(ExecutionMode::Compile, options);
    } else {
        if trace_alloc {
            eprintln!("Warning: --trace-alloc is only supported with '--compile'. Ignoring flag.");
        }

        println!("SLisp Interpreter REPL v0.1.0");
        println!("Type expressions to evaluate, or press Ctrl+D to quit.");
        println!();
        println!("Usage:");
        println!("  slisp                              - Start interpreter REPL");
        println!("  slisp --compile                    - Start compiler REPL (JIT execution)");
        println!("  slisp --compile [--keep-obj] [-o <out>] <file>  - Compile .slisp file to executable");
        println!("  slisp --compile --trace-alloc      - Enable allocator telemetry (REPL or compiled output)");
        println!();
        println!("Note: Executable compilation requires a .slisp/.lisp file with a -main function.");
        println!("      For quick expression testing, use the compiler REPL instead.");
        println!();
        repl_loop(ExecutionMode::Interpret, ReplOptions::default());
    }
}
