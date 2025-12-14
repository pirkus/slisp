use super::helpers::{dedup_retained_slots, ensure_owned_on_stack, literal_map_key, release_slots_for_literal, resolve_map_key_kind, retains_slot, runtime_tag_for_key, track_heap_slot};
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, slots::SlotTracker, CompileContext, CompileError, CompileResult, HeapOwnership, RetainedSlot, ValueKind};
use crate::ir::{IRInstruction, IRProgram};

pub(crate) fn compile_set_literal(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_set_create".to_string(), 3),
            ],
            ValueKind::Set,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = args.len();
    let mut instructions = Vec::new();

    let value_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_value_slots = value_slots.clone();
    ordered_value_slots.sort_unstable();
    ordered_value_slots.reverse();

    let tag_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_tag_slots = tag_slots.clone();
    ordered_tag_slots.sort_unstable();
    ordered_tag_slots.reverse();

    let mut retained_slots: Vec<RetainedSlot> = Vec::new();
    let mut element_kind_accumulator: Option<ValueKind> = None;

    for idx in 0..count {
        let value_node = &args[idx];
        let value_slot = ordered_value_slots[idx];
        let tag_slot = ordered_tag_slots[idx];

        let value_literal = literal_map_key(value_node);
        let mut value_result = compile_node(value_node, context, program)?;
        let value_kind = resolve_map_key_kind(value_node, value_result.kind, context)?;
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
        if value_kind != ValueKind::Any {
            if let Some(existing) = element_kind_accumulator {
                if existing != value_kind {
                    element_kind_accumulator = Some(ValueKind::Any);
                }
            } else {
                element_kind_accumulator = Some(value_kind);
            }
        }
        let value_dependents = value_result.take_retained_slots();
        instructions.push(IRInstruction::StoreLocal(value_slot));
        track_heap_slot(&mut retained_slots, value_slot, value_kind, value_literal.clone(), value_dependents);

        instructions.push(IRInstruction::Push(runtime_tag_for_key(value_kind)));
        instructions.push(IRInstruction::StoreLocal(tag_slot));
        value_result.free_retained_slots(&mut instructions, context);
    }

    instructions.push(IRInstruction::PushLocalAddress(ordered_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_tag_slots[0]));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_set_create".to_string(), 3));

    dedup_retained_slots(&mut retained_slots);

    value_slots.into_iter().filter(|slot| !retains_slot(&retained_slots, *slot)).for_each(|slot| {
        context.release_temp_slot(slot);
    });
    tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_set_element_kind(element_kind_accumulator)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_disj(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Err(CompileError::ArityError("disj".to_string(), 1, 0));
    }

    let mut base_result = compile_node(&args[0], context, program)?;
    if args.len() == 1 {
        let mut result = base_result;
        if result.kind != ValueKind::Set {
            result.kind = ValueKind::Set;
        }
        return Ok(result);
    }

    let mut instructions = std::mem::take(&mut base_result.instructions);
    let mut tracker = SlotTracker::new();
    let mut retained_slots = base_result.take_retained_slots();
    tracker.track_if_owned(&mut instructions, context, base_result.heap_ownership, ValueKind::Set);

    for value_idx in 1..args.len() {
        let value_literal = literal_map_key(&args[value_idx]);
        let mut value_result = compile_node(&args[value_idx], context, program)?;
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        let owned_value_slot = tracker.track_if_owned(&mut instructions, context, value_result.heap_ownership, ValueKind::Any);
        value_result.kind = resolve_map_key_kind(&args[value_idx], value_result.kind, context)?;
        owned_value_slot.into_iter().for_each(|slot| tracker.set_slot_kind(slot, value_result.kind));
        instructions.push(IRInstruction::Push(runtime_tag_for_key(value_result.kind)));
        instructions.push(IRInstruction::RuntimeCall("_set_disj".to_string(), 3));
        value_result.free_retained_slots(&mut instructions, context);

        if let Some(literal) = value_literal {
            release_slots_for_literal(&mut retained_slots, &literal, &mut instructions, context);
        }
    }

    instructions = tracker.apply_liveness_and_release(instructions, context);
    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}
