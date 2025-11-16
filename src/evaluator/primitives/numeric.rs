use super::{Environment, EvalError, Value};
use crate::ast::Node;

pub fn eval_arithmetic_op<F>(args: &[Node], env: &mut Environment, op: F, op_name: &str) -> Result<Value, EvalError>
where
    F: Fn(isize, isize) -> isize,
{
    if args.len() < 2 {
        return Err(EvalError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let first = crate::evaluator::eval_with_env(&args[0], env)?;
    let first_num = match first {
        Value::Number(n) => n,
        _ => return Err(EvalError::TypeError(format!("{} requires numbers", op_name))),
    };

    args[1..]
        .iter()
        .try_fold(first_num, |acc, arg| {
            let val = crate::evaluator::eval_with_env(arg, env)?;
            match val {
                Value::Number(n) => Ok(op(acc, n)),
                _ => Err(EvalError::TypeError(format!("{} requires numbers", op_name))),
            }
        })
        .map(Value::Number)
}

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
        (Value::Keyword(a), Value::Keyword(b)) => a == b,
        (Value::Vector(a), Value::Vector(b)) => a == b,
        (Value::Set(a), Value::Set(b)) => a == b,
        (Value::Map(a), Value::Map(b)) => a == b,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    };

    Ok(Value::Boolean(result))
}

pub fn eval_comparison_op<F>(args: &[Node], env: &mut Environment, op: F, op_name: &str) -> Result<Value, EvalError>
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
        _ => Err(EvalError::TypeError(format!("{} requires numbers", op_name))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_requires_two_args() {
        let mut env = Environment::new();
        let args = vec![Node::Primitive { value: crate::ast::Primitive::Number(1) }];
        let err = eval_arithmetic_op(&args, &mut env, |a, b| a + b, "+").unwrap_err();
        assert!(matches!(err, EvalError::ArityError(_, _, _)));
    }

    #[test]
    fn comparison_rejects_non_numbers() {
        let mut env = Environment::new();
        let args = vec![
            Node::Primitive { value: crate::ast::Primitive::String("a".into()) },
            Node::Primitive { value: crate::ast::Primitive::Number(1) },
        ];
        let err = eval_comparison_op(&args, &mut env, |a, b| a < b, "<").unwrap_err();
        assert!(matches!(err, EvalError::TypeError(_)));
    }
}
