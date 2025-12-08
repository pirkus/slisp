use super::{Environment, EvalError, MapKey, Value};
/// Primitive operations - arithmetic and comparisons
use crate::ast::Node;
use std::collections::HashSet;

/// Evaluate arithmetic operations (+, -, *, /)
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
        (Value::Keyword(a), Value::Keyword(b)) => a == b,
        (Value::Vector(a), Value::Vector(b)) => a == b,
        (Value::Set(a), Value::Set(b)) => a == b,
        (Value::Map(a), Value::Map(b)) => a == b,
        (Value::Nil, Value::Nil) => true,
        _ => false, // Different types are not equal
    };

    Ok(Value::Boolean(result))
}

/// Evaluate comparison operations (<, >, <=, >=)
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
        Value::Keyword(_) => true,
        Value::String(s) => !s.is_empty(),
        Value::Vector(items) => !items.is_empty(),
        Value::Set(entries) => !entries.is_empty(),
        Value::Map(entries) => !entries.is_empty(),
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Keyword(k) => format!(":{}", k),
        Value::Nil => "nil".to_string(),
        Value::Function { .. } => "#<function>".to_string(),
        Value::Vector(items) => {
            if items.is_empty() {
                "[]".to_string()
            } else {
                let mut out = String::from("[");
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str(&value_to_string(item));
                }
                out.push(']');
                out
            }
        }
        Value::Set(entries) => {
            if entries.is_empty() {
                "#{}".to_string()
            } else {
                let mut rendered: Vec<String> = entries.iter().map(map_key_to_string).collect();
                rendered.sort();
                let mut out = String::from("#{");
                for (idx, item) in rendered.iter().enumerate() {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str(item);
                }
                out.push('}');
                out
            }
        }
        Value::Map(entries) => {
            if entries.is_empty() {
                "{}".to_string()
            } else {
                let mut rendered: Vec<(String, String)> = entries.iter().map(|(key, value)| (map_key_to_string(key), value_to_string(value))).collect();
                rendered.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
                let mut out = String::from("{");
                for (idx, (key, value)) in rendered.iter().enumerate() {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str(key);
                    out.push(' ');
                    out.push_str(value);
                }
                out.push('}');
                out
            }
        }
    }
}

fn map_key_to_string(key: &MapKey) -> String {
    match key {
        MapKey::Number(n) => n.to_string(),
        MapKey::Boolean(true) => "true".to_string(),
        MapKey::Boolean(false) => "false".to_string(),
        MapKey::String(s) => format!("\"{}\"", s),
        MapKey::Keyword(k) => format!(":{}", k),
        MapKey::Nil => "nil".to_string(),
    }
}

fn resolve_default(default: Option<&Node>, env: &mut Environment) -> Result<Value, EvalError> {
    if let Some(expr) = default {
        crate::evaluator::eval_with_env(expr, env)
    } else {
        Ok(Value::Nil)
    }
}

/// str - String concatenation (Clojure-style)
/// Converts arguments to strings and concatenates them
pub fn eval_str(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut result = String::new();

    for arg in args {
        let val = crate::evaluator::eval_with_env(arg, env)?;
        result.push_str(&value_to_string(&val));
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
        Value::Vector(items) => Ok(Value::Number(items.len() as isize)),
        Value::Set(entries) => Ok(Value::Number(entries.len() as isize)),
        Value::Map(entries) => Ok(Value::Number(entries.len() as isize)),
        Value::Nil => Ok(Value::Number(0)),
        _ => Err(EvalError::TypeError("count requires a string, vector, map, set, or nil argument".to_string())),
    }
}

