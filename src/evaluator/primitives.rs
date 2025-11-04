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

/// map - Apply a function to each element of a collection
pub fn eval_map(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("map".to_string(), 2, args.len()));
    }

    let func = crate::evaluator::eval_with_env(&args[0], env)?;
    let coll = crate::evaluator::eval_with_env(&args[1], env)?;

    match coll {
        Value::Vector(items) => {
            let mut results = Vec::with_capacity(items.len());
            for item in items {
                // Create a temporary node for the value
                let item_node = value_to_node(&item);
                let result = crate::evaluator::special_forms::eval_function_call(func.clone(), &[item_node], env)?;
                results.push(result);
            }
            Ok(Value::Vector(results))
        }
        Value::Nil => Ok(Value::Vector(vec![])),
        _ => Err(EvalError::TypeError("map: second argument must be a vector or nil".to_string())),
    }
}

/// filter - Select elements that satisfy a predicate
pub fn eval_filter(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("filter".to_string(), 2, args.len()));
    }

    let pred = crate::evaluator::eval_with_env(&args[0], env)?;
    let coll = crate::evaluator::eval_with_env(&args[1], env)?;

    match coll {
        Value::Vector(items) => {
            let mut results = Vec::new();
            for item in items {
                let item_node = value_to_node(&item);
                let result = crate::evaluator::special_forms::eval_function_call(pred.clone(), &[item_node], env)?;
                if is_truthy(&result) {
                    results.push(item);
                }
            }
            Ok(Value::Vector(results))
        }
        Value::Nil => Ok(Value::Vector(vec![])),
        _ => Err(EvalError::TypeError("filter: second argument must be a vector or nil".to_string())),
    }
}

/// reduce - Fold/accumulate over a collection
pub fn eval_reduce(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::ArityError("reduce".to_string(), 2, args.len()));
    }

    let func = crate::evaluator::eval_with_env(&args[0], env)?;

    let (init, coll) = if args.len() == 3 {
        let init = crate::evaluator::eval_with_env(&args[1], env)?;
        let coll = crate::evaluator::eval_with_env(&args[2], env)?;
        (init, coll)
    } else {
        let coll = crate::evaluator::eval_with_env(&args[1], env)?;
        match &coll {
            Value::Vector(items) => {
                if items.is_empty() {
                    return Err(EvalError::InvalidOperation("reduce: empty collection with no initial value".to_string()));
                }
                (items[0].clone(), Value::Vector(items[1..].to_vec()))
            }
            Value::Nil => return Err(EvalError::InvalidOperation("reduce: empty collection with no initial value".to_string())),
            _ => return Err(EvalError::TypeError("reduce: second argument must be a vector or nil".to_string())),
        }
    };

    match coll {
        Value::Vector(items) => {
            let mut acc = init;
            for item in items {
                let acc_node = value_to_node(&acc);
                let item_node = value_to_node(&item);
                acc = crate::evaluator::special_forms::eval_function_call(func.clone(), &[acc_node, item_node], env)?;
            }
            Ok(acc)
        }
        Value::Nil => Ok(init),
        _ => Err(EvalError::TypeError("reduce: collection must be a vector or nil".to_string())),
    }
}

/// first - Get the first element of a collection
pub fn eval_first(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("first".to_string(), 1, args.len()));
    }

    let coll = crate::evaluator::eval_with_env(&args[0], env)?;
    match coll {
        Value::Vector(items) => {
            if items.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(items[0].clone())
            }
        }
        Value::Nil => Ok(Value::Nil),
        _ => Err(EvalError::TypeError("first: argument must be a vector or nil".to_string())),
    }
}

/// rest - Get all but the first element of a collection
pub fn eval_rest(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("rest".to_string(), 1, args.len()));
    }

    let coll = crate::evaluator::eval_with_env(&args[0], env)?;
    match coll {
        Value::Vector(items) => {
            if items.is_empty() {
                Ok(Value::Vector(vec![]))
            } else {
                Ok(Value::Vector(items[1..].to_vec()))
            }
        }
        Value::Nil => Ok(Value::Vector(vec![])),
        _ => Err(EvalError::TypeError("rest: argument must be a vector or nil".to_string())),
    }
}

