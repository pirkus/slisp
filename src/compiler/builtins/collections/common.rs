use crate::ast::{Node, Primitive};
use crate::compiler::{CompileContext, CompileError, CompileResult, HeapOwnership, MapKeyLiteral, RetainedSlot, ValueKind};
use crate::ir::IRInstruction;
use std::collections::{HashMap, HashSet};

use super::super::ownership::retains_slot;

pub(super) fn allocate_descending_slots(context: &mut CompileContext, count: usize) -> (Vec<usize>, Vec<usize>) {
    let slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered = slots.clone();
    ordered.sort_unstable_by(|a, b| b.cmp(a));
    (ordered, slots)
}

pub(super) fn release_slots(slots: Vec<usize>, retained: &[RetainedSlot], context: &mut CompileContext) {
    slots.into_iter().filter(|slot| !retains_slot(retained, *slot)).for_each(|slot| context.release_temp_slot(slot));
}

pub(super) fn release_all(slots: Vec<usize>, context: &mut CompileContext) {
    slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
}

pub(super) fn release_temp_slots(slots: Vec<usize>, context: &mut CompileContext) {
    slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
}

pub(super) fn resolve_value_kind(node: &Node, initial: ValueKind, context: &CompileContext) -> ValueKind {
    if initial != ValueKind::Any {
        return initial;
    }

    match node {
        Node::Symbol { value } => context.get_variable_type(value).or_else(|| context.get_parameter_type(value)).unwrap_or(initial),
        _ => initial,
    }
}

pub(super) fn resolve_map_key_kind(node: &Node, initial: ValueKind, context: &CompileContext) -> Result<ValueKind, CompileError> {
    let resolved = resolve_value_kind(node, initial, context);
    match resolved {
        ValueKind::Number | ValueKind::Boolean | ValueKind::String | ValueKind::Keyword | ValueKind::Nil => Ok(resolved),
        ValueKind::Any => Err(CompileError::InvalidExpression("map keys must have a concrete type".to_string())),
        _ => Err(CompileError::InvalidExpression("map keys must be numbers, booleans, strings, keywords, or nil".to_string())),
    }
}

pub(super) fn runtime_tag_for_key(kind: ValueKind) -> i64 {
    kind.runtime_tag()
}

fn literal_map_key_from_primitive(value: &Primitive) -> Option<MapKeyLiteral> {
    match value {
        Primitive::String(inner) => Some(MapKeyLiteral::String(inner.clone())),
        Primitive::Keyword(inner) => Some(MapKeyLiteral::Keyword(inner.clone())),
        Primitive::Number(num) => Some(MapKeyLiteral::Number(*num as i64)),
        Primitive::Boolean(flag) => Some(MapKeyLiteral::Boolean(*flag)),
    }
}

pub(super) fn literal_map_key(node: &Node) -> Option<MapKeyLiteral> {
    match node {
        Node::Primitive { value } => literal_map_key_from_primitive(value),
        Node::Symbol { value } if value == "nil" => Some(MapKeyLiteral::Nil),
        _ => None,
    }
}

pub(super) fn runtime_tag_for_value(kind: ValueKind) -> i64 {
    kind.runtime_tag()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_map_key_kind_accepts_numeric_literal() {
        let context = CompileContext::new();
        let node = Node::Primitive { value: Primitive::Number(42) };
        let kind = resolve_map_key_kind(&node, ValueKind::Number, &context).unwrap();
        assert_eq!(kind, ValueKind::Number);
    }

    #[test]
    fn resolve_map_key_kind_rejects_vector_type() {
        let context = CompileContext::new();
        let node = Node::Vector { root: Vec::new() };
        let kind = resolve_map_key_kind(&node, ValueKind::Vector, &context);
        assert!(kind.is_err());
    }

    #[test]
    fn literal_map_key_supports_nil_symbol() {
        let node = Node::Symbol { value: "nil".to_string() };
        assert!(matches!(literal_map_key(&node), Some(MapKeyLiteral::Nil)));
    }

    #[test]
    fn runtime_tag_for_value_matches_kind() {
        assert_eq!(runtime_tag_for_value(ValueKind::Map), ValueKind::Map.runtime_tag());
        assert_eq!(runtime_tag_for_value(ValueKind::Keyword), ValueKind::Keyword.runtime_tag());
    }
}

pub(super) fn track_owned_argument(
    result: &CompileResult,
    instructions: &mut Vec<IRInstruction>,
    context: &mut CompileContext,
    slot_kinds: &mut HashMap<usize, ValueKind>,
    tracked_slots: &mut HashSet<usize>,
    temp_slots: &mut Vec<usize>,
    kind_hint: ValueKind,
) -> Option<usize> {
    if result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, kind_hint);
        temp_slots.push(slot);
        Some(slot)
    } else {
        None
    }
}
