use super::{EvalError, Value};
use crate::ast::Node;

/// Shared truthiness predicate so interpreter semantics stay aligned.
pub(crate) fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Boolean(b) => *b,
        Value::Number(n) => *n != 0,
        Value::Nil => false,
        Value::Function { .. } => true,
        Value::Keyword(_) => true,
        Value::String(s) => !s.is_empty(),
        Value::Vector(items) => !items.is_empty(),
        Value::Set(entries) => !entries.is_empty(),
        Value::Map(entries) => !entries.is_empty(),
    }
}

/// Evaluate slices that encode alternating key/value nodes.
pub(crate) fn fold_pairs<'a, T, F, E>(nodes: &'a [Node], init: T, err_factory: E, mut f: F) -> Result<T, EvalError>
where
    F: FnMut(T, &'a Node, &'a Node) -> Result<T, EvalError>,
    E: Fn() -> EvalError,
{
    if nodes.len() % 2 != 0 {
        return Err(err_factory());
    }

    nodes.chunks(2).try_fold(init, |acc, chunk| f(acc, &chunk[0], &chunk[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Primitive;

    #[test]
    fn fold_pairs_rejects_odd_lengths() {
        let odd = vec![Node::Primitive { value: Primitive::Number(1) }];
        let err = fold_pairs(&odd, 0usize, || EvalError::InvalidOperation("oops".into()), |acc, _, _| Ok(acc)).unwrap_err();
        assert!(matches!(err, EvalError::InvalidOperation(_)));
    }

    #[test]
    fn fold_pairs_applies_callback_over_pairs() {
        let nodes = vec![
            Node::Primitive { value: Primitive::Number(1) },
            Node::Primitive { value: Primitive::Number(2) },
            Node::Primitive { value: Primitive::Number(3) },
            Node::Primitive { value: Primitive::Number(4) },
        ];
        let result = fold_pairs(
            &nodes,
            Vec::new(),
            || EvalError::InvalidOperation("bad".into()),
            |mut acc, left, right| {
                let l = match left {
                    Node::Primitive { value: Primitive::Number(n) } => *n,
                    _ => 0,
                };
                let r = match right {
                    Node::Primitive { value: Primitive::Number(n) } => *n,
                    _ => 0,
                };
                acc.push((l, r));
                Ok(acc)
            },
        )
        .expect("fold succeeds");
        assert_eq!(result, vec![(1, 2), (3, 4)]);
    }
}
