/// Primitive operations - arithmetic and comparisons

use crate::domain::Node;
use super::{Value, EvalError, Environment};

/// Evaluate arithmetic operations (+, -, *, /)
pub fn eval_arithmetic_op<F>(
    args: &[Node],
    env: &mut Environment,
    op: F,
    op_name: &str,
) -> Result<Value, EvalError>
where
    F: Fn(isize, isize) -> isize,
{
    if args.len() < 2 {
        return Err(EvalError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let first = crate::evaluator::eval_with_env(&args[0], env)?;
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
            let val = crate::evaluator::eval_with_env(arg, env)?;
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

/// Evaluate comparison operations (=, <, >, <=, >=)
pub fn eval_comparison_op<F>(
    args: &[Node],
    env: &mut Environment,
    op: F,
    op_name: &str,
) -> Result<Value, EvalError>
where
    F: Fn(isize, isize) -> bool,
{
    if args.len() != 2 {
        return Err(EvalError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let left = crate::evaluator::eval_with_env(&args[0], env)?;
    let right = crate::evaluator::eval_with_env(&args[1], env)?;

    match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(Value::Boolean(op(a, b))),
        _ => Err(EvalError::TypeError(format!(
            "{} requires numbers",
            op_name
        ))),
    }
}

/// Evaluate logical AND with short-circuit evaluation
pub fn eval_logical_and(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Boolean(true));
    }

    for arg in args {
        let val = crate::evaluator::eval_with_env(arg, env)?;
        let is_truthy = match val {
            Value::Boolean(b) => b,
            Value::Number(n) => n != 0,
            Value::Nil => false,
            Value::Function { .. } => true, // Functions are always truthy
        };

        if !is_truthy {
            return Ok(Value::Boolean(false));
        }
    }

    Ok(Value::Boolean(true))
}

/// Evaluate logical OR with short-circuit evaluation
pub fn eval_logical_or(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Boolean(false));
    }

    for arg in args {
        let val = crate::evaluator::eval_with_env(arg, env)?;
        let is_truthy = match val {
            Value::Boolean(b) => b,
            Value::Number(n) => n != 0,
            Value::Nil => false,
            Value::Function { .. } => true, // Functions are always truthy
        };

        if is_truthy {
            return Ok(Value::Boolean(true));
        }
    }

    Ok(Value::Boolean(false))
}

/// Evaluate logical NOT
pub fn eval_logical_not(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("not".to_string(), 1, args.len()));
    }

    let val = crate::evaluator::eval_with_env(&args[0], env)?;
    let is_truthy = match val {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
        Value::Function { .. } => true, // Functions are always truthy
    };

    Ok(Value::Boolean(!is_truthy))
}
