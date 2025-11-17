use super::super::ownership::{apply_plan_with_slot_kinds, dedup_retained_slots, ensure_owned_on_stack, release_slots_for_literal, track_heap_slot};
use super::common::{
    allocate_descending_slots, literal_map_key, release_all, release_slots, release_temp_slots, resolve_map_key_kind, resolve_value_kind, runtime_tag_for_key, runtime_tag_for_value,
    track_owned_argument,
};
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, liveness::compute_liveness_plan, CompileContext, CompileError, CompileResult, HeapOwnership, MapValueTypes, RetainedSlot, ValueKind};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

pub(crate) fn compile_hash_map(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() % 2 != 0 {
        return Err(CompileError::InvalidExpression("hash-map requires key/value pairs".to_string()));
    }

    let pair_count = args.len() / 2;
    if pair_count == 0 {
        return Ok(CompileResult::with_instructions(
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_map_create".to_string(), 5),
            ],
            ValueKind::Map,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let mut instructions = Vec::new();
    let mut map_value_types: Option<MapValueTypes> = None;
    let mut retained_slots: Vec<RetainedSlot> = Vec::new();

    let (ordered_key_value_slots, key_value_slots) = allocate_descending_slots(context, pair_count);
    let (ordered_key_tag_slots, key_tag_slots) = allocate_descending_slots(context, pair_count);
    let (ordered_value_slots, value_slots) = allocate_descending_slots(context, pair_count);
    let (ordered_value_tag_slots, value_tag_slots) = allocate_descending_slots(context, pair_count);

    let mut update_map_types = |key_node: &Node, value_kind: ValueKind| {
        if let Some(key_literal) = literal_map_key(key_node) {
            if value_kind == ValueKind::Any {
                if let Some(types) = map_value_types.as_mut() {
                    types.remove(&key_literal);
                }
            } else {
                map_value_types.get_or_insert_with(HashMap::new).insert(key_literal, value_kind);
            }
        } else {
            map_value_types = None;
        }
    };

    args.chunks_exact(2)
        .zip(ordered_key_value_slots.iter().copied())
        .zip(ordered_key_tag_slots.iter().copied())
        .zip(ordered_value_slots.iter().copied())
        .zip(ordered_value_tag_slots.iter().copied())
        .try_for_each(|((((pair, key_slot), key_tag_slot), value_slot), value_tag_slot)| -> Result<(), CompileError> {
            let key_node = &pair[0];
            let value_node = &pair[1];
            let key_literal = literal_map_key(key_node);

            let mut key_result = compile_node(key_node, context, program)?;
            let key_kind = resolve_map_key_kind(key_node, key_result.kind, context)?;
            let key_instructions = std::mem::take(&mut key_result.instructions);
            extend_with_offset(&mut instructions, key_instructions);
            ensure_owned_on_stack(&mut instructions, key_kind, &mut key_result.heap_ownership);
            let key_dependents = key_result.take_retained_slots();
            instructions.push(IRInstruction::StoreLocal(key_slot));
            track_heap_slot(&mut retained_slots, key_slot, key_kind, key_literal.clone(), key_dependents);
            instructions.push(IRInstruction::Push(runtime_tag_for_key(key_kind)));
            instructions.push(IRInstruction::StoreLocal(key_tag_slot));
            key_result.free_retained_slots(&mut instructions, context);

            let mut value_result = compile_node(value_node, context, program)?;
            let value_kind = resolve_value_kind(value_node, value_result.kind, context);
            let value_instructions = std::mem::take(&mut value_result.instructions);
            extend_with_offset(&mut instructions, value_instructions);
            ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
            let value_dependents = value_result.take_retained_slots();
            instructions.push(IRInstruction::StoreLocal(value_slot));
            track_heap_slot(&mut retained_slots, value_slot, value_kind, key_literal.clone(), value_dependents);
            instructions.push(IRInstruction::Push(runtime_tag_for_value(value_kind)));
            instructions.push(IRInstruction::StoreLocal(value_tag_slot));
            value_result.free_retained_slots(&mut instructions, context);

            update_map_types(key_node, value_kind);
            Ok(())
        })?;

    instructions.push(IRInstruction::PushLocalAddress(ordered_key_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_key_tag_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_value_tag_slots[0]));
    instructions.push(IRInstruction::Push(pair_count as i64));
    instructions.push(IRInstruction::RuntimeCall("_map_create".to_string(), 5));

    dedup_retained_slots(&mut retained_slots);

    release_slots(key_value_slots, &retained_slots, context);
    release_all(key_tag_slots, context);
    release_slots(value_slots, &retained_slots, context);
    release_all(value_tag_slots, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_assoc(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 3 {
        return Err(CompileError::ArityError("assoc".to_string(), 3, args.len()));
    }
    if (args.len() - 1) % 2 != 0 {
        return Err(CompileError::InvalidExpression("assoc expects key/value pairs".to_string()));
    }

    let mut base_result = compile_node(&args[0], context, program)?;
    let base_heap_ownership = base_result.heap_ownership;
    let mut map_value_types = base_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut base_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();

    if base_heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Map);
        temp_slots.push(slot);
    }

    args[1..].chunks_exact(2).try_for_each(|pair| -> Result<(), CompileError> {
        let key_node = &pair[0];
        let value_node = &pair[1];

        let key_literal = literal_map_key(key_node);
        if let Some(literal) = key_literal.as_ref() {
            release_slots_for_literal(&mut retained_slots, literal, &mut instructions, context);
        }
        let mut key_result = compile_node(key_node, context, program)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        key_result.kind = resolve_map_key_kind(key_node, key_result.kind, context)?;
        ensure_owned_on_stack(&mut instructions, key_result.kind, &mut key_result.heap_ownership);
        let key_dependents = key_result.take_retained_slots();
        if key_result.kind.is_heap_kind() {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            track_heap_slot(&mut retained_slots, slot, key_result.kind, key_literal.clone(), key_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
        key_result.free_retained_slots(&mut instructions, context);

        let mut value_result = compile_node(value_node, context, program)?;
        value_result.kind = resolve_value_kind(value_node, value_result.kind, context);
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_result.kind, &mut value_result.heap_ownership);
        let value_dependents = value_result.take_retained_slots();
        if value_result.kind.is_heap_kind() {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            track_heap_slot(&mut retained_slots, slot, value_result.kind, key_literal.clone(), value_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_result.kind)));
        value_result.free_retained_slots(&mut instructions, context);

        instructions.push(IRInstruction::RuntimeCall("_map_assoc".to_string(), 5));

        if let Some(key_literal) = literal_map_key(key_node) {
            if value_result.kind == ValueKind::Any {
                if let Some(types) = map_value_types.as_mut() {
                    types.remove(&key_literal);
                }
            } else {
                map_value_types.get_or_insert_with(HashMap::new).insert(key_literal, value_result.kind);
            }
        } else {
            map_value_types = None;
        }

        Ok(())
    })?;

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    release_temp_slots(temp_slots, context);

    dedup_retained_slots(&mut retained_slots);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_dissoc(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Err(CompileError::ArityError("dissoc".to_string(), 1, 0));
    }

    let mut base_result = compile_node(&args[0], context, program)?;
    if args.len() == 1 {
        return Ok(base_result);
    }

    let base_heap_ownership = base_result.heap_ownership;
    let mut map_value_types = base_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut base_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();

    if base_heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Map);
        temp_slots.push(slot);
    }

    args[1..].iter().try_for_each(|key| -> Result<(), CompileError> {
        let mut key_result = compile_node(key, context, program)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        let owned_key_slot = track_owned_argument(&key_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);
        key_result.kind = resolve_map_key_kind(key, key_result.kind, context)?;
        if let Some(slot) = owned_key_slot {
            slot_kinds.insert(slot, key_result.kind);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
        instructions.push(IRInstruction::RuntimeCall("_map_dissoc".to_string(), 3));
        key_result.free_retained_slots(&mut instructions, context);

        if let Some(key_literal) = literal_map_key(key) {
            release_slots_for_literal(&mut retained_slots, &key_literal, &mut instructions, context);
            if let Some(types) = map_value_types.as_mut() {
                types.remove(&key_literal);
            }
        } else {
            map_value_types = None;
        }
        Ok(())
    })?;

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    release_temp_slots(temp_slots, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_contains(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("contains?".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();

    track_owned_argument(&target_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);

    let target_kind = resolve_value_kind(&args[0], target_result.kind, context);
    if let Some(slot) = temp_slots.first() {
        slot_kinds.insert(*slot, target_kind);
    }

    let mut key_result = compile_node(&args[1], context, program)?;
    let key_instructions = std::mem::take(&mut key_result.instructions);
    extend_with_offset(&mut instructions, key_instructions);
    let owned_key_slot = track_owned_argument(&key_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);
    key_result.kind = resolve_map_key_kind(&args[1], key_result.kind, context)?;
    if let Some(slot) = owned_key_slot {
        slot_kinds.insert(slot, key_result.kind);
    }
    instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
    let runtime = if target_kind == ValueKind::Set { "_set_contains" } else { "_map_contains" };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));
    key_result.free_retained_slots(&mut instructions, context);

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    release_temp_slots(temp_slots, context);
    target_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}
