use super::{Environment, EvalError, Value};
/// Primitive operations - arithmetic and comparisons
use crate::ast::Node;

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

/// Evaluate equality comparison (supports multiple types)
pub fn eval_equal(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("=".to_string(), 2, args.len()));
    }

    let left = crate::evaluator::eval_with_env(&args[0], env)?;
    let right = crate::evaluator::eval_with_env(&args[1], env)?;

    let result = match (left, right) {
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Nil, Value::Nil) => true,
        _ => false, // Different types are not equal
    };

    Ok(Value::Boolean(result))
}

/// Evaluate comparison operations (<, >, <=, >=)
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

        if !is_truthy(&val) {
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

        if is_truthy(&val) {
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

    Ok(Value::Boolean(!is_truthy(&val)))
}

fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Boolean(b) => *b,
        Value::Number(n) => *n != 0,
        Value::Nil => false,
        Value::Function { .. } => true, // Functions are always truthy
        Value::String(s) => !s.is_empty(),
    }
}

/// str - String concatenation (Clojure-style)
/// Converts arguments to strings and concatenates them
pub fn eval_str(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut result = String::new();

    for arg in args {
        let val = crate::evaluator::eval_with_env(arg, env)?;
        match val {
            Value::String(s) => result.push_str(&s),
            Value::Number(n) => result.push_str(&n.to_string()),
            Value::Boolean(b) => result.push_str(if b { "true" } else { "false" }),
            Value::Nil => result.push_str("nil"),
            Value::Function { .. } => result.push_str("#<function>"),
        }
    }

    Ok(Value::String(result))
}

/// count - Returns the length of a string or collection
pub fn eval_count(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("count".to_string(), 1, args.len()));
    }

    let val = crate::evaluator::eval_with_env(&args[0], env)?;
    match val {
        Value::String(s) => Ok(Value::Number(s.len() as isize)),
        _ => Err(EvalError::TypeError(
            "count requires a string argument".to_string(),
        )),
    }
}

/// get - Get character at index (returns string or nil)
pub fn eval_get(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::ArityError("get".to_string(), 2, args.len()));
    }

    let string_val = crate::evaluator::eval_with_env(&args[0], env)?;
    let index_val = crate::evaluator::eval_with_env(&args[1], env)?;

    match (string_val, index_val) {
        (Value::String(s), Value::Number(idx)) => {
            if idx < 0 || idx >= s.len() as isize {
                // Return nil for out of bounds (Clojure style)
                return Ok(Value::Nil);
            }

            let idx = idx as usize;
            let ch = s.chars().nth(idx).unwrap();
            Ok(Value::String(ch.to_string()))
        }
        (Value::String(_), _) => Err(EvalError::TypeError(
            "get: index must be a number".to_string(),
        )),
        _ => Err(EvalError::TypeError(
            "get: first argument must be a string".to_string(),
        )),
    }
}

/// subs - Extract substring (start, end)
pub fn eval_subs(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::ArityError("subs".to_string(), 2, args.len()));
    }

    let string_val = crate::evaluator::eval_with_env(&args[0], env)?;
    let start_val = crate::evaluator::eval_with_env(&args[1], env)?;

    match string_val {
        Value::String(s) => {
            let start = match start_val {
                Value::Number(n) => {
                    if n < 0 {
                        return Err(EvalError::InvalidOperation(
                            "subs: start index cannot be negative".to_string(),
                        ));
                    }
                    n as usize
                }
                _ => {
                    return Err(EvalError::TypeError(
                        "subs: start index must be a number".to_string(),
                    ))
                }
            };

            let end = if args.len() == 3 {
                let end_val = crate::evaluator::eval_with_env(&args[2], env)?;
                match end_val {
                    Value::Number(n) => {
                        if n < 0 {
                            return Err(EvalError::InvalidOperation(
                                "subs: end index cannot be negative".to_string(),
                            ));
                        }
                        n as usize
                    }
                    _ => {
                        return Err(EvalError::TypeError(
                            "subs: end index must be a number".to_string(),
                        ))
                    }
                }
            } else {
                s.len()
            };

            if start > s.len() {
                return Err(EvalError::InvalidOperation(format!(
                    "subs: start index {} out of bounds for string of length {}",
                    start,
                    s.len()
                )));
            }

            if end > s.len() {
                return Err(EvalError::InvalidOperation(format!(
                    "subs: end index {} out of bounds for string of length {}",
                    end,
                    s.len()
                )));
            }

            if start > end {
                return Err(EvalError::InvalidOperation(format!(
                    "subs: start index {} is greater than end index {}",
                    start, end
                )));
            }

            let substring: String = s.chars().skip(start).take(end - start).collect();
            Ok(Value::String(substring))
        }
        _ => Err(EvalError::TypeError(
            "subs: first argument must be a string".to_string(),
        )),
    }
}
