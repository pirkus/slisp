use crate::ast::{Node, Primitive};
use crate::compiler::{CompileContext, CompileError, HeapOwnership, MapKeyLiteral, RetainedSlot, ValueKind};
use crate::ir::IRInstruction;

pub(crate) fn resolve_value_kind(node: &Node, initial: ValueKind, context: &CompileContext) -> ValueKind {
    if initial != ValueKind::Any {
        return initial;
    }

    match node {
        Node::Symbol { value } => context.get_variable_type(value).or_else(|| context.get_parameter_type(value)).unwrap_or(initial),
        _ => initial,
    }
}

pub(crate) fn resolve_map_key_kind(node: &Node, initial: ValueKind, context: &CompileContext) -> Result<ValueKind, CompileError> {
    let resolved = resolve_value_kind(node, initial, context);
    match resolved {
        ValueKind::Number | ValueKind::Boolean | ValueKind::String | ValueKind::Keyword | ValueKind::Nil => Ok(resolved),
        ValueKind::Any => Err(CompileError::InvalidExpression("map keys must have a concrete type".to_string())),
        _ => Err(CompileError::InvalidExpression("map keys must be numbers, booleans, strings, keywords, or nil".to_string())),
    }
}

pub(crate) fn runtime_tag_for_key(kind: ValueKind) -> i64 {
    kind.runtime_tag()
}

pub(crate) fn runtime_tag_for_value(kind: ValueKind) -> i64 {
    kind.runtime_tag()
}

pub(crate) fn literal_map_key_from_primitive(value: &Primitive) -> Option<MapKeyLiteral> {
    match value {
        Primitive::String(inner) => Some(MapKeyLiteral::String(inner.clone())),
        Primitive::Keyword(inner) => Some(MapKeyLiteral::Keyword(inner.clone())),
        Primitive::Number(num) => Some(MapKeyLiteral::Number(*num as i64)),
        Primitive::Boolean(flag) => Some(MapKeyLiteral::Boolean(*flag)),
    }
}

pub(crate) fn literal_map_key(node: &Node) -> Option<MapKeyLiteral> {
    match node {
        Node::Primitive { value } => literal_map_key_from_primitive(value),
        Node::Symbol { value } if value == "nil" => Some(MapKeyLiteral::Nil),
        _ => None,
    }
}

pub(crate) fn clone_runtime_for_kind(kind: ValueKind) -> Option<&'static str> {
    match kind {
        ValueKind::String => Some("_string_clone"),
        ValueKind::Vector => Some("_vector_clone"),
        ValueKind::Map => Some("_map_clone"),
        ValueKind::Set => Some("_set_clone"),
        _ => None,
    }
}

pub(crate) fn runtime_free_for_kind(kind: ValueKind) -> Option<&'static str> {
    match kind {
        ValueKind::Vector => Some("_vector_free"),
        ValueKind::Map => Some("_map_free"),
        ValueKind::Set => Some("_set_free"),
        _ => None,
    }
}

pub(crate) fn emit_free_for_slot(instructions: &mut Vec<IRInstruction>, slot: usize, kind: ValueKind) {
    if let Some(runtime) = runtime_free_for_kind(kind) {
        instructions.push(IRInstruction::FreeLocalWithRuntime(slot, runtime.to_string()));
    } else {
        instructions.push(IRInstruction::FreeLocal(slot));
    }
}

pub(crate) fn free_retained_slot(slot: RetainedSlot, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    slot.dependents.into_iter().for_each(|dependent| {
        free_retained_slot(dependent, instructions, context);
    });
    emit_free_for_slot(instructions, slot.slot, slot.kind);
    context.release_temp_slot(slot.slot);
}

pub(crate) fn free_retained_dependents(slot: &mut RetainedSlot, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    slot.dependents.drain(..).for_each(|dependent| {
        free_retained_slot(dependent, instructions, context);
    });
}

pub(crate) fn ensure_owned_on_stack(instructions: &mut Vec<IRInstruction>, kind: ValueKind, ownership: &mut HeapOwnership) {
    if *ownership == HeapOwnership::Owned {
        return;
    }

    if let Some(runtime) = clone_runtime_for_kind(kind) {
        instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
        *ownership = HeapOwnership::Owned;
    }
}

