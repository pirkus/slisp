/// Special forms - if, let, fn, def, defn

use crate::domain::Node;
use super::{Value, EvalError, Environment};

/// Evaluate if conditional
pub fn eval_if(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::ArityError("if".to_string(), 3, args.len()));
    }

    let condition = crate::evaluator::eval_with_env(&args[0], env)?;
    let is_truthy = match condition {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
        Value::Function { .. } => true, // Functions are always truthy
    };

    if is_truthy {
        crate::evaluator::eval_with_env(&args[1], env)
    } else {
        crate::evaluator::eval_with_env(&args[2], env)
    }
}

/// Evaluate let binding expression
pub fn eval_let(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("let".to_string(), 2, args.len()));
    }

    // First argument should be a vector of bindings [var1 val1 var2 val2 ...]
    let bindings = match args[0].as_ref() {
        Node::Vector { root } => root,
        _ => {
            return Err(EvalError::TypeError(
                "let requires a vector of bindings".to_string(),
            ))
        }
    };

    // Check that we have an even number of binding elements
    if bindings.len() % 2 != 0 {
        return Err(EvalError::TypeError(
            "let bindings must have even number of elements".to_string(),
        ));
    }

    // Create new environment with bindings
    let mut new_env = env.clone();

    // Process bindings in pairs [var val var val ...]
    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        // Variable must be a symbol
        let var_name = match var_node.as_ref() {
            Node::Symbol { value } => value,
            _ => {
                return Err(EvalError::TypeError(
                    "let binding variables must be symbols".to_string(),
                ))
            }
        };

        // Evaluate the value in the current environment (not new_env)
        // This allows for sequential binding where later bindings can reference earlier ones
        let val = crate::evaluator::eval_with_env(val_node, &mut new_env)?;
        new_env.insert(var_name.clone(), val);
    }

    // Evaluate body in the new environment
    crate::evaluator::eval_with_env(&args[1], &mut new_env)
}

/// Evaluate fn (anonymous function) creation
pub fn eval_fn(args: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("fn".to_string(), 2, args.len()));
    }

    // First argument should be a vector of parameters [param1 param2 ...]
    let params = match args[0].as_ref() {
        Node::Vector { root } => {
            let mut param_names = Vec::new();
            for param_node in root {
                match param_node.as_ref() {
                    Node::Symbol { value } => param_names.push(value.clone()),
                    _ => {
                        return Err(EvalError::TypeError(
                            "fn parameters must be symbols".to_string(),
                        ))
                    }
                }
            }
            param_names
        }
        _ => {
            return Err(EvalError::TypeError(
                "fn requires a vector of parameters".to_string(),
            ))
        }
    };

    // Second argument is the function body
    let body = args[1].clone();

    // Create function value with captured environment (closure)
    Ok(Value::Function {
        params,
        body,
        closure: env.clone(),
    })
}

/// Evaluate function call
pub fn eval_function_call(
    func_value: Value,
    args: &[Box<Node>],
    env: &mut Environment,
) -> Result<Value, EvalError> {
    match func_value {
        Value::Function {
            params,
            body,
            closure,
        } => {
            // Check arity
            if args.len() != params.len() {
                return Err(EvalError::ArityError(
                    "function call".to_string(),
                    params.len(),
                    args.len(),
                ));
            }

            // Create new environment from closure + parameter bindings
            let mut func_env = closure;
            for (param, arg) in params.iter().zip(args.iter()) {
                let arg_value = crate::evaluator::eval_with_env(arg, env)?;
                func_env.insert(param.clone(), arg_value);
            }

            // Evaluate function body in the new environment
            crate::evaluator::eval_with_env(&body, &mut func_env)
        }
        _ => Err(EvalError::TypeError(
            "Cannot call non-function value".to_string(),
        )),
    }
}

/// Evaluate def (variable definition)
pub fn eval_def(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("def".to_string(), 2, args.len()));
    }

    // First argument must be a symbol (the name)
    let _name = match args[0].as_ref() {
        Node::Symbol { value } => value,
        _ => {
            return Err(EvalError::TypeError(
                "def requires a symbol as first argument".to_string(),
            ))
        }
    };

    // Second argument is the value
    let value = crate::evaluator::eval_with_env(&args[1], env)?;

    // Store the binding in the environment
    if let Node::Symbol { value: name } = args[0].as_ref() {
        env.insert(name.clone(), value.clone());
    }

    Ok(value)
}

/// Evaluate defn (named function definition)
pub fn eval_defn(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::ArityError("defn".to_string(), 3, args.len()));
    }

    // First argument must be a symbol (the function name)
    let _name = match args[0].as_ref() {
        Node::Symbol { value } => value,
        _ => {
            return Err(EvalError::TypeError(
                "defn requires a symbol as first argument".to_string(),
            ))
        }
    };

    // Second argument should be a vector of parameters
    let params = match args[1].as_ref() {
        Node::Vector { root } => {
            let mut param_names = Vec::new();
            for param_node in root {
                match param_node.as_ref() {
                    Node::Symbol { value } => param_names.push(value.clone()),
                    _ => {
                        return Err(EvalError::TypeError(
                            "defn parameters must be symbols".to_string(),
                        ))
                    }
                }
            }
            param_names
        }
        _ => {
            return Err(EvalError::TypeError(
                "defn requires a vector of parameters".to_string(),
            ))
        }
    };

    // Rest of arguments form the function body (for now, just take the first one)
    let body = if args.len() == 3 {
        args[2].clone()
    } else {
        // Multiple body expressions - wrap in an implicit do (not implemented yet)
        return Err(EvalError::InvalidOperation(
            "Multiple body expressions not supported yet".to_string(),
        ));
    };

    // Create function value
    let func_value = Value::Function {
        params,
        body,
        closure: env.clone(),
    };

    // Store the function in the environment
    if let Node::Symbol { value: name } = args[0].as_ref() {
        env.insert(name.clone(), func_value.clone());
    }

    Ok(func_value)
}
