use crate::domain::{Node, Primitive};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(isize),
    Boolean(bool),
    Nil,
    Function {
        params: Vec<String>,
        body: Box<Node>,
        closure: Environment, // Captured environment
    },
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
    eval_with_env(node, &mut Environment::new())
}

fn eval_with_env(node: &Node, env: &mut Environment) -> Result<Value, EvalError> {
    match node {
        Node::Primitive { value } => eval_primitive(value),
        Node::Symbol { value } => eval_symbol(value, env),
        Node::List { root } => eval_list(root, env),
        Node::Vector { root } => eval_vector(root, env),
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

fn eval_list(nodes: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
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
            "let" => eval_let(args, env),
            "fn" => eval_fn(args, env),
            "def" => eval_def(args, env),
            "defn" => eval_defn(args, env),
            op => {
                // Try to look up the operator as a function in the environment
                if let Some(func_value) = env.get(op) {
                    eval_function_call(func_value.clone(), args, env)
                } else {
                    Err(EvalError::UndefinedSymbol(op.to_string()))
                }
            }
        },
        _ => {
            // First element is not a symbol, might be a function expression
            let func_expr = eval_with_env(operator, env)?;
            eval_function_call(func_expr, args, env)
        }
    }
}

fn eval_arithmetic_op<F>(
    args: &[Box<Node>],
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

fn eval_if(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::ArityError("if".to_string(), 3, args.len()));
    }

    let condition = eval_with_env(&args[0], env)?;
    let is_truthy = match condition {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
        Value::Function { .. } => true, // Functions are always truthy
    };

    if is_truthy {
        eval_with_env(&args[1], env)
    } else {
        eval_with_env(&args[2], env)
    }
}

fn eval_logical_and(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Boolean(true));
    }

    for arg in args {
        let val = eval_with_env(arg, env)?;
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

fn eval_logical_or(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Boolean(false));
    }

    for arg in args {
        let val = eval_with_env(arg, env)?;
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

fn eval_logical_not(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("not".to_string(), 1, args.len()));
    }

    let val = eval_with_env(&args[0], env)?;
    let is_truthy = match val {
        Value::Boolean(b) => b,
        Value::Number(n) => n != 0,
        Value::Nil => false,
        Value::Function { .. } => true, // Functions are always truthy
    };

    Ok(Value::Boolean(!is_truthy))
}

fn eval_vector(_nodes: &[Box<Node>], _env: &Environment) -> Result<Value, EvalError> {
    // For now, vectors just evaluate to Nil - they're used as data structures in let
    Ok(Value::Nil)
}

fn eval_let(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
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
        let val = eval_with_env(val_node, &mut new_env)?;
        new_env.insert(var_name.clone(), val);
    }

    // Evaluate body in the new environment
    eval_with_env(&args[1], &mut new_env)
}

fn eval_fn(args: &[Box<Node>], env: &Environment) -> Result<Value, EvalError> {
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

fn eval_function_call(
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
                let arg_value = eval_with_env(arg, env)?;
                func_env.insert(param.clone(), arg_value);
            }

            // Evaluate function body in the new environment
            eval_with_env(&body, &mut func_env)
        }
        _ => Err(EvalError::TypeError(
            "Cannot call non-function value".to_string(),
        )),
    }
}

fn eval_def(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
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
    let value = eval_with_env(&args[1], env)?;

    // Store the binding in the environment
    if let Node::Symbol { value: name } = args[0].as_ref() {
        env.insert(name.clone(), value.clone());
    }

    Ok(value)
}

