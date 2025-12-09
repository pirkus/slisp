/// Slot tracking utilities for managing temporary local variable slots during compilation.
///
/// This module provides a centralized way to allocate, track, and release temporary slots
/// used during code generation, along with liveness-aware freeing.
use super::{CompileContext, HeapOwnership, RetainedSlot, ValueKind};
use crate::compiler::builtins::emit_free_for_slot;
use crate::compiler::liveness::{apply_liveness_plan, compute_liveness_plan};
use crate::ir::IRInstruction;
use std::collections::{HashMap, HashSet};

/// Tracks temporary slots during compilation, handling allocation, liveness analysis, and cleanup.
pub struct SlotTracker {
    tracked_slots: HashSet<usize>,
    slot_kinds: HashMap<usize, ValueKind>,
    temp_slots: Vec<usize>,
    owned_slots: Vec<usize>,
}

impl SlotTracker {
    pub fn new() -> Self {
        SlotTracker {
            tracked_slots: HashSet::new(),
            slot_kinds: HashMap::new(),
            temp_slots: Vec::new(),
            owned_slots: Vec::new(),
        }
    }

    /// Track an owned heap value by storing it in a temporary slot.
    /// Returns the allocated slot index.
    pub fn track_owned(&mut self, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, kind: ValueKind) -> usize {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        self.tracked_slots.insert(slot);
        self.slot_kinds.insert(slot, kind);
        self.temp_slots.push(slot);
        self.owned_slots.push(slot);
        slot
    }

    /// Track an owned value if ownership matches, otherwise just return None.
    pub fn track_if_owned(&mut self, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, ownership: HeapOwnership, kind: ValueKind) -> Option<usize> {
        if ownership == HeapOwnership::Owned {
            Some(self.track_owned(instructions, context, kind))
        } else {
            None
        }
    }

    /// Update the kind for a tracked slot (useful when type is refined after initial tracking).
    pub fn set_slot_kind(&mut self, slot: usize, kind: ValueKind) {
        self.slot_kinds.insert(slot, kind);
    }

    /// Register a retained slot for tracking without allocating a new one.
    pub fn track_retained(&mut self, slot: &RetainedSlot) {
        self.tracked_slots.insert(slot.slot);
        self.slot_kinds.insert(slot.slot, slot.kind);
    }

    /// Register multiple retained slots for tracking.
    pub fn track_retained_slots(&mut self, slots: &[RetainedSlot]) {
        slots.iter().for_each(|slot| self.track_retained(slot));
    }

    /// Remove a slot from liveness tracking (useful when manually freeing).
    pub fn untrack(&mut self, slot: usize) {
        self.tracked_slots.remove(&slot);
        self.slot_kinds.remove(&slot);
    }

    /// Apply liveness-aware freeing to the instruction stream and release all temp slots.
    pub fn apply_liveness_and_release(self, instructions: Vec<IRInstruction>, context: &mut CompileContext) -> Vec<IRInstruction> {
        let result = if self.tracked_slots.is_empty() {
            instructions
        } else {
            let plan = compute_liveness_plan(&instructions, &self.tracked_slots);
            apply_liveness_plan(instructions, &plan, |insts, slot| {
                let kind = self.slot_kinds.get(&slot).copied().unwrap_or(ValueKind::Any);
                emit_free_for_slot(insts, slot, kind);
            })
        };

        self.temp_slots.into_iter().for_each(|slot| {
            context.release_temp_slot(slot);
        });

        result
    }
}

impl Default for SlotTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_owned_allocates_and_records() {
        let mut context = CompileContext::new();
        let mut instructions = Vec::new();
        let mut tracker = SlotTracker::new();

        let slot = tracker.track_owned(&mut instructions, &mut context, ValueKind::String);

        assert_eq!(slot, 0);
        assert_eq!(instructions.len(), 2);
        assert!(matches!(instructions[0], IRInstruction::StoreLocal(0)));
        assert!(matches!(instructions[1], IRInstruction::LoadLocal(0)));
    }

    #[test]
    fn track_if_owned_respects_ownership() {
        let mut context = CompileContext::new();
        let mut instructions = Vec::new();
        let mut tracker = SlotTracker::new();

        let none_result = tracker.track_if_owned(&mut instructions, &mut context, HeapOwnership::None, ValueKind::String);
        assert!(none_result.is_none());
        assert!(instructions.is_empty());

        let owned_result = tracker.track_if_owned(&mut instructions, &mut context, HeapOwnership::Owned, ValueKind::String);
        assert!(owned_result.is_some());
        assert_eq!(instructions.len(), 2);
    }

    #[test]
    fn apply_liveness_releases_slots() {
        let mut context = CompileContext::new();
        let mut instructions = Vec::new();
        let mut tracker = SlotTracker::new();

        tracker.track_owned(&mut instructions, &mut context, ValueKind::String);
        tracker.track_owned(&mut instructions, &mut context, ValueKind::Vector);

        assert_eq!(context.next_slot, 2);
        let result = tracker.apply_liveness_and_release(instructions, &mut context);

        // Slots should be returned to free pool and frees emitted
        assert!(result.iter().any(|inst| matches!(inst, IRInstruction::FreeLocal(_) | IRInstruction::FreeLocalWithRuntime(_, _))));
        assert_eq!(context.free_slots.len(), 2);
    }
}
