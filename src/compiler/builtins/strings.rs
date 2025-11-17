use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, is_heap_allocated_symbol, CompileContext, CompileError, CompileResult, HeapOwnership, ValueKind};
use crate::ir::{IRInstruction, IRProgram};

fn push_node_as_string(arg: &Node, context: &mut CompileContext, program: &mut IRProgram, instructions: &mut Vec<IRInstruction>) -> Result<bool, CompileError> {
    let mut arg_result = compile_node(arg, context, program)?;
    let arg_instructions = std::mem::take(&mut arg_result.instructions);
    extend_with_offset(instructions, arg_instructions);

    let mut needs_free = arg_result.heap_ownership == HeapOwnership::Owned;
    let mut arg_kind = arg_result.kind;

    if arg_kind == ValueKind::Any {
        if let Node::Symbol { value } = arg {
            if let Some(var_kind) = context.get_variable_type(value) {
                arg_kind = var_kind;
            } else if let Some(param_kind) = context.get_parameter_type(value) {
                arg_kind = param_kind;
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
                needs_free = true;
            }
        }
        ValueKind::Keyword | ValueKind::Nil => {
            instructions.push(IRInstruction::Push(0));
            instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
            needs_free = false;
        }
        ValueKind::Vector => {
            instructions.push(IRInstruction::RuntimeCall("_vector_to_string".to_string(), 1));
            needs_free = true;
        }
        ValueKind::Map => {
            instructions.push(IRInstruction::RuntimeCall("_map_to_string".to_string(), 1));
            needs_free = true;
        }
        ValueKind::Set => {
            instructions.push(IRInstruction::RuntimeCall("_set_to_string".to_string(), 1));
            needs_free = true;
        }
        ValueKind::Boolean => {
            instructions.push(IRInstruction::RuntimeCall("_string_from_boolean".to_string(), 1));
            needs_free = false;
        }
        ValueKind::Number | ValueKind::Any => {
            instructions.push(IRInstruction::RuntimeCall("_string_from_number".to_string(), 1));
            needs_free = true;
        }
    }

    arg_result.free_retained_slots(instructions, context);
    Ok(needs_free)
}

fn lower_nodes_to_string_slots(
    args: &[Node],
    context: &mut CompileContext,
    program: &mut IRProgram,
    instructions: &mut Vec<IRInstruction>,
) -> Result<(Vec<usize>, Vec<bool>, Vec<usize>), CompileError> {
    if args.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }

    let temp_slots = context.allocate_contiguous_temp_slots(args.len());
    let mut ordered_slots = temp_slots.clone();
    ordered_slots.sort_unstable();
    ordered_slots.reverse();

    let mut needs_free = Vec::with_capacity(args.len());

    for (arg, slot) in args.iter().zip(ordered_slots.iter()) {
        let slot_needs_free = push_node_as_string(arg, context, program, instructions)?;
        instructions.push(IRInstruction::StoreLocal(*slot));
        needs_free.push(slot_needs_free);
    }

    Ok((ordered_slots, needs_free, temp_slots))
}

/// Compile str operation (string concatenation)
pub(crate) fn compile_str(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![IRInstruction::Push(0), IRInstruction::Push(0), IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2)],
            ValueKind::String,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let mut instructions = Vec::new();
    let (ordered_slots, needs_free, temp_slots) = lower_nodes_to_string_slots(args, context, program, &mut instructions)?;

    let base_slot = ordered_slots[0];
    instructions.push(IRInstruction::PushLocalAddress(base_slot));
    instructions.push(IRInstruction::Push(args.len() as i64));
    instructions.push(IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2));

    for (slot, free) in ordered_slots.iter().zip(needs_free.iter()) {
        if *free {
            instructions.push(IRInstruction::FreeLocal(*slot));
        }
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}

fn compile_print_like(args: &[Node], newline: bool, context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    let mut instructions = Vec::new();
    let (ordered_slots, needs_free, temp_slots) = lower_nodes_to_string_slots(args, context, program, &mut instructions)?;

    if ordered_slots.is_empty() {
        instructions.push(IRInstruction::Push(0));
    } else {
        instructions.push(IRInstruction::PushLocalAddress(ordered_slots[0]));
    }
    instructions.push(IRInstruction::Push(args.len() as i64));
    instructions.push(IRInstruction::Push(if newline { 1 } else { 0 }));
    instructions.push(IRInstruction::RuntimeCall("_print_values".to_string(), 3));

    for (slot, free_flag) in ordered_slots.iter().zip(needs_free.iter()) {
        if *free_flag {
            instructions.push(IRInstruction::FreeLocal(*slot));
        }
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Nil))
}

pub(crate) fn compile_print(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    compile_print_like(args, false, context, program)
}

pub(crate) fn compile_println(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    compile_print_like(args, true, context, program)
}

pub(crate) fn compile_printf(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Err(CompileError::ArityError("printf".to_string(), 1, 0));
    }

    let mut instructions = Vec::new();
    let format_slot = context.allocate_temp_slot();
    let format_needs_free = push_node_as_string(&args[0], context, program, &mut instructions)?;
    instructions.push(IRInstruction::StoreLocal(format_slot));

    let trailing_args = &args[1..];
    let (ordered_slots, needs_free, temp_slots) = lower_nodes_to_string_slots(trailing_args, context, program, &mut instructions)?;

    instructions.push(IRInstruction::LoadLocal(format_slot));
    if ordered_slots.is_empty() {
        instructions.push(IRInstruction::Push(0));
    } else {
        instructions.push(IRInstruction::PushLocalAddress(ordered_slots[0]));
    }
    instructions.push(IRInstruction::Push(trailing_args.len() as i64));
    instructions.push(IRInstruction::RuntimeCall("_printf_values".to_string(), 3));

    for (slot, free_flag) in ordered_slots.iter().zip(needs_free.iter()) {
        if *free_flag {
            instructions.push(IRInstruction::FreeLocal(*slot));
        }
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    if format_needs_free {
        instructions.push(IRInstruction::FreeLocal(format_slot));
    }
    context.release_temp_slot(format_slot);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Nil))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Primitive;

    fn make_context_and_program() -> (CompileContext, IRProgram) {
        (CompileContext::new(), IRProgram::new())
    }

    #[test]
    fn push_node_as_string_converts_numbers() {
        let (mut context, mut program) = make_context_and_program();
        let node = Node::Primitive { value: Primitive::Number(1) };
        let mut instructions = Vec::new();

        let needs_free = push_node_as_string(&node, &mut context, &mut program, &mut instructions).unwrap();

        assert!(needs_free, "numeric coercions should return owned strings");
        assert!(
            instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 1) if name == "_string_from_number")),
            "expected runtime normalization for numeric input"
        );
    }

    #[test]
    fn push_node_as_string_normalizes_keywords_without_allocations() {
        let (mut context, mut program) = make_context_and_program();
        let node = Node::Primitive {
            value: Primitive::Keyword(":kw".to_string()),
        };
        let mut instructions = Vec::new();

        let needs_free = push_node_as_string(&node, &mut context, &mut program, &mut instructions).unwrap();

        assert!(!needs_free, "keyword rendering reuses the inline literal");
        assert!(
            instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 2) if name == "_string_normalize")),
            "expected keyword path to call normalize via runtime"
        );
    }
}
