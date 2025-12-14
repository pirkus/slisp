use super::helpers::{
    dedup_retained_slots, discard_loaded_target, ensure_owned_on_stack, literal_map_key, release_slots_for_literal, resolve_map_key_kind, resolve_value_kind, runtime_tag_for_key,
    runtime_tag_for_value, take_slots_for_literal, DefaultHandling,
};
use super::strings::emit_string_get;
use super::vectors::emit_vector_get;
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, slots::SlotTracker, CompileContext, CompileError, CompileResult, HeapOwnership, MapValueTypes, ValueKind};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::HashMap;

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

    let key_value_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_key_value_slots = key_value_slots.clone();
    ordered_key_value_slots.sort_unstable();
    ordered_key_value_slots.reverse();

    let key_tag_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_key_tag_slots = key_tag_slots.clone();
    ordered_key_tag_slots.sort_unstable();
    ordered_key_tag_slots.reverse();

    let value_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_value_slots = value_slots.clone();
    ordered_value_slots.sort_unstable();
    ordered_value_slots.reverse();

    let value_tag_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_value_tag_slots = value_tag_slots.clone();
    ordered_value_tag_slots.sort_unstable();
    ordered_value_tag_slots.reverse();

    for idx in 0..pair_count {
        let key_node = &args[idx * 2];
        let value_node = &args[idx * 2 + 1];
        let key_literal = literal_map_key(key_node);

        let key_slot = ordered_key_value_slots[idx];
        let key_tag_slot = ordered_key_tag_slots[idx];
        let value_slot = ordered_value_slots[idx];
        let value_tag_slot = ordered_value_tag_slots[idx];

        let mut key_result = compile_node(key_node, context, program)?;
        let key_kind = resolve_map_key_kind(key_node, key_result.kind, context)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        ensure_owned_on_stack(&mut instructions, key_kind, &mut key_result.heap_ownership);
        instructions.push(IRInstruction::StoreLocal(key_slot));
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_kind)));
        instructions.push(IRInstruction::StoreLocal(key_tag_slot));
        key_result.free_retained_slots(&mut instructions, context);

        let mut value_result = compile_node(value_node, context, program)?;
        let value_kind = resolve_value_kind(value_node, value_result.kind, context);
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
        instructions.push(IRInstruction::StoreLocal(value_slot));
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_kind)));
        instructions.push(IRInstruction::StoreLocal(value_tag_slot));
        if value_kind.is_heap_kind() {
            instructions.push(IRInstruction::LoadLocal(value_slot));
            instructions.push(IRInstruction::LoadLocal(value_tag_slot));
            instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
            instructions.push(IRInstruction::StoreLocal(value_slot));
        }
        value_result.free_retained_slots(&mut instructions, context);

        if let Some(key_literal) = key_literal {
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
    }

    instructions.push(IRInstruction::PushLocalAddress(ordered_key_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_key_tag_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_value_tag_slots[0]));
    instructions.push(IRInstruction::Push(pair_count as i64));
    instructions.push(IRInstruction::RuntimeCall("_map_create".to_string(), 5));

    key_value_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
    key_tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
    value_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
    value_tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types))
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
    let mut tracker = SlotTracker::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();

    tracker.track_if_owned(&mut instructions, context, base_heap_ownership, ValueKind::Map);

    for pair_idx in 0..((args.len() - 1) / 2) {
        let key_index = 1 + pair_idx * 2;
        let value_index = key_index + 1;

        let key_literal = literal_map_key(&args[key_index]);
        let mut slots_to_free_after_call = Vec::new();
        if let Some(literal) = key_literal.as_ref() {
            slots_to_free_after_call = take_slots_for_literal(&mut retained_slots, literal);
        }
        let mut key_result = compile_node(&args[key_index], context, program)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        key_result.kind = resolve_map_key_kind(&args[key_index], key_result.kind, context)?;
        ensure_owned_on_stack(&mut instructions, key_result.kind, &mut key_result.heap_ownership);
        let key_dependents = key_result.take_retained_slots();
        if key_result.kind.is_heap_kind() {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            temp_slots.push(slot);
            drop(key_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
        key_result.free_retained_slots(&mut instructions, context);

        let mut value_result = compile_node(&args[value_index], context, program)?;
        value_result.kind = resolve_value_kind(&args[value_index], value_result.kind, context);
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_result.kind, &mut value_result.heap_ownership);
        let value_dependents = value_result.take_retained_slots();
        if value_result.kind.is_heap_kind() {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            instructions.push(IRInstruction::Push(runtime_tag_for_value(value_result.kind)));
            instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            temp_slots.push(slot);
            drop(value_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_result.kind)));
        value_result.free_retained_slots(&mut instructions, context);

        instructions.push(IRInstruction::RuntimeCall("_map_assoc".to_string(), 5));
        slots_to_free_after_call.into_iter().for_each(|slot| {
            super::helpers::free_retained_slot(slot, &mut instructions, context);
        });

        if let Some(key_literal) = literal_map_key(&args[key_index]) {
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
    }

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    instructions = tracker.apply_liveness_and_release(instructions, context);
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
    let mut tracker = SlotTracker::new();
    let mut retained_slots = base_result.take_retained_slots();

    tracker.track_if_owned(&mut instructions, context, base_heap_ownership, ValueKind::Map);

    for key_idx in 1..args.len() {
        let mut key_result = compile_node(&args[key_idx], context, program)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        let owned_key_slot = tracker.track_if_owned(&mut instructions, context, key_result.heap_ownership, ValueKind::Any);
        key_result.kind = resolve_map_key_kind(&args[key_idx], key_result.kind, context)?;
        owned_key_slot.into_iter().for_each(|slot| tracker.set_slot_kind(slot, key_result.kind));
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
        instructions.push(IRInstruction::RuntimeCall("_map_dissoc".to_string(), 3));
        key_result.free_retained_slots(&mut instructions, context);

        if let Some(key_literal) = literal_map_key(&args[key_idx]) {
            release_slots_for_literal(&mut retained_slots, &key_literal, &mut instructions, context);
            if let Some(types) = map_value_types.as_mut() {
                types.remove(&key_literal);
            }
        } else {
            map_value_types = None;
        }
    }

    instructions = tracker.apply_liveness_and_release(instructions, context);
    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_get(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("get".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let target_map_value_types = target_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracker = SlotTracker::new();
    let mut temp_slots = Vec::new();

    let owned_arg_slot = tracker.track_if_owned(&mut instructions, context, target_result.heap_ownership, ValueKind::Any);

    let mut key_result = compile_node(&args[1], context, program)?;
    let key_ownership = key_result.heap_ownership;
    let mut key_kind = key_result.kind;
    let key_instructions = std::mem::take(&mut key_result.instructions);
    extend_with_offset(&mut instructions, key_instructions);
    key_result.free_retained_slots(&mut instructions, context);

    let mut default_slot = None;
    let mut default_owned = false;
    let mut default_kind = ValueKind::Any;
    let mut default_retained_slots = Vec::new();

    if args.len() == 3 {
        let mut default_result = compile_node(&args[2], context, program)?;
        default_kind = resolve_value_kind(&args[2], default_result.kind, context);
        default_retained_slots = default_result.take_retained_slots();
        extend_with_offset(&mut instructions, default_result.instructions);
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        default_owned = default_result.heap_ownership == HeapOwnership::Owned;
        default_slot = Some(slot);
    }

    let mut default_handling = DefaultHandling::from_parts(default_slot, default_owned, default_kind, default_retained_slots);
    let target_kind = resolve_value_kind(&args[0], target_result.kind, context);

    owned_arg_slot.into_iter().for_each(|slot| tracker.set_slot_kind(slot, target_kind));

    match target_kind {
        ValueKind::Vector => {
            emit_vector_get(&mut instructions, context, &mut tracker, owned_arg_slot, &mut default_handling);
        }
        ValueKind::Map => {
            let owned_key_slot = tracker.track_if_owned(&mut instructions, context, key_ownership, ValueKind::Any);

            key_kind = resolve_map_key_kind(&args[1], key_kind, context)?;
            owned_key_slot.into_iter().for_each(|slot| tracker.set_slot_kind(slot, key_kind));
            let key_tag = runtime_tag_for_key(key_kind);
            instructions.push(IRInstruction::Push(key_tag));

            let value_slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::Push(0));
            instructions.push(IRInstruction::StoreLocal(value_slot));
            instructions.push(IRInstruction::PushLocalAddress(value_slot));
            temp_slots.push(value_slot);

            let tag_slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::Push(0));
            instructions.push(IRInstruction::StoreLocal(tag_slot));
            instructions.push(IRInstruction::PushLocalAddress(tag_slot));
            temp_slots.push(tag_slot);

            instructions.push(IRInstruction::RuntimeCall("_map_get".to_string(), 5));

            let failure_jump_pos = instructions.len();
            instructions.push(IRInstruction::JumpIfZero(0));

            let inferred_value_kind = target_map_value_types.as_ref().and_then(|types| literal_map_key(&args[1]).and_then(|key| types.get(&key).copied()));
            let needs_clone = inferred_value_kind.map(|kind| kind.is_heap_kind()).unwrap_or(true);
            if needs_clone {
                instructions.push(IRInstruction::LoadLocal(value_slot));
                instructions.push(IRInstruction::LoadLocal(tag_slot));
                instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
                instructions.push(IRInstruction::StoreLocal(value_slot));
            }

            instructions.push(IRInstruction::LoadLocal(value_slot));
            default_handling.success_cleanup(&mut instructions, context);
            let success_jump_pos = instructions.len();
            instructions.push(IRInstruction::Jump(0));

            let failure_block_pos = instructions.len();
            instructions[failure_jump_pos] = IRInstruction::JumpIfZero(failure_block_pos);

            default_handling.emit_fallback(&mut instructions);

            let end_pos = instructions.len();
            instructions[success_jump_pos] = IRInstruction::Jump(end_pos);
        }
        _ => {
            emit_string_get(&mut instructions, context, &mut default_handling);
        }
    }

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    instructions = tracker.apply_liveness_and_release(instructions, context);
    default_handling.release_slot(context);
    target_result.free_retained_slots(&mut instructions, context);

    let inferred_map_value_kind = target_map_value_types.as_ref().and_then(|types| literal_map_key(&args[1]).and_then(|key| types.get(&key).copied()));

    let result_kind = match target_kind {
        ValueKind::Vector => default_handling.inferred_kind().unwrap_or(ValueKind::Any),
        ValueKind::Map => inferred_map_value_kind.or_else(|| default_handling.inferred_kind()).unwrap_or(ValueKind::Any),
        _ if default_handling.has_value() => default_handling.inferred_kind().unwrap_or(ValueKind::String),
        _ => ValueKind::String,
    };

    let map_needs_clone_flag = match target_kind {
        ValueKind::Map => inferred_map_value_kind.map(|kind| kind.is_heap_kind()).unwrap_or(true),
        _ => false,
    };

    let heap_ownership = match target_kind {
        ValueKind::Vector => HeapOwnership::None,
        ValueKind::Map => match (inferred_map_value_kind, target_result.heap_ownership) {
            (Some(kind), HeapOwnership::Owned) if kind.is_heap_kind() => HeapOwnership::Owned,
            (Some(kind), _) if kind.is_heap_kind() => HeapOwnership::Borrowed,
            _ if result_kind.is_heap_clone_kind() || map_needs_clone_flag => HeapOwnership::Owned,
            _ => HeapOwnership::None,
        },
        _ => HeapOwnership::Owned,
    };

    let retained_slots = default_handling.take_retained_slots();

    Ok(CompileResult::with_instructions(instructions, result_kind)
        .with_heap_ownership(heap_ownership)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_contains(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("contains?".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracker = SlotTracker::new();

    let owned_target_slot = tracker.track_if_owned(&mut instructions, context, target_result.heap_ownership, ValueKind::Any);

    let target_kind = resolve_value_kind(&args[0], target_result.kind, context);
    if let Some(slot) = owned_target_slot {
        tracker.set_slot_kind(slot, target_kind);
    }

    if target_kind == ValueKind::Map {
        if let Some(map_value_types) = target_result.map_value_types.as_ref() {
            if let Some(key_literal) = literal_map_key(&args[1]) {
                if map_value_types.contains_key(&key_literal) {
                    discard_loaded_target(&mut instructions, context, owned_target_slot);
                    instructions.push(IRInstruction::Push(1));

                    instructions = tracker.apply_liveness_and_release(instructions, context);
                    target_result.free_retained_slots(&mut instructions, context);

                    return Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean));
                }
            }
        }
    }

    let mut key_result = compile_node(&args[1], context, program)?;
    let key_instructions = std::mem::take(&mut key_result.instructions);
    extend_with_offset(&mut instructions, key_instructions);

    let owned_key_slot = tracker.track_if_owned(&mut instructions, context, key_result.heap_ownership, ValueKind::Any);

    key_result.kind = resolve_map_key_kind(&args[1], key_result.kind, context)?;
    if let Some(slot) = owned_key_slot {
        tracker.set_slot_kind(slot, key_result.kind);
    }
    instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
    let runtime = if target_kind == ValueKind::Set { "_set_contains" } else { "_map_contains" };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));
    key_result.free_retained_slots(&mut instructions, context);

    instructions = tracker.apply_liveness_and_release(instructions, context);
    target_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Primitive;
    use crate::compiler::MapKeyLiteral;

    #[test]
    fn map_literal_carries_value_metadata() {
        let mut context = CompileContext::new();
        let mut program = IRProgram::new();
        let args = vec![
            Node::Primitive {
                value: Primitive::Keyword("nums".to_string()),
            },
            Node::Set {
                root: vec![Node::Primitive { value: Primitive::Number(1) }, Node::Primitive { value: Primitive::Number(2) }],
            },
            Node::Primitive {
                value: Primitive::Keyword("letters".to_string()),
            },
            Node::Vector {
                root: vec![
                    Node::Primitive {
                        value: Primitive::String("a".to_string()),
                    },
                    Node::Primitive {
                        value: Primitive::String("b".to_string()),
                    },
                ],
            },
        ];

        let result = compile_hash_map(&args, &mut context, &mut program).unwrap();
        let metadata = result.map_value_types.expect("expected map metadata");
        assert_eq!(metadata.get(&MapKeyLiteral::Keyword("nums".to_string())), Some(&ValueKind::Set));
        assert_eq!(metadata.get(&MapKeyLiteral::Keyword("letters".to_string())), Some(&ValueKind::Vector));
    }

    #[test]
    fn compile_get_uses_map_metadata_for_literals() {
        let mut context = CompileContext::new();
        let slot = context.add_variable("m".to_string());
        context.set_variable_type("m", ValueKind::Map);
        context.mark_heap_allocated("m", ValueKind::Map);
        let mut metadata = MapValueTypes::new();
        metadata.insert(MapKeyLiteral::String("a".to_string()), ValueKind::String);
        context.set_variable_map_value_types("m", Some(metadata));
        let mut program = IRProgram::new();
        let args = vec![
            Node::Symbol { value: "m".to_string() },
            Node::Primitive {
                value: Primitive::String("a".to_string()),
            },
        ];
        let result = compile_get(&args, &mut context, &mut program).unwrap();
        assert_eq!(result.kind, ValueKind::String);
        assert_eq!(result.heap_ownership, HeapOwnership::Borrowed);
        assert!(context.get_variable("m").is_some());
        assert_eq!(slot, 0);
    }
}
