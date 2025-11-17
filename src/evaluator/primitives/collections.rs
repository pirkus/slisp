use super::{Environment, EvalError, Value};
use crate::ast::Node;
use crate::evaluator::helpers::fold_pairs;
use crate::evaluator::MapKey;
use std::collections::{HashMap, HashSet};

fn resolve_default(default: Option<&Node>, env: &mut Environment) -> Result<Value, EvalError> {
    if let Some(expr) = default {
        crate::evaluator::eval_with_env(expr, env)
    } else {
        Ok(Value::Nil)
    }
}

pub(crate) fn eval_get(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
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

pub(crate) fn eval_subs(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
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

pub(crate) fn eval_vec(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    args.iter().map(|arg| crate::evaluator::eval_with_env(arg, env)).collect::<Result<Vec<_>, _>>().map(Value::Vector)
}

pub(crate) fn eval_set(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    args.iter()
        .try_fold(HashSet::with_capacity(args.len()), |mut acc, arg| {
            let value = crate::evaluator::eval_with_env(arg, env)?;
            let key = MapKey::try_from_value(&value)?;
            acc.insert(key);
            Ok(acc)
        })
        .map(Value::Set)
}

pub(crate) fn eval_hash_map(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let initial = HashMap::with_capacity(args.len() / 2);
    fold_pairs(
        args,
        initial,
        || EvalError::InvalidOperation("hash-map requires key/value pairs".to_string()),
        |mut acc, key_node, value_node| {
            let key_val = crate::evaluator::eval_with_env(key_node, env)?;
            let value_val = crate::evaluator::eval_with_env(value_node, env)?;
            let key = MapKey::try_from_value(&key_val)?;
            acc.insert(key, value_val);
            Ok(acc)
        },
    )
    .map(Value::Map)
}

pub(crate) fn eval_assoc(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::ArityError("assoc".to_string(), 3, args.len()));
    }

    if (args.len() - 1) % 2 != 0 {
        return Err(EvalError::InvalidOperation("assoc expects key/value pairs".to_string()));
    }

    let base = crate::evaluator::eval_with_env(&args[0], env)?;
    let entries = match base {
        Value::Map(map) => map,
        Value::Nil => HashMap::new(),
        _ => return Err(EvalError::TypeError("assoc: first argument must be a map or nil".to_string())),
    };

    fold_pairs(
        &args[1..],
        entries,
        || EvalError::InvalidOperation("assoc expects key/value pairs".to_string()),
        |mut acc, key_node, value_node| {
            let key_val = crate::evaluator::eval_with_env(key_node, env)?;
            let value_val = crate::evaluator::eval_with_env(value_node, env)?;
            let key = MapKey::try_from_value(&key_val)?;
            acc.insert(key, value_val);
            Ok(acc)
        },
    )
    .map(Value::Map)
}

pub(crate) fn eval_dissoc(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.len() < 1 {
        return Err(EvalError::ArityError("dissoc".to_string(), 1, 0));
    }

    let base = crate::evaluator::eval_with_env(&args[0], env)?;
    if args.len() == 1 {
        return match base {
            Value::Map(map) => Ok(Value::Map(map)),
            Value::Nil => Ok(Value::Map(HashMap::new())),
            _ => Err(EvalError::TypeError("dissoc: first argument must be a map or nil".to_string())),
        };
    }

    let entries = match base {
        Value::Map(map) => map,
        Value::Nil => HashMap::new(),
        _ => return Err(EvalError::TypeError("dissoc: first argument must be a map or nil".to_string())),
    };

    args.iter()
        .skip(1)
        .try_fold(entries, |mut acc, key_expr| {
            let key_val = crate::evaluator::eval_with_env(key_expr, env)?;
            let key = MapKey::try_from_value(&key_val)?;
            acc.remove(&key);
            Ok(acc)
        })
        .map(Value::Map)
}

pub(crate) fn eval_disj(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
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

    let entries = match base {
        Value::Set(entries) => entries,
        Value::Nil => HashSet::with_capacity(args.len() - 1),
        _ => return Err(EvalError::TypeError("disj: first argument must be a set or nil".to_string())),
    };

    args.iter()
        .skip(1)
        .try_fold(entries, |mut acc, expr| {
            let value = crate::evaluator::eval_with_env(expr, env)?;
            let key = MapKey::try_from_value(&value)?;
            acc.remove(&key);
            Ok(acc)
        })
        .map(Value::Set)
}

pub(crate) fn eval_contains(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Primitive;

    fn number_node(value: usize) -> Node {
        Node::Primitive { value: Primitive::Number(value) }
    }

    fn string_node(value: &str) -> Node {
        Node::Primitive {
            value: Primitive::String(value.to_string()),
        }
    }

    fn keyword_node(value: &str) -> Node {
        Node::Primitive {
            value: Primitive::Keyword(value.to_string()),
        }
    }

    #[test]
    fn eval_vec_collects_nodes() {
        let nodes = vec![number_node(1), number_node(2)];
        let mut env = Environment::new();
        let result = eval_vec(&nodes, &mut env).expect("vector build succeeds");
        match result {
            Value::Vector(items) => assert_eq!(items.len(), 2),
            other => panic!("expected vector, got {:?}", other),
        }
    }

    #[test]
    fn eval_assoc_extends_nil_base() {
        let args = vec![Node::Symbol { value: "nil".into() }, keyword_node("k"), string_node("v")];
        let mut env = Environment::new();
        env.insert("nil".into(), Value::Nil);
        let result = eval_assoc(&args, &mut env).expect("assoc succeeds");
        match result {
            Value::Map(map) => assert_eq!(map.len(), 1),
            other => panic!("expected map, got {:?}", other),
        }
    }

    #[test]
    fn eval_get_uses_default_on_out_of_bounds() {
        let args = vec![string_node("abc"), number_node(10), string_node("fallback")];
        let mut env = Environment::new();
        let result = eval_get(&args, &mut env).expect("get succeeds");
        assert_eq!(result, Value::String("fallback".into()));
    }
}
