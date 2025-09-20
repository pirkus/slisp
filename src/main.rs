mod ast_parser;
mod domain;
mod evaluator;
mod jit_runner;

use ast_parser::{AstParser, AstParserTrt};
use evaluator::{eval_node, EvalError, Value};
use std::io::{self, Write};

fn main() {
    println!("SLisp REPL v0.1.0");
    println!("Type expressions to evaluate, or press Ctrl+D to quit.");
    println!();

    loop {
        print!("slisp> ");
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

                match parse_and_eval(input) {
                    Ok(value) => println!("{}", format_value(&value)),
                    Err(error) => println!("Error: {}", format_error(&error)),
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
