use super::{Environment, EvalError, Value};
/// Special forms - if, let, fn, def, defn
use crate::ast::Node;

/// Evaluate if conditional
pub fn eval_if(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::ArityError("if".to_string(), 3, args.len()));
    }

    let condition = crate::evaluator::eval_with_env(&args[0], env)?;
    let is_truthy = match condition {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
        Value::Function { .. } => true, // Functions are always truthy
        Value::Keyword(_) => true,
        Value::String(s) => !s.is_empty(),
        Value::Vector(items) => !items.is_empty(),
        Value::Map(entries) => !entries.is_empty(),
    };

    if is_truthy {
        crate::evaluator::eval_with_env(&args[1], env)
    } else {
        crate::evaluator::eval_with_env(&args[2], env)
    }
}

/// Evaluate let binding expression
pub fn eval_let(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("let".to_string(), 2, args.len()));
    }

    // Bindings format: [var1 val1 var2 val2 ...]
    let bindings = match &args[0] {
        Node::Vector { root } => root,
        _ => return Err(EvalError::TypeError("let requires a vector of bindings".to_string())),
    };

    if bindings.len() % 2 != 0 {
        return Err(EvalError::TypeError("let bindings must have even number of elements".to_string()));
    }

    let mut new_env = env.clone();

    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        let var_name = match var_node {
            Node::Symbol { value } => value,
            _ => return Err(EvalError::TypeError("let binding variables must be symbols".to_string())),
        };

        // Sequential binding: later bindings can reference earlier ones
        let val = crate::evaluator::eval_with_env(val_node, &mut new_env)?;
        new_env.insert(var_name.clone(), val);
    }

    crate::evaluator::eval_with_env(&args[1], &mut new_env)
}

/// Evaluate fn (anonymous function) creation
pub fn eval_fn(args: &[Node], env: &Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("fn".to_string(), 2, args.len()));
    }

    // Parameters format: [param1 param2 ...]
    let params = match &args[0] {
        Node::Vector { root } => {
            let mut param_names = Vec::new();
            for param_node in root {
                match param_node {
                    Node::Symbol { value } => param_names.push(value.clone()),
                    _ => return Err(EvalError::TypeError("fn parameters must be symbols".to_string())),
                }
            }
            param_names
        }
        _ => return Err(EvalError::TypeError("fn requires a vector of parameters".to_string())),
    };

    let body = Box::new(args[1].clone());

    Ok(Value::Function { params, body, closure: env.clone() })
}

/// Evaluate function call
pub fn eval_function_call(func_value: Value, args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    match func_value {
        Value::Function { params, body, closure } => {
            if args.len() != params.len() {
                return Err(EvalError::ArityError("function call".to_string(), params.len(), args.len()));
            }

            let mut func_env = closure;
            for (param, arg) in params.iter().zip(args.iter()) {
                let arg_value = crate::evaluator::eval_with_env(arg, env)?;
                func_env.insert(param.clone(), arg_value);
            }

            crate::evaluator::eval_with_env(&body, &mut func_env)
        }
        _ => Err(EvalError::TypeError("Cannot call non-function value".to_string())),
    }
}

/// Evaluate def (variable definition)
pub fn eval_def(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("def".to_string(), 2, args.len()));
    }

    let _name = match &args[0] {
        Node::Symbol { value } => value,
        _ => return Err(EvalError::TypeError("def requires a symbol as first argument".to_string())),
    };

    let value = crate::evaluator::eval_with_env(&args[1], env)?;

    if let Node::Symbol { value: name } = &args[0] {
        env.insert(name.clone(), value.clone());
    }

    Ok(value)
}

/// Evaluate defn (named function definition)
pub fn eval_defn(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::ArityError("defn".to_string(), 3, args.len()));
    }

    let _name = match &args[0] {
        Node::Symbol { value } => value,
        _ => return Err(EvalError::TypeError("defn requires a symbol as first argument".to_string())),
    };

    // Parameters format: [param1 param2 ...]
    let params = match &args[1] {
        Node::Vector { root } => {
            let mut param_names = Vec::new();
            for param_node in root {
                match param_node {
                    Node::Symbol { value } => param_names.push(value.clone()),
                    _ => return Err(EvalError::TypeError("defn parameters must be symbols".to_string())),
                }
            }
            param_names
        }
        _ => return Err(EvalError::TypeError("defn requires a vector of parameters".to_string())),
    };

    let body = if args.len() == 3 {
        Box::new(args[2].clone())
    } else {
        // TODO: Multiple body expressions - wrap in an implicit do
        return Err(EvalError::InvalidOperation("Multiple body expressions not supported yet".to_string()));
    };

    let func_value = Value::Function { params, body, closure: env.clone() };

    if let Node::Symbol { value: name } = &args[0] {
        env.insert(name.clone(), func_value.clone());
    }

    Ok(func_value)
}
