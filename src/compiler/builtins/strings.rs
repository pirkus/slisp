use super::helpers::resolve_value_kind;
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, is_heap_allocated_symbol, slots::SlotTracker, CompileContext, CompileError, CompileResult, HeapOwnership, ValueKind};
use crate::ir::{IRInstruction, IRProgram};

pub(crate) fn compile_count(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("count".to_string(), 1, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracker = SlotTracker::new();

    if let Some(slot) = tracker.track_if_owned(&mut instructions, context, arg_result.heap_ownership, ValueKind::Any) {
        let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);
        tracker.set_slot_kind(slot, target_kind);
    }

    let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);

    let runtime = match target_kind {
        ValueKind::Vector => "_vector_count",
        ValueKind::Map => "_map_count",
        ValueKind::Set => "_set_count",
        _ => "_string_count",
    };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));

    instructions = tracker.apply_liveness_and_release(instructions, context);
    arg_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Number))
}

pub(crate) fn emit_string_get(instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, default: &mut super::helpers::DefaultHandling) {
    instructions.push(IRInstruction::RuntimeCall("_string_get".to_string(), 2));

    let result_slot = context.allocate_temp_slot();
    instructions.push(IRInstruction::StoreLocal(result_slot));
    instructions.push(IRInstruction::LoadLocal(result_slot));
    let fallback_jump_pos = instructions.len();
    instructions.push(IRInstruction::JumpIfZero(0));

    instructions.push(IRInstruction::LoadLocal(result_slot));
    default.success_cleanup(instructions, context);
    let success_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let fallback_block_pos = instructions.len();
    instructions[fallback_jump_pos] = IRInstruction::JumpIfZero(fallback_block_pos);

    default.emit_fallback(instructions);

    let end_pos = instructions.len();
    instructions[success_jump_pos] = IRInstruction::Jump(end_pos);

    context.release_temp_slot(result_slot);
}

pub(crate) fn compile_subs(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("subs".to_string(), 2, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracker = SlotTracker::new();

    let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);

    if let Some(slot) = tracker.track_if_owned(&mut instructions, context, arg_result.heap_ownership, ValueKind::Any) {
        tracker.set_slot_kind(slot, target_kind);
    }

    let mut start_result = compile_node(&args[1], context, program)?;
    let start_instructions = std::mem::take(&mut start_result.instructions);
    extend_with_offset(&mut instructions, start_instructions);
    start_result.free_retained_slots(&mut instructions, context);

    if args.len() == 3 {
        let mut end_result = compile_node(&args[2], context, program)?;
        let end_instructions = std::mem::take(&mut end_result.instructions);
        extend_with_offset(&mut instructions, end_instructions);
        end_result.free_retained_slots(&mut instructions, context);
    } else {
        instructions.push(IRInstruction::Push(-1));
    }

    let runtime = if target_kind == ValueKind::Vector { "_vector_slice" } else { "_string_subs" };

    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));

    instructions = tracker.apply_liveness_and_release(instructions, context);
    arg_result.free_retained_slots(&mut instructions, context);

    let result_kind = if target_kind == ValueKind::Vector { ValueKind::Vector } else { ValueKind::String };

    Ok(CompileResult::with_instructions(instructions, result_kind).with_heap_ownership(HeapOwnership::Owned))
}

pub(crate) fn compile_str(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![IRInstruction::Push(0), IRInstruction::Push(0), IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2)],
            ValueKind::String,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = args.len();
    let mut instructions = Vec::new();
    let temp_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_slots = temp_slots.clone();
    ordered_slots.sort_unstable();
    ordered_slots.reverse();

    let mut needs_free = Vec::with_capacity(count);

    for (arg, slot) in args.iter().zip(ordered_slots.iter()) {
        let mut arg_result = compile_node(arg, context, program)?;
        let arg_instructions = std::mem::take(&mut arg_result.instructions);
        extend_with_offset(&mut instructions, arg_instructions);

        let mut slot_needs_free = arg_result.heap_ownership == HeapOwnership::Owned;

        let mut arg_kind = arg_result.kind;
        if arg_kind == ValueKind::Any {
            if let Node::Symbol { value } = arg {
                if let Some(var_kind) = context.get_variable_type(value).or_else(|| context.get_parameter_type(value)) {
                    if var_kind != ValueKind::Any {
                        arg_kind = var_kind;
                    } else if context.get_parameter(value).is_some() {
                        context.mark_heap_allocated(value, ValueKind::String);
                        arg_kind = ValueKind::String;
                    }
                } else if context.get_parameter(value).is_some() {
                    context.mark_heap_allocated(value, ValueKind::String);
                    arg_kind = ValueKind::String;
                }
            }
        }

        match arg_kind {
            ValueKind::String => {
                let clone_flag = if let Node::Symbol { value } = arg {
                    if is_heap_allocated_symbol(value, context) {
                        1
                    } else {
                        0
                    }
                } else {
                    0
                };
                instructions.push(IRInstruction::Push(clone_flag));
                instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
                if clone_flag != 0 {
                    slot_needs_free = true;
                }
            }
            ValueKind::Keyword => {
                instructions.push(IRInstruction::Push(0));
                instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
                slot_needs_free = false;
            }
            ValueKind::Nil => {
                instructions.push(IRInstruction::Push(0));
                instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
                slot_needs_free = false;
            }
            ValueKind::Vector => {
                instructions.push(IRInstruction::RuntimeCall("_vector_to_string".to_string(), 1));
                slot_needs_free = true;
            }
            ValueKind::Map => {
                instructions.push(IRInstruction::RuntimeCall("_map_to_string".to_string(), 1));
                slot_needs_free = true;
            }
            ValueKind::Set => {
                instructions.push(IRInstruction::RuntimeCall("_set_to_string".to_string(), 1));
                slot_needs_free = true;
            }
            ValueKind::Boolean => {
                instructions.push(IRInstruction::RuntimeCall("_string_from_boolean".to_string(), 1));
                slot_needs_free = false;
            }
            ValueKind::Number | ValueKind::Any => {
                instructions.push(IRInstruction::RuntimeCall("_string_from_number".to_string(), 1));
                slot_needs_free = true;
            }
        }

        instructions.push(IRInstruction::StoreLocal(*slot));
        needs_free.push(slot_needs_free);
        arg_result.free_retained_slots(&mut instructions, context);
    }

    let base_slot = ordered_slots[0];
    instructions.push(IRInstruction::PushLocalAddress(base_slot));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2));

    ordered_slots.iter().zip(needs_free.iter()).filter(|(_, free)| **free).for_each(|(slot, _)| {
        instructions.push(IRInstruction::FreeLocal(*slot));
    });

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}