/// cons - Add element to the front of a collection
pub fn eval_cons(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("cons".to_string(), 2, args.len()));
    }

    let elem = crate::evaluator::eval_with_env(&args[0], env)?;
    let coll = crate::evaluator::eval_with_env(&args[1], env)?;

    match coll {
        Value::Vector(items) => {
            let mut result = vec![elem];
            result.extend(items);
            Ok(Value::Vector(result))
        }
        Value::Nil => Ok(Value::Vector(vec![elem])),
        _ => Err(EvalError::TypeError("cons: second argument must be a vector or nil".to_string())),
    }
}

/// conj - Conjoin element to a collection (append for vectors)
pub fn eval_conj(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::ArityError("conj".to_string(), 2, args.len()));
    }

    let coll = crate::evaluator::eval_with_env(&args[0], env)?;

    match coll {
        Value::Vector(mut items) => {
            for arg in &args[1..] {
                let elem = crate::evaluator::eval_with_env(arg, env)?;
                items.push(elem);
            }
            Ok(Value::Vector(items))
        }
        Value::Nil => {
            let mut items = Vec::with_capacity(args.len() - 1);
            for arg in &args[1..] {
                let elem = crate::evaluator::eval_with_env(arg, env)?;
                items.push(elem);
            }
            Ok(Value::Vector(items))
        }
        _ => Err(EvalError::TypeError("conj: first argument must be a vector or nil".to_string())),
    }
}

/// concat - Concatenate multiple collections
pub fn eval_concat(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut result = Vec::new();

    for arg in args {
        let coll = crate::evaluator::eval_with_env(arg, env)?;
        match coll {
            Value::Vector(items) => {
                result.extend(items);
            }
            Value::Nil => {}
            _ => return Err(EvalError::TypeError("concat: arguments must be vectors or nil".to_string())),
        }
    }

    Ok(Value::Vector(result))
}

/// keys - Get all keys from a map
pub fn eval_keys(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("keys".to_string(), 1, args.len()));
    }

    let coll = crate::evaluator::eval_with_env(&args[0], env)?;
    match coll {
        Value::Map(entries) => {
            let keys: Vec<Value> = entries.keys().map(|k| match k {
                MapKey::Number(n) => Value::Number(*n),
                MapKey::Boolean(b) => Value::Boolean(*b),
                MapKey::String(s) => Value::String(s.clone()),
                MapKey::Keyword(k) => Value::Keyword(k.clone()),
                MapKey::Nil => Value::Nil,
            }).collect();
            Ok(Value::Vector(keys))
        }
        Value::Nil => Ok(Value::Vector(vec![])),
        _ => Err(EvalError::TypeError("keys: argument must be a map or nil".to_string())),
    }
}

/// vals - Get all values from a map
pub fn eval_vals(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::ArityError("vals".to_string(), 1, args.len()));
    }

    let coll = crate::evaluator::eval_with_env(&args[0], env)?;
    match coll {
        Value::Map(entries) => {
            let values: Vec<Value> = entries.values().cloned().collect();
            Ok(Value::Vector(values))
        }
        Value::Nil => Ok(Value::Vector(vec![])),
        _ => Err(EvalError::TypeError("vals: argument must be a map or nil".to_string())),
    }
}

/// merge - Merge multiple maps (right-associative)
pub fn eval_merge(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let mut result = std::collections::HashMap::new();

    for arg in args {
        let coll = crate::evaluator::eval_with_env(arg, env)?;
        match coll {
            Value::Map(entries) => {
                for (k, v) in entries {
                    result.insert(k, v);
                }
            }
            Value::Nil => {}
            _ => return Err(EvalError::TypeError("merge: arguments must be maps or nil".to_string())),
        }
    }

    Ok(Value::Map(result))
}

