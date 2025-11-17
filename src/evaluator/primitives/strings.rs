use super::printer::value_to_string;
use super::{Environment, EvalError, Value};
use crate::ast::Node;

pub fn eval_str(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    args.iter()
        .try_fold(String::new(), |mut acc, arg| {
            let val = crate::evaluator::eval_with_env(arg, env)?;
            acc.push_str(&value_to_string(&val));
            Ok(acc)
        })
        .map(Value::String)
}

pub fn eval_count(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("count".to_string(), 1, args.len()));
    }

    let val = crate::evaluator::eval_with_env(&args[0], env)?;
    match val {
        Value::String(s) => Ok(Value::Number(s.len() as isize)),
        Value::Vector(items) => Ok(Value::Number(items.len() as isize)),
        Value::Set(entries) => Ok(Value::Number(entries.len() as isize)),
        Value::Map(entries) => Ok(Value::Number(entries.len() as isize)),
        Value::Nil => Ok(Value::Number(0)),
        _ => Err(EvalError::TypeError("count requires a string, vector, map, set, or nil argument".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_zero_for_nil() {
        let mut env = Environment::new();
        let args = vec![Node::Symbol { value: "n".into() }];
        env.insert("n".into(), Value::Nil);
        assert_eq!(eval_count(&args, &mut env), Ok(Value::Number(0)));
    }
}