/// get - Get character at index (returns string or nil)
pub fn eval_get(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::ArityError("get".to_string(), 2, args.len()));
    }

    let target = crate::evaluator::eval_with_env(&args[0], env)?;
    let index_val = crate::evaluator::eval_with_env(&args[1], env)?;
    let default = if args.len() == 3 { Some(&args[2]) } else { None };

    match (target, index_val) {
        (Value::String(s), Value::Number(idx)) => {
            if idx < 0 || idx >= s.len() as isize {
                return resolve_default(default, env);
            }

            let idx_usize = idx as usize;
            if let Some(ch) = s.chars().nth(idx_usize) {
                Ok(Value::String(ch.to_string()))
            } else {
                resolve_default(default, env)
            }
        }
        (Value::Vector(items), Value::Number(idx)) => {
            if idx < 0 || idx >= items.len() as isize {
                return resolve_default(default, env);
            }
            Ok(items[idx as usize].clone())
        }
        (Value::Map(entries), key_value) => {
            let key = MapKey::try_from_value(&key_value)?;
            if let Some(found) = entries.get(&key) {
                Ok(found.clone())
            } else {
                resolve_default(default, env)
            }
        }
        (Value::String(_), _) | (Value::Vector(_), _) => Err(EvalError::TypeError("get: index must be a number".to_string())),
        _ => Err(EvalError::TypeError("get: first argument must be a string, vector, or map".to_string())),
    }
}

fn compute_range(start_val: Value, args: &[Node], env: &mut Environment, len: usize) -> Result<(usize, usize), EvalError> {
    let start = match start_val {
        Value::Number(n) => {
            if n < 0 {
                return Err(EvalError::InvalidOperation("subs: start index cannot be negative".to_string()));
            }
            n as usize
        }
        _ => return Err(EvalError::TypeError("subs: start index must be a number".to_string())),
    };

    let end = if args.len() == 3 {
        let end_val = crate::evaluator::eval_with_env(&args[2], env)?;
        match end_val {
            Value::Number(n) => {
                if n < 0 {
                    return Err(EvalError::InvalidOperation("subs: end index cannot be negative".to_string()));
                }
                n as usize
            }
            _ => return Err(EvalError::TypeError("subs: end index must be a number".to_string())),
        }
    } else {
        len
    };

    if start > len {
        return Err(EvalError::InvalidOperation(format!("subs: start index {} out of bounds for length {}", start, len)));
    }

    if end > len {
        return Err(EvalError::InvalidOperation(format!("subs: end index {} out of bounds for length {}", end, len)));
    }

    if start > end {
        return Err(EvalError::InvalidOperation(format!("subs: start index {} is greater than end index {}", start, end)));
    }

    Ok((start, end))
}

/// subs - Extract substring (start, end)
pub fn eval_subs(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::ArityError("subs".to_string(), 2, args.len()));
    }

    let target = crate::evaluator::eval_with_env(&args[0], env)?;
    let start_val = crate::evaluator::eval_with_env(&args[1], env)?;

    match target {
        Value::String(s) => {
            let len = s.len();
            let (start, end) = compute_range(start_val, args, env, len)?;
            let substring: String = s.chars().skip(start).take(end - start).collect();
            Ok(Value::String(substring))
        }
        Value::Vector(items) => {
            let len = items.len();
            let (start, end) = compute_range(start_val, args, env, len)?;
            let slice = items[start..end].to_vec();
            Ok(Value::Vector(slice))
        }
        _ => Err(EvalError::TypeError("subs: first argument must be a string or vector".to_string())),
    }
}

/// vec - Construct a vector from evaluated arguments
pub fn eval_vec(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(crate::evaluator::eval_with_env(arg, env)?);
    }
    Ok(Value::Vector(values))
}

/// set - Construct a set from evaluated arguments (duplicates removed)
pub fn eval_set(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut entries = HashSet::with_capacity(args.len());
    for arg in args {
        let value = crate::evaluator::eval_with_env(arg, env)?;
        let key = MapKey::try_from_value(&value)?;
        entries.insert(key);
    }
    Ok(Value::Set(entries))
}

