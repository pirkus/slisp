use super::{Environment, EvalError, Value};
use crate::ast::Node;
use crate::evaluator::MapKey;
use std::io::{self, Write};

pub(crate) fn value_to_string(value: &Value) -> String {
    match value {
        Value::Number(n) => n.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Keyword(k) => format!(":{}", k),
        Value::Nil => "nil".to_string(),
        Value::Function { .. } => "#<function>".to_string(),
        Value::Vector(items) => stringify_sequence(items.iter().map(value_to_string), "[", "]"),
        Value::Set(entries) => {
            if entries.is_empty() {
                "#{}".to_string()
            } else {
                let mut rendered: Vec<String> = entries.iter().map(map_key_to_string).collect();
                rendered.sort();
                stringify_sequence(rendered.into_iter(), "#{", "}")
            }
        }
        Value::Map(entries) => {
            if entries.is_empty() {
                "{}".to_string()
            } else {
                let mut rendered: Vec<(String, String)> =
                    entries.iter().map(|(key, value)| (map_key_to_string(key), value_to_string(value))).collect();
                rendered.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));
                let pairs = rendered.into_iter().map(|(key, value)| format!("{} {}", key, value));
                stringify_sequence(pairs, "{", "}")
            }
        }
    }
}

fn stringify_sequence<I>(iter: I, prefix: &str, suffix: &str) -> String
where
    I: Iterator<Item = String>,
{
    let body = iter.collect::<Vec<_>>().join(" ");
    format!("{}{}{}", prefix, body, suffix)
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

fn write_output(chunks: &[String], newline: bool) -> Result<(), EvalError> {
    let mut stdout = io::stdout();
    if let Some((first, rest)) = chunks.split_first() {
        stdout.write_all(first.as_bytes()).map_err(|err| EvalError::InvalidOperation(format!("print failed: {}", err)))?;
        for chunk in rest {
            stdout.write_all(b" ").map_err(|err| EvalError::InvalidOperation(format!("print failed: {}", err)))?;
            stdout.write_all(chunk.as_bytes()).map_err(|err| EvalError::InvalidOperation(format!("print failed: {}", err)))?;
        }
    }
    if newline {
        stdout.write_all(b"\n").map_err(|err| EvalError::InvalidOperation(format!("print failed: {}", err)))?;
    }
    stdout.flush().map_err(|err| EvalError::InvalidOperation(format!("print failed: {}", err)))
}

fn render_arguments(args: &[Node], env: &mut Environment) -> Result<Vec<String>, EvalError> {
    args.iter()
        .map(|arg| crate::evaluator::eval_with_env(arg, env).map(|value| value_to_string(&value)))
        .collect()
}

fn format_printf_string(format: &str, values: &[Value]) -> String {
    let mut rendered = String::new();
    let mut arg_index = 0usize;
    let mut idx = 0usize;
    let bytes = format.as_bytes();

    while idx < bytes.len() {
        if bytes[idx] != b'%' {
            if let Some(rel) = format[idx..].find('%') {
                let end = idx + rel;
                rendered.push_str(&format[idx..end]);
                idx = end;
            } else {
                rendered.push_str(&format[idx..]);
                break;
            }
            continue;
        }

        let placeholder_start = idx;
        idx += 1;
        if idx >= bytes.len() {
            rendered.push('%');
            break;
        }

        if bytes[idx] == b'%' {
            rendered.push('%');
            idx += 1;
            continue;
        }

        let mut spec_idx = idx;
        while spec_idx < bytes.len() && !bytes[spec_idx].is_ascii_alphabetic() {
            spec_idx += 1;
        }

        if spec_idx >= bytes.len() {
            rendered.push_str(&format[placeholder_start..]);
            break;
        }

        let spec = bytes[spec_idx].to_ascii_lowercase();
        if spec == b'n' {
            rendered.push('\n');
            idx = spec_idx + 1;
            continue;
        }

        if arg_index < values.len() {
            rendered.push_str(&value_to_string(&values[arg_index]));
            arg_index += 1;
        } else {
            rendered.push_str(&format[placeholder_start..spec_idx + 1]);
        }

        idx = spec_idx + 1;
    }

    rendered
}

pub(crate) fn eval_print(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let chunks = render_arguments(args, env)?;
    write_output(&chunks, false)?;
    Ok(Value::Nil)
}

pub(crate) fn eval_println(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    let chunks = render_arguments(args, env)?;
    write_output(&chunks, true)?;
    Ok(Value::Nil)
}

pub(crate) fn eval_printf(args: &[Node], env: &mut Environment) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::ArityError("printf".to_string(), 1, 0));
    }

    let format_value = crate::evaluator::eval_with_env(&args[0], env)?;
    let format_string = value_to_string(&format_value);

    let mut values = Vec::with_capacity(args.len().saturating_sub(1));
    for arg in &args[1..] {
        values.push(crate::evaluator::eval_with_env(arg, env)?);
    }

    let rendered = format_printf_string(&format_string, &values);
    let output = vec![rendered];
    write_output(&output, false)?;
    Ok(Value::Nil)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_formatter_handles_placeholders() {
        let values = vec![Value::Number(10), Value::String("units".to_string())];
        let formatted = format_printf_string("value=%s %s", &values);
        assert_eq!(formatted, "value=10 units");
    }

    #[test]
    fn printf_formatter_handles_missing_args() {
        let values = vec![Value::Number(10)];
        let formatted = format_printf_string("value=%s %s", &values);
        assert_eq!(formatted, "value=10 %s");
    }

    #[test]
    fn printf_formatter_supports_newline_escape() {
        let values = Vec::new();
        let formatted = format_printf_string("row%ntwo", &values);
        assert_eq!(formatted, "row\ntwo");
    }

    #[test]
    fn printf_formatter_keeps_literal_percent() {
        let values = vec![Value::Nil];
        let formatted = format_printf_string("progress %% %s", &values);
        assert_eq!(formatted, "progress % nil");
    }
}
