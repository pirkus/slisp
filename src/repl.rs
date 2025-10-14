/// REPL (Read-Eval-Print-Loop) for both interpreter and compiler modes
use crate::ast_parser::{AstParser, AstParserTrt};
use crate::codegen::compile_to_executable;
use crate::compiler::{compile_to_ir, CompileError};
use crate::evaluator::{eval_node, EvalError, Value};
use crate::jit_runner::{JitRunner, JitRunnerTrt};
use std::io::{self, Write};

pub enum ExecutionMode {
    Interpret,
    Compile,
}

/// Main REPL loop
pub fn repl_loop(mode: ExecutionMode) {
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
    let ast = match std::panic::catch_unwind(|| {
        AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0)
    }) {
        Ok(ast) => ast,
        Err(_) => return Err("Parse error: malformed expression".to_string()),
    };

    let ir_program = match compile_to_ir(&ast) {
        Ok(program) => program,
        Err(error) => return Err(format_compile_error(&error)),
    };

    let (machine_code, _heap_offset) = compile_to_executable(&ir_program);

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
        Value::String(s) => format!("\"{}\"", s),
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

pub fn format_compile_error(error: &CompileError) -> String {
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
        CompileError::DuplicateFunction(func) => format!("Duplicate function definition: {}", func),
    }
}