/// select-keys - Select a subset of keys from a map
pub fn eval_select_keys(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("select-keys".to_string(), 2, args.len()));
    }

    let map_val = crate::evaluator::eval_with_env(&args[0], env)?;
    let keys_val = crate::evaluator::eval_with_env(&args[1], env)?;

    let entries = match map_val {
        Value::Map(e) => e,
        Value::Nil => return Ok(Value::Map(std::collections::HashMap::new())),
        _ => return Err(EvalError::TypeError("select-keys: first argument must be a map or nil".to_string())),
    };

    let keys_vec = match keys_val {
        Value::Vector(v) => v,
        _ => return Err(EvalError::TypeError("select-keys: second argument must be a vector".to_string())),
    };

    let mut result = std::collections::HashMap::new();
    for key_val in keys_vec {
        let key = MapKey::try_from_value(&key_val)?;
        if let Some(value) = entries.get(&key) {
            result.insert(key, value.clone());
        }
    }

    Ok(Value::Map(result))
}

/// zipmap - Create a map from a vector of keys and a vector of values
pub fn eval_zipmap(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::ArityError("zipmap".to_string(), 2, args.len()));
    }

    let keys_val = crate::evaluator::eval_with_env(&args[0], env)?;
    let vals_val = crate::evaluator::eval_with_env(&args[1], env)?;

    let keys_vec = match keys_val {
        Value::Vector(v) => v,
        Value::Nil => return Ok(Value::Map(std::collections::HashMap::new())),
        _ => return Err(EvalError::TypeError("zipmap: first argument must be a vector or nil".to_string())),
    };

    let vals_vec = match vals_val {
        Value::Vector(v) => v,
        Value::Nil => vec![],
        _ => return Err(EvalError::TypeError("zipmap: second argument must be a vector or nil".to_string())),
    };

    let mut result = std::collections::HashMap::new();
    for (key_val, value) in keys_vec.into_iter().zip(vals_vec.into_iter()) {
        let key = MapKey::try_from_value(&key_val)?;
        result.insert(key, value);
    }

    Ok(Value::Map(result))
}

/// Helper function to convert Value back to Node for function application
fn value_to_node(value: &Value) -> Node {
    match value {
        Value::Number(n) => Node::Primitive { value: crate::ast::Primitive::Number(*n as usize) },
        Value::Boolean(b) => Node::Primitive { value: crate::ast::Primitive::Boolean(*b) },
        Value::String(s) => Node::Primitive { value: crate::ast::Primitive::String(s.clone()) },
        Value::Keyword(k) => Node::Primitive { value: crate::ast::Primitive::Keyword(k.clone()) },
        Value::Nil => Node::List { root: vec![] },
        Value::Vector(items) => {
            let nodes: Vec<Node> = items.iter().map(value_to_node).collect();
            Node::Vector { root: nodes }
        }
        Value::Set(entries) => {
            let nodes: Vec<Node> = entries.iter().map(|key| map_key_to_node(key)).collect();
            Node::Set { root: nodes }
        }
        Value::Map(entries) => {
            let nodes: Vec<(Node, Node)> = entries.iter().map(|(k, v)| (map_key_to_node(k), value_to_node(v))).collect();
            Node::Map { entries: nodes }
        }
        Value::Function { .. } => {
            // Functions can't be converted back to nodes directly
            // This is a limitation, but for higher-order functions we pass them as Values
            Node::List { root: vec![] }
        }
    }
}

fn map_key_to_node(key: &MapKey) -> Node {
    match key {
        MapKey::Number(n) => Node::Primitive { value: crate::ast::Primitive::Number(*n as usize) },
        MapKey::Boolean(b) => Node::Primitive { value: crate::ast::Primitive::Boolean(*b) },
        MapKey::String(s) => Node::Primitive { value: crate::ast::Primitive::String(s.clone()) },
        MapKey::Keyword(k) => Node::Primitive { value: crate::ast::Primitive::Keyword(k.clone()) },
        MapKey::Nil => Node::List { root: vec![] },
    }
}
