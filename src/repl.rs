/// REPL (Read-Eval-Print-Loop) for both interpreter and compiler modes
use crate::allocator_trace;
use crate::ast::{AstParser, AstParserTrt};
use crate::codegen::{compile_to_executable, detect_host_target};
use crate::compiler::{compile_to_ir, CompileError};
use crate::evaluator::{eval_node, EvalError, Value};
use crate::jit_runner::JitRunner;
use std::io::{self, Write};

pub enum ExecutionMode {
    Interpret,
    Compile,
}

#[derive(Clone, Copy)]
pub struct ReplOptions {
    pub trace_allocations: bool,
}

impl Default for ReplOptions {
    fn default() -> Self {
        Self { trace_allocations: false }
    }
}

pub struct ExecutionResult {
    pub value: i64,
    pub telemetry_log: Option<String>,
}

/// Main REPL loop
pub fn repl_loop(mode: ExecutionMode, options: ReplOptions) {
    if let ExecutionMode::Compile = mode {
        allocator_trace::set_enabled(options.trace_allocations);
    }

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
                    ExecutionMode::Compile => match parse_compile_and_execute(input, &options) {
                        Ok(result) => {
                            println!("{}", result.value);
                            if let Some(report) = result.telemetry_log {
                                print!("{}", report);
                            }
                        }
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

    if let ExecutionMode::Compile = mode {
        allocator_trace::set_enabled(false);
    }
}

fn parse_and_eval(input: &str) -> Result<Value, EvalError> {
    match std::panic::catch_unwind(|| {
        let ast = AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0);
        eval_node(&ast)
    }) {
        Ok(result) => result,
        Err(_) => Err(EvalError::InvalidOperation("Parse error: malformed expression".to_string())),
    }
}

fn parse_compile_and_execute(input: &str, options: &ReplOptions) -> Result<ExecutionResult, String> {
    let ast = match std::panic::catch_unwind(|| AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0)) {
        Ok(ast) => ast,
        Err(_) => return Err("Parse error: malformed expression".to_string()),
    };

    let ir_program = match compile_to_ir(&ast) {
        Ok(program) => program,
        Err(error) => return Err(format_compile_error(&error)),
    };

    let target = detect_host_target();
    let artifact = compile_to_executable(&ir_program, target);

    if options.trace_allocations {
        allocator_trace::prepare_run();
    }

    let result = JitRunner::exec_artifact(&artifact) as i64;
    let telemetry_log = if options.trace_allocations { allocator_trace::collect_report() } else { None };

    Ok(ExecutionResult { value: result, telemetry_log })
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
            format!("Arity error in '{}': expected {} arguments, got {}", op, expected, actual)
        }
        EvalError::TypeError(msg) => format!("Type error: {}", msg),
    }
}

pub fn format_compile_error(error: &CompileError) -> String {
    match error {
        CompileError::UnsupportedOperation(op) => format!("Unsupported operation: {}", op),
        CompileError::InvalidExpression(msg) => format!("Invalid expression: {}", msg),
        CompileError::ArityError(op, expected, actual) => {
            format!("Arity error in '{}': expected {} arguments, got {}", op, expected, actual)
        }
        CompileError::UndefinedVariable(var) => format!("Undefined variable: {}", var),
        CompileError::DuplicateFunction(func) => format!("Duplicate function definition: {}", func),
    }
}
