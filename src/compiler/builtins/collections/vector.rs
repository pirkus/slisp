use super::super::ownership::{dedup_retained_slots, ensure_owned_on_stack, track_heap_slot};
use super::common::{allocate_descending_slots, release_all, release_slots, resolve_value_kind};
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, CompileContext, CompileError, CompileResult, HeapOwnership, RetainedSlot, ValueKind};
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
    let (ordered_value_slots, value_slots) = allocate_descending_slots(context, count);
    let (ordered_tag_slots, tag_slots) = allocate_descending_slots(context, count);

    let mut retained_slots: Vec<RetainedSlot> = Vec::new();

    elements
        .iter()
        .zip(ordered_value_slots.iter().copied())
        .zip(ordered_tag_slots.iter().copied())
        .try_for_each(|((element, value_slot), tag_slot)| -> Result<(), CompileError> {
            let mut element_result = compile_node(element, context, program)?;
            let element_instructions = std::mem::take(&mut element_result.instructions);
            extend_with_offset(&mut instructions, element_instructions);

            let element_kind = resolve_value_kind(element, element_result.kind, context);
            ensure_owned_on_stack(&mut instructions, element_kind, &mut element_result.heap_ownership);
            let element_dependents = element_result.take_retained_slots();
            instructions.push(IRInstruction::StoreLocal(value_slot));
            track_heap_slot(&mut retained_slots, value_slot, element_kind, None, element_dependents);

            instructions.push(IRInstruction::Push(element_kind.runtime_tag()));
            instructions.push(IRInstruction::StoreLocal(tag_slot));
            element_result.free_retained_slots(&mut instructions, context);
            Ok(())
        })?;

    let values_base = ordered_value_slots[0];
    let tags_base = ordered_tag_slots[0];
    instructions.push(IRInstruction::PushLocalAddress(values_base));
    instructions.push(IRInstruction::PushLocalAddress(tags_base));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_vector_create".to_string(), 3));

    dedup_retained_slots(&mut retained_slots);

    release_slots(value_slots, &retained_slots, context);
    release_all(tag_slots, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Vector)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}
