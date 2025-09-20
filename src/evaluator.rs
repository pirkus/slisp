use crate::domain::{Node, Primitive};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(isize),
    Boolean(bool),
    Nil,
}

#[derive(Debug, PartialEq)]
pub enum EvalError {
    UndefinedSymbol(String),
    InvalidOperation(String),
    ArityError(String, usize, usize), // operation, expected, actual
    TypeError(String),
}

type Environment = HashMap<String, Value>;

pub fn eval_node(node: &Node) -> Result<Value, EvalError> {
    eval_with_env(node, &Environment::new())
}

fn eval_with_env(node: &Node, env: &Environment) -> Result<Value, EvalError> {
    match node {
        Node::Primitive { value } => eval_primitive(value),
        Node::Symbol { value } => eval_symbol(value, env),
        Node::List { root } => eval_list(root, env),
    }
}

fn eval_primitive(primitive: &Primitive) -> Result<Value, EvalError> {
    match primitive {
        Primitive::Number(n) => Ok(Value::Number(*n as isize)),
        Primitive::_Str(_) => Err(EvalError::TypeError(
            "String literals not supported yet".to_string(),
        )),
    }
}

fn eval_symbol(symbol: &str, env: &Environment) -> Result<Value, EvalError> {
    env.get(symbol)
        .cloned()
        .ok_or_else(|| EvalError::UndefinedSymbol(symbol.to_string()))
}

fn eval_list(nodes: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
    if nodes.is_empty() {
        return Ok(Value::Nil);
    }

    let operator = &nodes[0];
    let args = &nodes[1..];

    match operator.as_ref() {
        Node::Symbol { value } => match value.as_str() {
            "+" => eval_arithmetic_op(args, env, |a, b| a + b, "+"),
            "-" => eval_arithmetic_op(args, env, |a, b| a - b, "-"),
            "*" => eval_arithmetic_op(args, env, |a, b| a * b, "*"),
            "/" => eval_arithmetic_op(
                args,
                env,
                |a, b| {
                    if b == 0 {
                        panic!("Division by zero")
                    } else {
                        a / b
                    }
                },
                "/",
            ),
            "=" => eval_comparison_op(args, env, |a, b| a == b, "="),
            "<" => eval_comparison_op(args, env, |a, b| a < b, "<"),
            ">" => eval_comparison_op(args, env, |a, b| a > b, ">"),
            "<=" => eval_comparison_op(args, env, |a, b| a <= b, "<="),
            ">=" => eval_comparison_op(args, env, |a, b| a >= b, ">="),
            "if" => eval_if(args, env),
            "and" => eval_logical_and(args, env),
            "or" => eval_logical_or(args, env),
            "not" => eval_logical_not(args, env),
            op => Err(EvalError::UndefinedSymbol(op.to_string())),
        },
        _ => Err(EvalError::InvalidOperation(
            "First element must be a symbol".to_string(),
        )),
    }
}

fn eval_arithmetic_op<F>(
    args: &[Box<Node>],
    env: &Environment,
    op: F,
    op_name: &str,
) -> Result<Value, EvalError>
where
    F: Fn(isize, isize) -> isize,
{
    if args.len() < 2 {
        return Err(EvalError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let first = eval_with_env(&args[0], env)?;
    let first_num = match first {
        Value::Number(n) => n,
        _ => {
            return Err(EvalError::TypeError(format!(
                "{} requires numbers",
                op_name
            )))
        }
    };

    args[1..]
        .iter()
        .try_fold(first_num, |acc, arg| {
            let val = eval_with_env(arg, env)?;
            match val {
                Value::Number(n) => Ok(op(acc, n)),
                _ => Err(EvalError::TypeError(format!(
                    "{} requires numbers",
                    op_name
                ))),
            }
        })
        .map(Value::Number)
}

fn eval_comparison_op<F>(
    args: &[Box<Node>],
    env: &Environment,
    op: F,
    op_name: &str,
) -> Result<Value, EvalError>
where
    F: Fn(isize, isize) -> bool,
{
    if args.len() != 2 {
        return Err(EvalError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let left = eval_with_env(&args[0], env)?;
    let right = eval_with_env(&args[1], env)?;

    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Boolean(op(a, b))),
        _ => Err(EvalError::TypeError(format!(
            "{} requires numbers",
            op_name
        ))),
    }
}

fn eval_if(args: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::ArityError("if".to_string(), 3, args.len()));
    }

    let condition = eval_with_env(&args[0], env)?;
    let is_truthy = match condition {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
    };

    if is_truthy {
        eval_with_env(&args[1], env)
    } else {
        eval_with_env(&args[2], env)
    }
}

fn eval_logical_and(args: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Boolean(true));
    }

    for arg in args {
        let val = eval_with_env(arg, env)?;
        let is_truthy = match val {
            Value::Boolean(b) => b,
            Value::Number(n) => n != 0,
            Value::Nil => false,
        };

        if !is_truthy {
            return Ok(Value::Boolean(false));
        }
    }

    Ok(Value::Boolean(true))
}

