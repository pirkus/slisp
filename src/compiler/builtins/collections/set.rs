use super::super::ownership::{apply_plan_with_slot_kinds, dedup_retained_slots, ensure_owned_on_stack, release_slots_for_literal, track_heap_slot};
use super::common::{allocate_descending_slots, literal_map_key, release_all, release_slots, release_temp_slots, resolve_map_key_kind, runtime_tag_for_key, track_owned_argument};
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, liveness::compute_liveness_plan, CompileContext, CompileError, CompileResult, HeapOwnership, RetainedSlot, ValueKind};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

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

    let (ordered_value_slots, value_slots) = allocate_descending_slots(context, count);
    let (ordered_tag_slots, tag_slots) = allocate_descending_slots(context, count);

    let mut retained_slots: Vec<RetainedSlot> = Vec::new();

    args.iter()
        .zip(ordered_value_slots.iter().copied())
        .zip(ordered_tag_slots.iter().copied())
        .try_for_each(|((value_node, value_slot), tag_slot)| -> Result<(), CompileError> {
            let value_literal = literal_map_key(value_node);
            let mut value_result = compile_node(value_node, context, program)?;
            let value_kind = resolve_map_key_kind(value_node, value_result.kind, context)?;
            let value_instructions = std::mem::take(&mut value_result.instructions);
            extend_with_offset(&mut instructions, value_instructions);
            ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
            let value_dependents = value_result.take_retained_slots();
            instructions.push(IRInstruction::StoreLocal(value_slot));
            track_heap_slot(&mut retained_slots, value_slot, value_kind, value_literal.clone(), value_dependents);

            instructions.push(IRInstruction::Push(runtime_tag_for_key(value_kind)));
            instructions.push(IRInstruction::StoreLocal(tag_slot));
            value_result.free_retained_slots(&mut instructions, context);
            Ok(())
        })?;

    instructions.push(IRInstruction::PushLocalAddress(ordered_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_tag_slots[0]));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_set_create".to_string(), 3));

    dedup_retained_slots(&mut retained_slots);

    release_slots(value_slots, &retained_slots, context);
    release_all(tag_slots, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
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
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();
    if base_result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Set);
        temp_slots.push(slot);
    }

    args[1..].iter().try_for_each(|value_node| -> Result<(), CompileError> {
        let value_literal = literal_map_key(value_node);
        let mut value_result = compile_node(value_node, context, program)?;
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        let owned_value_slot = track_owned_argument(&value_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);
        value_result.kind = resolve_map_key_kind(value_node, value_result.kind, context)?;
        if let Some(slot) = owned_value_slot {
            slot_kinds.insert(slot, value_result.kind);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(value_result.kind)));
        instructions.push(IRInstruction::RuntimeCall("_set_disj".to_string(), 3));
        value_result.free_retained_slots(&mut instructions, context);

        if let Some(literal) = value_literal {
            release_slots_for_literal(&mut retained_slots, &literal, &mut instructions, context);
        }
        Ok(())
    })?;

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    release_temp_slots(temp_slots, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}