pub(crate) fn track_heap_slot(retained_slots: &mut Vec<RetainedSlot>, slot: usize, kind: ValueKind, key: Option<MapKeyLiteral>, dependents: Vec<RetainedSlot>) {
    if kind.is_heap_kind() {
        retained_slots.push(RetainedSlot { slot, key, kind, dependents });
    }
}

pub(crate) fn release_slots_for_literal(retained_slots: &mut Vec<RetainedSlot>, literal: &MapKeyLiteral, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    let (to_free, kept): (Vec<_>, Vec<_>) = retained_slots.drain(..).partition(|slot| slot.key.as_ref() == Some(literal));
    *retained_slots = kept;
    for slot_info in to_free {
        free_retained_slot(slot_info, instructions, context);
    }
}

pub(crate) fn take_slots_for_literal(retained_slots: &mut Vec<RetainedSlot>, literal: &MapKeyLiteral) -> Vec<RetainedSlot> {
    let (removed, kept): (Vec<_>, Vec<_>) = retained_slots.drain(..).partition(|slot| slot.key.as_ref() == Some(literal));
    *retained_slots = kept;
    removed
}

pub(crate) fn dedup_retained_slots(slots: &mut Vec<RetainedSlot>) {
    if slots.is_empty() {
        return;
    }
    slots.sort_by_key(|info| info.slot);
    slots.dedup_by(|a, b| {
        if a.slot == b.slot {
            a.dependents.extend(b.dependents.drain(..));
            true
        } else {
            false
        }
    });
}

pub(crate) fn retains_slot(slots: &[RetainedSlot], slot: usize) -> bool {
    slots.iter().any(|info| info.slot == slot)
}

pub(crate) fn discard_loaded_target(instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, owned_slot: Option<usize>) {
    if let Some(slot) = owned_slot {
        instructions.push(IRInstruction::StoreLocal(slot));
    } else {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        context.release_temp_slot(slot);
    }
}

pub(crate) enum DefaultHandling {
    None,
    Some(DefaultValue),
}

pub(crate) struct DefaultValue {
    slot: usize,
    owned: bool,
    kind: ValueKind,
    retained_slots: Vec<RetainedSlot>,
}

impl DefaultHandling {
    pub(super) fn from_parts(slot: Option<usize>, owned: bool, kind: ValueKind, mut retained_slots: Vec<RetainedSlot>) -> Self {
        dedup_retained_slots(&mut retained_slots);
        match slot {
            Some(slot) => DefaultHandling::Some(DefaultValue { slot, owned, kind, retained_slots }),
            None => DefaultHandling::None,
        }
    }

    pub(super) fn has_value(&self) -> bool {
        matches!(self, DefaultHandling::Some(_))
    }

    pub(super) fn success_cleanup(&mut self, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
        if let DefaultHandling::Some(default) = self {
            if default.owned {
                emit_free_for_slot(instructions, default.slot, default.kind);
            }
            default.retained_slots.drain(..).for_each(|slot| {
                free_retained_slot(slot, instructions, context);
            });
        }
    }

    pub(super) fn emit_fallback(&self, instructions: &mut Vec<IRInstruction>) {
        match self {
            DefaultHandling::Some(default) => {
                instructions.push(IRInstruction::LoadLocal(default.slot));
                if let Some(runtime) = clone_runtime_for_kind(default.kind) {
                    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
                    if default.owned {
                        emit_free_for_slot(instructions, default.slot, default.kind);
                    }
                } else if default.owned {
                    emit_free_for_slot(instructions, default.slot, default.kind);
                }
            }
            DefaultHandling::None => instructions.push(IRInstruction::Push(0)),
        }
    }

    pub(super) fn release_slot(&self, context: &mut CompileContext) {
        if let DefaultHandling::Some(default) = self {
            context.release_temp_slot(default.slot);
        }
    }

    pub(super) fn inferred_kind(&self) -> Option<ValueKind> {
        match self {
            DefaultHandling::Some(default) => Some(default.kind),
            DefaultHandling::None => None,
        }
    }

    pub(super) fn take_retained_slots(&mut self) -> Vec<RetainedSlot> {
        match self {
            DefaultHandling::Some(default) => std::mem::take(&mut default.retained_slots),
            DefaultHandling::None => Vec::new(),
        }
    }
}