pub fn eval_hash_map(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() % 2 != 0 {
        return Err(EvalError::InvalidOperation("hash-map requires key/value pairs".to_string()));
    }

    let mut entries = std::collections::HashMap::with_capacity(args.len() / 2);
    let mut idx = 0usize;
    while idx < args.len() {
        let key_val = crate::evaluator::eval_with_env(&args[idx], env)?;
        let value_val = crate::evaluator::eval_with_env(&args[idx + 1], env)?;
        let key = MapKey::try_from_value(&key_val)?;
        entries.insert(key, value_val);
        idx += 2;
    }
    Ok(Value::Map(entries))
}

pub fn eval_assoc(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::ArityError("assoc".to_string(), 3, args.len()));
    }

    if (args.len() - 1) % 2 != 0 {
        return Err(EvalError::InvalidOperation("assoc expects key/value pairs".to_string()));
    }

    let base = crate::evaluator::eval_with_env(&args[0], env)?;
    let mut entries = match base {
        Value::Map(map) => map,
        Value::Nil => std::collections::HashMap::new(),
        _ => return Err(EvalError::TypeError("assoc: first argument must be a map or nil".to_string())),
    };

    let mut idx = 1usize;
    while idx < args.len() {
        let key_val = crate::evaluator::eval_with_env(&args[idx], env)?;
        let value_val = crate::evaluator::eval_with_env(&args[idx + 1], env)?;
        let key = MapKey::try_from_value(&key_val)?;
        entries.insert(key, value_val);
        idx += 2;
    }

    Ok(Value::Map(entries))
}

pub fn eval_dissoc(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 1 {
        return Err(EvalError::ArityError("dissoc".to_string(), 1, 0));
    }

    let base = crate::evaluator::eval_with_env(&args[0], env)?;
    if args.len() == 1 {
        return match base {
            Value::Map(map) => Ok(Value::Map(map)),
            Value::Nil => Ok(Value::Map(std::collections::HashMap::new())),
            _ => Err(EvalError::TypeError("dissoc: first argument must be a map or nil".to_string())),
        };
    }

    let mut entries = match base {
        Value::Map(map) => map,
        Value::Nil => std::collections::HashMap::new(),
        _ => return Err(EvalError::TypeError("dissoc: first argument must be a map or nil".to_string())),
    };

    for key_expr in &args[1..] {
        let key_val = crate::evaluator::eval_with_env(key_expr, env)?;
        let key = MapKey::try_from_value(&key_val)?;
        entries.remove(&key);
    }

    Ok(Value::Map(entries))
}

pub fn eval_disj(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::ArityError("disj".to_string(), 1, 0));
    }

    let base = crate::evaluator::eval_with_env(&args[0], env)?;
    if args.len() == 1 {
        return match base {
            Value::Set(entries) => Ok(Value::Set(entries)),
            Value::Nil => Ok(Value::Set(HashSet::new())),
            _ => Err(EvalError::TypeError("disj: first argument must be a set or nil".to_string())),
        };
    }

    let mut entries = match base {
        Value::Set(entries) => entries,
        Value::Nil => HashSet::with_capacity(args.len() - 1),
        _ => return Err(EvalError::TypeError("disj: first argument must be a set or nil".to_string())),
    };

    for expr in &args[1..] {
        let value = crate::evaluator::eval_with_env(expr, env)?;
        let key = MapKey::try_from_value(&value)?;
        entries.remove(&key);
    }

    Ok(Value::Set(entries))
}

pub fn eval_contains(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("contains?".to_string(), 2, args.len()));
    }

    let target = crate::evaluator::eval_with_env(&args[0], env)?;
    let key_val = crate::evaluator::eval_with_env(&args[1], env)?;

    match target {
        Value::Map(entries) => {
            let key = MapKey::try_from_value(&key_val)?;
            Ok(Value::Boolean(entries.contains_key(&key)))
        }
        Value::Set(entries) => {
            let key = MapKey::try_from_value(&key_val)?;
            Ok(Value::Boolean(entries.contains(&key)))
        }
        Value::Nil => Ok(Value::Boolean(false)),
        _ => Err(EvalError::TypeError("contains?: first argument must be a map, set, or nil".to_string())),
    }
}
