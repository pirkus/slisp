use crate::compiler::{
    liveness::{apply_liveness_plan, LivenessPlan},
    CompileContext, HeapOwnership, MapKeyLiteral, RetainedSlot, ValueKind,
};
use crate::ir::IRInstruction;
use std::collections::HashMap;

pub(super) fn clone_runtime_for_kind(kind: ValueKind) -> Option<&'static str> {
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
    for dependent in slot.dependents {
        free_retained_slot(dependent, instructions, context);
    }
    emit_free_for_slot(instructions, slot.slot, slot.kind);
    context.release_temp_slot(slot.slot);
}

pub(crate) fn free_retained_dependents(slot: &mut RetainedSlot, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    for dependent in slot.dependents.drain(..) {
        free_retained_slot(dependent, instructions, context);
    }
}

pub(super) fn apply_plan_with_slot_kinds(instructions: Vec<IRInstruction>, plan: &LivenessPlan, slot_kinds: &HashMap<usize, ValueKind>) -> Vec<IRInstruction> {
    apply_liveness_plan(instructions, plan, |insts, slot| {
        let kind = slot_kinds.get(&slot).copied().unwrap_or(ValueKind::Any);
        emit_free_for_slot(insts, slot, kind);
    })
}

pub(super) fn ensure_owned_on_stack(instructions: &mut Vec<IRInstruction>, kind: ValueKind, ownership: &mut HeapOwnership) {
    if *ownership == HeapOwnership::Owned {
        return;
    }

    if let Some(runtime) = clone_runtime_for_kind(kind) {
        instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
        *ownership = HeapOwnership::Owned;
    }
}

pub(super) fn track_heap_slot(retained_slots: &mut Vec<RetainedSlot>, slot: usize, kind: ValueKind, key: Option<MapKeyLiteral>, dependents: Vec<RetainedSlot>) {
    if kind.is_heap_kind() {
        retained_slots.push(RetainedSlot { slot, key, kind, dependents });
    }
}

pub(super) fn release_slots_for_literal(retained_slots: &mut Vec<RetainedSlot>, literal: &MapKeyLiteral, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    let mut idx = 0;
    while idx < retained_slots.len() {
        if retained_slots[idx].key.as_ref() == Some(literal) {
            let slot_info = retained_slots.remove(idx);
            free_retained_slot(slot_info, instructions, context);
        } else {
            idx += 1;
        }
    }
}

pub(super) fn dedup_retained_slots(slots: &mut Vec<RetainedSlot>) {
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

pub(super) fn retains_slot(slots: &[RetainedSlot], slot: usize) -> bool {
    slots.iter().any(|info| info.slot == slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_slot(slot: usize) -> RetainedSlot {
        RetainedSlot {
            slot,
            key: Some(MapKeyLiteral::Number(slot as i64)),
            kind: ValueKind::String,
            dependents: Vec::new(),
        }
    }

    #[test]
    fn retains_slot_detects_matching_slot() {
        let slots = vec![sample_slot(1), sample_slot(3)];
        assert!(retains_slot(&slots, 3));
        assert!(!retains_slot(&slots, 2));
    }

    #[test]
    fn release_slots_for_literal_removes_and_frees() {
        let mut slots = vec![sample_slot(1), sample_slot(2)];
        let mut instructions = Vec::new();
        let mut context = CompileContext::new();
        release_slots_for_literal(&mut slots, &MapKeyLiteral::Number(1), &mut instructions, &mut context);
        assert_eq!(slots.len(), 1);
    }
}
