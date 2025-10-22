/// Evaluator module - interprets AST nodes
///
/// This module is organized into:
/// - primitives: Arithmetic, comparison, and logical operations
/// - special_forms: Special forms (if, let, fn, def, defn)
mod primitives;
mod special_forms;

use crate::ast::{Node, Primitive};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(isize),
    Boolean(bool),
    String(String),
    Vector(Vec<Value>),
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

pub type Environment = HashMap<String, Value>;

/// Evaluate a node with a fresh environment
pub fn eval_node(node: &Node) -> Result<Value, EvalError> {
    eval_with_env(node, &mut Environment::new())
}

/// Evaluate a node with the given environment
pub(crate) fn eval_with_env(node: &Node, env: &mut Environment) -> Result<Value, EvalError> {
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
        Primitive::Boolean(b) => Ok(Value::Boolean(*b)),
        Primitive::String(s) => Ok(Value::String(s.clone())),
    }
}

fn eval_symbol(symbol: &str, env: &Environment) -> Result<Value, EvalError> {
    env.get(symbol).cloned().ok_or_else(|| EvalError::UndefinedSymbol(symbol.to_string()))
}

fn eval_list(nodes: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if nodes.is_empty() {
        return Ok(Value::Nil);
    }

    let operator = &nodes[0];
    let args = &nodes[1..];

    match operator {
        Node::Symbol { value } => match value.as_str() {
            "+" => primitives::eval_arithmetic_op(args, env, |a, b| a + b, "+"),
            "-" => primitives::eval_arithmetic_op(args, env, |a, b| a - b, "-"),
            "*" => primitives::eval_arithmetic_op(args, env, |a, b| a * b, "*"),
            "/" => primitives::eval_arithmetic_op(
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
            "=" => primitives::eval_equal(args, env),
            "<" => primitives::eval_comparison_op(args, env, |a, b| a < b, "<"),
            ">" => primitives::eval_comparison_op(args, env, |a, b| a > b, ">"),
            "<=" => primitives::eval_comparison_op(args, env, |a, b| a <= b, "<="),
            ">=" => primitives::eval_comparison_op(args, env, |a, b| a >= b, ">="),
            "if" => special_forms::eval_if(args, env),
            "and" => primitives::eval_logical_and(args, env),
            "or" => primitives::eval_logical_or(args, env),
            "not" => primitives::eval_logical_not(args, env),
            "let" => special_forms::eval_let(args, env),
            "fn" => special_forms::eval_fn(args, env),
            "def" => special_forms::eval_def(args, env),
            "defn" => special_forms::eval_defn(args, env),
            "str" => primitives::eval_str(args, env),
            "count" => primitives::eval_count(args, env),
            "get" => primitives::eval_get(args, env),
            "subs" => primitives::eval_subs(args, env),
            "vec" => primitives::eval_vec(args, env),
            op => {
                if let Some(func_value) = env.get(op) {
                    special_forms::eval_function_call(func_value.clone(), args, env)
                } else {
                    Err(EvalError::UndefinedSymbol(op.to_string()))
                }
            }
        },
        _ => {
            let func_expr = eval_with_env(operator, env)?;
            special_forms::eval_function_call(func_expr, args, env)
        }
    }
}

fn eval_vector(nodes: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut values = Vec::with_capacity(nodes.len());
    for node in nodes {
        values.push(eval_with_env(node, env)?);
    }
    Ok(Value::Vector(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstParser, AstParserTrt};

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
        assert_eq!(parse_and_eval("(if (and (> 10 5) (< 3 8)) (+ 2 3) (* 2 4))"), Ok(Value::Number(5)));

        assert_eq!(parse_and_eval("(+ (* 2 3) (if (= 1 1) 4 0))"), Ok(Value::Number(10)));
    }

    #[test]
    fn test_error_cases() {
        assert!(matches!(parse_and_eval("(+ 1)"), Err(EvalError::ArityError(_, 2, 1))));
        assert!(matches!(parse_and_eval("(unknown 1 2)"), Err(EvalError::UndefinedSymbol(_))));
        assert!(matches!(parse_and_eval("(if 1 2)"), Err(EvalError::ArityError(_, 3, 2))));
        assert!(matches!(parse_and_eval("(not 1 2)"), Err(EvalError::ArityError(_, 1, 2))));
    }

    #[test]
    fn test_primitives() {
        assert_eq!(parse_and_eval("42"), Ok(Value::Number(42)));
        assert_eq!(parse_and_eval("0"), Ok(Value::Number(0)));
        assert_eq!(parse_and_eval("true"), Ok(Value::Boolean(true)));
        assert_eq!(parse_and_eval("false"), Ok(Value::Boolean(false)));
        assert_eq!(parse_and_eval("\"hello\""), Ok(Value::String("hello".to_string())));
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
        assert_eq!(parse_and_eval("(let [x 5 y 10] (+ x y))"), Ok(Value::Number(15)));
    }

    #[test]
    fn test_let_sequential_binding() {
        // Later bindings can reference earlier ones
        assert_eq!(parse_and_eval("(let [x 5 y (+ x 2)] y)"), Ok(Value::Number(7)));
        assert_eq!(parse_and_eval("(let [x 3 y (* x 2) z (+ x y)] z)"), Ok(Value::Number(9)));
    }

    #[test]
    fn test_let_nested() {
        assert_eq!(parse_and_eval("(let [x 5] (let [y 10] (+ x y)))"), Ok(Value::Number(15)));
    }

    #[test]
    fn test_let_shadow_binding() {
        // Inner let should shadow outer binding
        assert_eq!(parse_and_eval("(let [x 5] (let [x 10] x))"), Ok(Value::Number(10)));
    }

    #[test]
    fn test_let_complex_expressions() {
        assert_eq!(parse_and_eval("(let [x 5 y 3] (if (> x y) (+ x y) (* x y)))"), Ok(Value::Number(8)));
    }

    #[test]
    fn test_let_error_cases() {
        // Odd number of binding elements
        assert!(matches!(parse_and_eval("(let [x] x)"), Err(EvalError::TypeError(_))));

        // Wrong arity
        assert!(matches!(parse_and_eval("(let [x 5])"), Err(EvalError::ArityError(_, 2, 1))));

        // Non-vector bindings
        assert!(matches!(parse_and_eval("(let (x 5) x)"), Err(EvalError::TypeError(_))));

        // Non-symbol in binding
        assert!(matches!(parse_and_eval("(let [5 x] x)"), Err(EvalError::TypeError(_))));
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
        assert_eq!(parse_and_eval("((fn [x y] (+ x y)) 3 4)"), Ok(Value::Number(7)));
        assert_eq!(parse_and_eval("((fn [] 42))"), Ok(Value::Number(42)));
    }

    #[test]
    fn test_fn_with_let() {
        // Function that uses let binding
        assert_eq!(parse_and_eval("((fn [x] (let [y 10] (+ x y))) 5)"), Ok(Value::Number(15)));
    }

    #[test]
    fn test_fn_error_cases() {
        // Wrong arity in function call
        assert!(matches!(parse_and_eval("((fn [x] x) 1 2)"), Err(EvalError::ArityError(_, 1, 2))));

        // Wrong arity in fn definition
        assert!(matches!(parse_and_eval("(fn [x])"), Err(EvalError::ArityError(_, 2, 1))));

        // Non-vector parameters
        assert!(matches!(parse_and_eval("(fn (x) x)"), Err(EvalError::TypeError(_))));

        // Non-symbol parameter
        assert!(matches!(parse_and_eval("(fn [5] x)"), Err(EvalError::TypeError(_))));
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
        let ast1 = AstParser::parse_sexp_new_domain(b"(defn double-plus-one [x] (let [doubled (* x 2)] (+ doubled 1)))", &mut 0);
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
        assert!(matches!(eval_with_env(&ast1, &mut env), Err(EvalError::ArityError(_, 3, 2))));

        // Non-symbol name
        let ast2 = AstParser::parse_sexp_new_domain(b"(defn 123 [x] x)", &mut 0);
        assert!(matches!(eval_with_env(&ast2, &mut env), Err(EvalError::TypeError(_))));

        // Non-vector parameters
        let ast3 = AstParser::parse_sexp_new_domain(b"(defn foo (x) x)", &mut 0);
        assert!(matches!(eval_with_env(&ast3, &mut env), Err(EvalError::TypeError(_))));

        // Non-symbol parameter
        let ast4 = AstParser::parse_sexp_new_domain(b"(defn foo [123] x)", &mut 0);
        assert!(matches!(eval_with_env(&ast4, &mut env), Err(EvalError::TypeError(_))));
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

    #[test]
    fn test_string_literal() {
        assert_eq!(parse_and_eval("\"hello\""), Ok(Value::String("hello".to_string())));
        assert_eq!(parse_and_eval("\"world\""), Ok(Value::String("world".to_string())));
    }

    #[test]
    fn test_string_with_escapes() {
        assert_eq!(parse_and_eval("\"hello\\nworld\""), Ok(Value::String("hello\nworld".to_string())));
        assert_eq!(parse_and_eval("\"tab\\there\""), Ok(Value::String("tab\there".to_string())));
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(parse_and_eval("\"\""), Ok(Value::String("".to_string())));
    }

    #[test]
    fn test_string_truthiness() {
        // Non-empty strings are truthy
        assert_eq!(parse_and_eval("(if \"hello\" 1 0)"), Ok(Value::Number(1)));
        // Empty strings are falsy
        assert_eq!(parse_and_eval("(if \"\" 1 0)"), Ok(Value::Number(0)));
    }

    #[test]
    fn test_string_in_let() {
        assert_eq!(parse_and_eval("(let [s \"hello\"] s)"), Ok(Value::String("hello".to_string())));
    }

    #[test]
    fn test_str_concatenation() {
        // Basic concatenation
        assert_eq!(parse_and_eval("(str \"hello\" \"world\")"), Ok(Value::String("helloworld".to_string())));
        // With spaces
        assert_eq!(parse_and_eval("(str \"hello\" \" \" \"world\")"), Ok(Value::String("hello world".to_string())));
        // Empty strings
        assert_eq!(parse_and_eval("(str \"\" \"hello\" \"\")"), Ok(Value::String("hello".to_string())));
        // No arguments
        assert_eq!(parse_and_eval("(str)"), Ok(Value::String("".to_string())));
    }

    #[test]
    fn test_str_mixed_types() {
        // Numbers
        assert_eq!(parse_and_eval("(str \"value: \" 42)"), Ok(Value::String("value: 42".to_string())));
        // Booleans
        assert_eq!(parse_and_eval("(str \"result: \" (> 5 3))"), Ok(Value::String("result: true".to_string())));
    }

    #[test]
    fn test_count_string() {
        assert_eq!(parse_and_eval("(count \"hello\")"), Ok(Value::Number(5)));
        assert_eq!(parse_and_eval("(count \"\")"), Ok(Value::Number(0)));
        assert_eq!(parse_and_eval("(count \"hello world\")"), Ok(Value::Number(11)));
    }

    #[test]
    fn test_get_string() {
        // Valid indices
        assert_eq!(parse_and_eval("(get \"hello\" 0)"), Ok(Value::String("h".to_string())));
        assert_eq!(parse_and_eval("(get \"hello\" 4)"), Ok(Value::String("o".to_string())));
        // Out of bounds returns nil
        assert_eq!(parse_and_eval("(get \"hello\" 5)"), Ok(Value::Nil));
        assert_eq!(parse_and_eval("(get \"hello\" 100)"), Ok(Value::Nil));
        // Note: Negative literals like -1 are parsed as (- 1) in simple Lisp parsers
        // For now, we test with explicit subtraction
        assert_eq!(parse_and_eval("(get \"hello\" (- 0 1))"), Ok(Value::Nil));
    }

    #[test]
    fn test_subs_string() {
        // Basic substring
        assert_eq!(parse_and_eval("(subs \"hello\" 0 5)"), Ok(Value::String("hello".to_string())));
        assert_eq!(parse_and_eval("(subs \"hello\" 1 4)"), Ok(Value::String("ell".to_string())));
        // Substring to end
        assert_eq!(parse_and_eval("(subs \"hello\" 2)"), Ok(Value::String("llo".to_string())));
        // Empty substring
        assert_eq!(parse_and_eval("(subs \"hello\" 2 2)"), Ok(Value::String("".to_string())));
        // From beginning
        assert_eq!(parse_and_eval("(subs \"hello\" 0 3)"), Ok(Value::String("hel".to_string())));
    }

    #[test]
    fn test_string_operations_combined() {
        // Count of concatenated string
        assert_eq!(parse_and_eval("(count (str \"hello\" \"world\"))"), Ok(Value::Number(10)));
        // Get from concatenated string
        assert_eq!(parse_and_eval("(get (str \"hello\" \"world\") 5)"), Ok(Value::String("w".to_string())));
        // Substring of concatenated string
        assert_eq!(parse_and_eval("(subs (str \"hello\" \" \" \"world\") 0 5)"), Ok(Value::String("hello".to_string())));
    }

    #[test]
    fn test_string_in_conditionals() {
        assert_eq!(parse_and_eval("(if (= (count \"hi\") 2) \"yes\" \"no\")"), Ok(Value::String("yes".to_string())));
        assert_eq!(parse_and_eval("(if (= (get \"abc\" 0) \"a\") 1 0)"), Ok(Value::Number(1)));
    }

    #[test]
    fn test_vector_literal() {
        assert_eq!(parse_and_eval("[1 2 3]"), Ok(Value::Vector(vec![Value::Number(1), Value::Number(2), Value::Number(3)])));
    }

    #[test]
    fn test_vec_function() {
        assert_eq!(parse_and_eval("(vec 1 2 (+ 1 1))"), Ok(Value::Vector(vec![Value::Number(1), Value::Number(2), Value::Number(2)])));
    }

    #[test]
    fn test_count_vector() {
        assert_eq!(parse_and_eval("(count [1 2 3])"), Ok(Value::Number(3)));
        assert_eq!(parse_and_eval("(count (vec))"), Ok(Value::Number(0)));
    }

    #[test]
    fn test_get_vector() {
        assert_eq!(parse_and_eval("(get [10 20 30] 1)"), Ok(Value::Number(20)));
        assert_eq!(parse_and_eval("(get [10 20] 5)"), Ok(Value::Nil));
        assert_eq!(parse_and_eval("(get [10 20] 5 99)"), Ok(Value::Number(99)));
    }

    #[test]
    fn test_subs_vector() {
        assert_eq!(parse_and_eval("(subs [1 2 3 4] 1 3)"), Ok(Value::Vector(vec![Value::Number(2), Value::Number(3)])));
        assert_eq!(parse_and_eval("(subs [1 2 3 4] 2)"), Ok(Value::Vector(vec![Value::Number(3), Value::Number(4)])));
    }

    #[test]
    fn test_str_vector() {
        assert_eq!(parse_and_eval("(str [1 2])"), Ok(Value::String("[1 2]".to_string())));
        assert_eq!(parse_and_eval("(str (vec))"), Ok(Value::String("[]".to_string())));
    }
}
