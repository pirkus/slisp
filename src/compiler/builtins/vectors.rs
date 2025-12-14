use super::helpers::{dedup_retained_slots, ensure_owned_on_stack, retains_slot, track_heap_slot, DefaultHandling};
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, slots::SlotTracker, CompileContext, CompileError, CompileResult, HeapOwnership, RetainedSlot, ValueKind};
use crate::ir::{IRInstruction, IRProgram};

pub(crate) fn compile_vector_literal(elements: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if elements.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_vector_create".to_string(), 3),
            ],
            ValueKind::Vector,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = elements.len();
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
        let element = &elements[idx];
        let value_slot = ordered_value_slots[idx];
        let tag_slot = ordered_tag_slots[idx];

        let mut element_result = compile_node(element, context, program)?;
        let element_instructions = std::mem::take(&mut element_result.instructions);
        extend_with_offset(&mut instructions, element_instructions);

        let mut element_kind = element_result.kind;
        if element_kind == ValueKind::Any {
            if let Node::Symbol { value } = element {
                if let Some(var_kind) = context.get_variable_type(value) {
                    element_kind = var_kind;
                } else if let Some(param_kind) = context.get_parameter_type(value) {
                    element_kind = param_kind;
                }
            }
        }

        ensure_owned_on_stack(&mut instructions, element_kind, &mut element_result.heap_ownership);
        if element_kind != ValueKind::Any {
            if let Some(existing) = element_kind_accumulator {
                if existing != element_kind {
                    element_kind_accumulator = Some(ValueKind::Any);
                }
            } else {
                element_kind_accumulator = Some(element_kind);
            }
        }
        let element_dependents = element_result.take_retained_slots();
        instructions.push(IRInstruction::StoreLocal(value_slot));
        track_heap_slot(&mut retained_slots, value_slot, element_kind, None, element_dependents);

        instructions.push(IRInstruction::Push(element_kind.runtime_tag()));
        instructions.push(IRInstruction::StoreLocal(tag_slot));
        element_result.free_retained_slots(&mut instructions, context);
    }

    let values_base = ordered_value_slots[0];
    let tags_base = ordered_tag_slots[0];
    instructions.push(IRInstruction::PushLocalAddress(values_base));
    instructions.push(IRInstruction::PushLocalAddress(tags_base));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_vector_create".to_string(), 3));

    dedup_retained_slots(&mut retained_slots);

    value_slots.into_iter().filter(|slot| !retains_slot(&retained_slots, *slot)).for_each(|slot| {
        context.release_temp_slot(slot);
    });
    tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Vector)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_vector_element_kind(element_kind_accumulator)
        .with_retained_slots(retained_slots))
}

pub(crate) fn emit_vector_get(instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, tracker: &mut SlotTracker, owned_arg_slot: Option<usize>, default: &mut DefaultHandling) {
    owned_arg_slot.into_iter().for_each(|slot| tracker.untrack(slot));

    let out_slot = context.allocate_temp_slot();
    instructions.push(IRInstruction::Push(0));
    instructions.push(IRInstruction::StoreLocal(out_slot));
    instructions.push(IRInstruction::PushLocalAddress(out_slot));
    instructions.push(IRInstruction::RuntimeCall("_vector_get".to_string(), 3));

    let failure_jump_pos = instructions.len();
    instructions.push(IRInstruction::JumpIfZero(0));

    instructions.push(IRInstruction::LoadLocal(out_slot));
    default.success_cleanup(instructions, context);
    let success_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let failure_block_pos = instructions.len();
    instructions[failure_jump_pos] = IRInstruction::JumpIfZero(failure_block_pos);

    default.emit_fallback(instructions);

    let end_pos = instructions.len();
    instructions[success_jump_pos] = IRInstruction::Jump(end_pos);

    context.release_temp_slot(out_slot);
}