fn eval_logical_or(args: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Boolean(false));
    }

    for arg in args {
        let val = eval_with_env(arg, env)?;
        let is_truthy = match val {
            Value::Boolean(b) => b,
            Value::Number(n) => n != 0,
            Value::Nil => false,
        };

        if is_truthy {
            return Ok(Value::Boolean(true));
        }
    }

    Ok(Value::Boolean(false))
}

fn eval_logical_not(args: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("not".to_string(), 1, args.len()));
    }

    let val = eval_with_env(&args[0], env)?;
    let is_truthy = match val {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
    };

    Ok(Value::Boolean(!is_truthy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_parser::{AstParser, AstParserTrt};

    fn parse_and_eval(input: &str) -> Result<Value, EvalError> {
        let ast = AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0);
        eval_node(&ast)
    }

    #[test]
    fn test_arithmetic_operations() {
        assert_eq!(parse_and_eval("(+ 2 3)"), Ok(Value::Number(5)));
        assert_eq!(parse_and_eval("(- 10 4)"), Ok(Value::Number(6)));
        assert_eq!(parse_and_eval("(* 3 4)"), Ok(Value::Number(12)));
        assert_eq!(parse_and_eval("(/ 8 2)"), Ok(Value::Number(4)));
    }

    #[test]
    fn test_nested_arithmetic() {
        assert_eq!(parse_and_eval("(+ 2 (* 3 4))"), Ok(Value::Number(14)));
        assert_eq!(parse_and_eval("(* (+ 1 2) (- 5 3))"), Ok(Value::Number(6)));
        assert_eq!(parse_and_eval("(+ (+ 1 2) (+ 3 4))"), Ok(Value::Number(10)));
    }

    #[test]
    fn test_multi_operand_arithmetic() {
        assert_eq!(parse_and_eval("(+ 1 2 3 4)"), Ok(Value::Number(10)));
        assert_eq!(parse_and_eval("(* 2 3 4)"), Ok(Value::Number(24)));
        assert_eq!(parse_and_eval("(- 20 3 2)"), Ok(Value::Number(15)));
    }

    #[test]
    fn test_comparison_operations() {
        assert_eq!(parse_and_eval("(= 5 5)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(= 5 3)"), Ok(Value::Boolean(false)));
        assert_eq!(parse_and_eval("(< 3 5)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(> 5 3)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(<= 3 3)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(>= 5 3)"), Ok(Value::Boolean(true)));
    }

    #[test]
    fn test_logical_operations() {
        assert_eq!(parse_and_eval("(and 1 2)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(and 1 0)"), Ok(Value::Boolean(false)));
        assert_eq!(parse_and_eval("(or 0 2)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(or 0 0)"), Ok(Value::Boolean(false)));
        assert_eq!(parse_and_eval("(not 0)"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("(not 1)"), Ok(Value::Boolean(false)));
    }

    #[test]
    fn test_if_conditional() {
        assert_eq!(parse_and_eval("(if 1 42 0)"), Ok(Value::Number(42)));
        assert_eq!(parse_and_eval("(if 0 42 24)"), Ok(Value::Number(24)));
        assert_eq!(parse_and_eval("(if (> 5 3) 1 0)"), Ok(Value::Number(1)));
        assert_eq!(parse_and_eval("(if (< 5 3) 1 0)"), Ok(Value::Number(0)));
    }

    #[test]
    fn test_complex_expressions() {
        assert_eq!(
            parse_and_eval("(if (and (> 10 5) (< 3 8)) (+ 2 3) (* 2 4))"),
            Ok(Value::Number(5))
        );

        assert_eq!(
            parse_and_eval("(+ (* 2 3) (if (= 1 1) 4 0))"),
            Ok(Value::Number(10))
        );
    }

    #[test]
    fn test_error_cases() {
        assert!(matches!(
            parse_and_eval("(+ 1)"),
            Err(EvalError::ArityError(_, 2, 1))
        ));
        assert!(matches!(
            parse_and_eval("(unknown 1 2)"),
            Err(EvalError::UndefinedSymbol(_))
        ));
        assert!(matches!(
            parse_and_eval("(if 1 2)"),
            Err(EvalError::ArityError(_, 3, 2))
        ));
        assert!(matches!(
            parse_and_eval("(not 1 2)"),
            Err(EvalError::ArityError(_, 1, 2))
        ));
    }

    #[test]
    fn test_primitives() {
        assert_eq!(parse_and_eval("42"), Ok(Value::Number(42)));
        assert_eq!(parse_and_eval("0"), Ok(Value::Number(0)));
    }

    #[test]
    fn test_empty_list() {
        let empty_list = Node::List { root: vec![] };
        assert_eq!(eval_node(&empty_list), Ok(Value::Nil));
    }
}
