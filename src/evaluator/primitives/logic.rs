use super::{Environment, EvalError, Value};
use crate::ast::Node;
use crate::evaluator::helpers::is_truthy;

pub fn eval_logical_and(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    fn all_truthy(nodes: &[Node], env: &mut Environment) -> Result<bool, EvalError> {
        match nodes.split_first() {
            None => Ok(true),
            Some((node, rest)) => {
                let value = crate::evaluator::eval_with_env(node, env)?;
                if is_truthy(&value) {
                    all_truthy(rest, env)
                } else {
                    Ok(false)
                }
            }
        }
    }

    if args.is_empty() {
        Ok(Value::Boolean(true))
    } else {
        all_truthy(args, env).map(Value::Boolean)
    }
}

pub fn eval_logical_or(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    fn any_truthy(nodes: &[Node], env: &mut Environment) -> Result<bool, EvalError> {
        match nodes.split_first() {
            None => Ok(false),
            Some((node, rest)) => {
                let value = crate::evaluator::eval_with_env(node, env)?;
                if is_truthy(&value) {
                    Ok(true)
                } else {
                    any_truthy(rest, env)
                }
            }
        }
    }

    if args.is_empty() {
        Ok(Value::Boolean(false))
    } else {
        any_truthy(args, env).map(Value::Boolean)
    }
}

pub fn eval_logical_not(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("not".to_string(), 1, args.len()));
    }

    let val = crate::evaluator::eval_with_env(&args[0], env)?;
    Ok(Value::Boolean(!is_truthy(&val)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_and_short_circuits_on_falsey() {
        let mut env = Environment::new();
        let args = vec![
            Node::Primitive {
                value: crate::ast::Primitive::Number(1),
            },
            Node::Primitive {
                value: crate::ast::Primitive::Number(0),
            },
            Node::Primitive {
                value: crate::ast::Primitive::Number(1),
            },
        ];
        assert_eq!(eval_logical_and(&args, &mut env), Ok(Value::Boolean(false)));
    }

    #[test]
    fn logical_or_handles_empty() {
        let mut env = Environment::new();
        let args = Vec::new();
        assert_eq!(eval_logical_or(&args, &mut env), Ok(Value::Boolean(false)));
    }
}