fn eval_defn(args: &[Box<Node>], env: &mut Environment) -> Result<Value, EvalError> {
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

    #[test]
    fn test_let_binding() {
        assert_eq!(parse_and_eval("(let [x 5] x)"), Ok(Value::Number(5)));
        assert_eq!(parse_and_eval("(let [x 5] (+ x 3))"), Ok(Value::Number(8)));
        assert_eq!(
            parse_and_eval("(let [x 5 y 10] (+ x y))"),
            Ok(Value::Number(15))
        );
    }

    #[test]
    fn test_let_sequential_binding() {
        // Later bindings can reference earlier ones
        assert_eq!(
            parse_and_eval("(let [x 5 y (+ x 2)] y)"),
            Ok(Value::Number(7))
        );
        assert_eq!(
            parse_and_eval("(let [x 3 y (* x 2) z (+ x y)] z)"),
            Ok(Value::Number(9))
        );
    }

    #[test]
    fn test_let_nested() {
        assert_eq!(
            parse_and_eval("(let [x 5] (let [y 10] (+ x y)))"),
            Ok(Value::Number(15))
        );
    }

    #[test]
    fn test_let_shadow_binding() {
        // Inner let should shadow outer binding
        assert_eq!(
            parse_and_eval("(let [x 5] (let [x 10] x))"),
            Ok(Value::Number(10))
        );
    }

    #[test]
    fn test_let_complex_expressions() {
        assert_eq!(
            parse_and_eval("(let [x 5 y 3] (if (> x y) (+ x y) (* x y)))"),
            Ok(Value::Number(8))
        );
    }

    #[test]
    fn test_let_error_cases() {
        // Odd number of binding elements
        assert!(matches!(
            parse_and_eval("(let [x] x)"),
            Err(EvalError::TypeError(_))
        ));

        // Wrong arity
        assert!(matches!(
            parse_and_eval("(let [x 5])"),
            Err(EvalError::ArityError(_, 2, 1))
        ));

        // Non-vector bindings
        assert!(matches!(
            parse_and_eval("(let (x 5) x)"),
            Err(EvalError::TypeError(_))
        ));

        // Non-symbol in binding
        assert!(matches!(
            parse_and_eval("(let [5 x] x)"),
            Err(EvalError::TypeError(_))
        ));
    }

    #[test]
    fn test_fn_creation() {
        // Create a simple function
        let result = parse_and_eval("(fn [x] x)");
        assert!(matches!(result, Ok(Value::Function { .. })));

        // Function with multiple parameters
        let result = parse_and_eval("(fn [x y] (+ x y))");
        assert!(matches!(result, Ok(Value::Function { .. })));

        // Function with no parameters
        let result = parse_and_eval("(fn [] 42)");
        assert!(matches!(result, Ok(Value::Function { .. })));
    }

    #[test]
    fn test_fn_call_immediate() {
        // Call function immediately
        assert_eq!(parse_and_eval("((fn [x] x) 5)"), Ok(Value::Number(5)));
        assert_eq!(
            parse_and_eval("((fn [x y] (+ x y)) 3 4)"),
            Ok(Value::Number(7))
        );
        assert_eq!(parse_and_eval("((fn [] 42))"), Ok(Value::Number(42)));
    }

    #[test]
    fn test_fn_with_let() {
        // Function that uses let binding
        assert_eq!(
            parse_and_eval("((fn [x] (let [y 10] (+ x y))) 5)"),
            Ok(Value::Number(15))
        );
    }

    #[test]
    fn test_fn_error_cases() {
        // Wrong arity in function call
        assert!(matches!(
            parse_and_eval("((fn [x] x) 1 2)"),
            Err(EvalError::ArityError(_, 1, 2))
        ));

        // Wrong arity in fn definition
        assert!(matches!(
            parse_and_eval("(fn [x])"),
            Err(EvalError::ArityError(_, 2, 1))
        ));

        // Non-vector parameters
        assert!(matches!(
            parse_and_eval("(fn (x) x)"),
            Err(EvalError::TypeError(_))
        ));

        // Non-symbol parameter
        assert!(matches!(
            parse_and_eval("(fn [5] x)"),
            Err(EvalError::TypeError(_))
        ));
    }

    #[test]
    fn test_defn_creation() {
        use super::*;
        use std::collections::HashMap;

        let mut env = HashMap::new();
        let ast = AstParser::parse_sexp_new_domain(b"(defn inc [x] (+ x 1))", &mut 0);
        let result = eval_with_env(&ast, &mut env).unwrap();

        // Should return the function value
        assert!(matches!(result, Value::Function { .. }));

        // Should be stored in environment
        assert!(env.contains_key("inc"));
        assert!(matches!(env.get("inc"), Some(Value::Function { .. })));
    }

    #[test]
    fn test_defn_and_call() {
        use super::*;
        use std::collections::HashMap;

        let mut env = HashMap::new();

        // Define function
        let ast1 = AstParser::parse_sexp_new_domain(b"(defn inc [x] (+ x 1))", &mut 0);
        eval_with_env(&ast1, &mut env).unwrap();

        // Call function
        let ast2 = AstParser::parse_sexp_new_domain(b"(inc 5)", &mut 0);
        let result = eval_with_env(&ast2, &mut env).unwrap();

        assert_eq!(result, Value::Number(6));
    }

    #[test]
    fn test_defn_multiple_params() {
        use super::*;
        use std::collections::HashMap;

        let mut env = HashMap::new();

        // Define function with multiple parameters
        let ast1 = AstParser::parse_sexp_new_domain(b"(defn add [x y] (+ x y))", &mut 0);
        eval_with_env(&ast1, &mut env).unwrap();

        // Call function
        let ast2 = AstParser::parse_sexp_new_domain(b"(add 3 4)", &mut 0);
        let result = eval_with_env(&ast2, &mut env).unwrap();

        assert_eq!(result, Value::Number(7));
    }

    #[test]
    fn test_defn_with_let() {
        use super::*;
        use std::collections::HashMap;

        let mut env = HashMap::new();

        // Define function that uses let
        let ast1 = AstParser::parse_sexp_new_domain(
            b"(defn double-plus-one [x] (let [doubled (* x 2)] (+ doubled 1)))",
            &mut 0,
        );
        eval_with_env(&ast1, &mut env).unwrap();

        // Call function
        let ast2 = AstParser::parse_sexp_new_domain(b"(double-plus-one 5)", &mut 0);
        let result = eval_with_env(&ast2, &mut env).unwrap();

        assert_eq!(result, Value::Number(11));
    }

    #[test]
    fn test_defn_error_cases() {
        use super::*;
        use std::collections::HashMap;

        let mut env = HashMap::new();

        // Wrong arity
        let ast1 = AstParser::parse_sexp_new_domain(b"(defn foo [x])", &mut 0);
        assert!(matches!(
            eval_with_env(&ast1, &mut env),
            Err(EvalError::ArityError(_, 3, 2))
        ));

        // Non-symbol name
        let ast2 = AstParser::parse_sexp_new_domain(b"(defn 123 [x] x)", &mut 0);
        assert!(matches!(
            eval_with_env(&ast2, &mut env),
            Err(EvalError::TypeError(_))
        ));

        // Non-vector parameters
        let ast3 = AstParser::parse_sexp_new_domain(b"(defn foo (x) x)", &mut 0);
        assert!(matches!(
            eval_with_env(&ast3, &mut env),
            Err(EvalError::TypeError(_))
        ));

        // Non-symbol parameter
        let ast4 = AstParser::parse_sexp_new_domain(b"(defn foo [123] x)", &mut 0);
        assert!(matches!(
            eval_with_env(&ast4, &mut env),
            Err(EvalError::TypeError(_))
        ));
    }

    #[test]
    fn test_def_variable() {
        use super::*;
        use std::collections::HashMap;

        let mut env = HashMap::new();

        // Define variable
        let ast1 = AstParser::parse_sexp_new_domain(b"(def x 42)", &mut 0);
        let result = eval_with_env(&ast1, &mut env).unwrap();

        assert_eq!(result, Value::Number(42));
        assert_eq!(env.get("x"), Some(&Value::Number(42)));

        // Use variable
        let ast2 = AstParser::parse_sexp_new_domain(b"(+ x 8)", &mut 0);
        let result2 = eval_with_env(&ast2, &mut env).unwrap();

        assert_eq!(result2, Value::Number(50));
    }
}
