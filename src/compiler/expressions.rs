use super::{
    extend_with_offset,
    slots::SlotTracker,
    CompileContext,
    CompileError,
    CompileResult,
    HeapOwnership,
    RetainedSlot,
    ValueKind,
};
/// Expression compilation - arithmetic, comparisons, conditionals, logical operations
use crate::ast::{Node, Primitive};
use crate::compiler::is_heap_allocated_symbol;
use crate::ir::{IRInstruction, IRProgram};

/// Compile a primitive value (numbers, strings)
pub fn compile_primitive(primitive: &Primitive, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    match primitive {
        Primitive::Number(n) => Ok(CompileResult::with_instructions(vec![IRInstruction::Push(*n as i64)], ValueKind::Number)),
        Primitive::Boolean(b) => Ok(CompileResult::with_instructions(vec![IRInstruction::Push(if *b { 1 } else { 0 })], ValueKind::Boolean)),
        Primitive::String(s) => {
            let string_index = program.add_string(s.clone());
            Ok(CompileResult::with_instructions(vec![IRInstruction::PushString(string_index)], ValueKind::String))
        }
        Primitive::Keyword(k) => {
            let literal = format!(":{}", k);
            let string_index = program.add_string(literal);
            Ok(CompileResult::with_instructions(vec![IRInstruction::PushString(string_index)], ValueKind::Keyword))
        }
    }
}

/// Compile arithmetic operations (+, -, *, /)
pub fn compile_arithmetic_op(args: &[Node], context: &mut CompileContext, program: &mut IRProgram, instruction: IRInstruction, op_name: &str) -> Result<CompileResult, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let mut left_result = crate::compiler::compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut left_result.instructions);
    left_result.free_retained_slots(&mut instructions, context);

    for arg in &args[1..] {
        let mut compiled = crate::compiler::compile_node(arg, context, program)?;
        let compiled_instructions = std::mem::take(&mut compiled.instructions);
        extend_with_offset(&mut instructions, compiled_instructions);
        compiled.free_retained_slots(&mut instructions, context);
        instructions.push(instruction.clone());
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Number))
}

/// Compile comparison operations (=, <, >, <=, >=)
pub fn compile_comparison_op(args: &[Node], context: &mut CompileContext, program: &mut IRProgram, instruction: IRInstruction, op_name: &str) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let mut tracker = SlotTracker::new();

    let mut left_result = crate::compiler::compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut left_result.instructions);
    let mut left_kind = left_result.kind;

    let left_slot = tracker.track_if_owned(&mut instructions, context, left_result.heap_ownership, ValueKind::Any);

    let mut right_result = crate::compiler::compile_node(&args[1], context, program)?;
    let mut right_kind = right_result.kind;

    let right_instructions = std::mem::take(&mut right_result.instructions);
    extend_with_offset(&mut instructions, right_instructions);

    let right_slot = tracker.track_if_owned(&mut instructions, context, right_result.heap_ownership, ValueKind::Any);

    if left_kind == ValueKind::Any {
        left_kind = resolve_operand_kind(&args[0], left_kind, context);
    }
    if right_kind == ValueKind::Any {
        right_kind = resolve_operand_kind(&args[1], right_kind, context);
    }

    if let Some(slot) = left_slot {
        tracker.set_slot_kind(slot, left_kind);
    }
    if let Some(slot) = right_slot {
        tracker.set_slot_kind(slot, right_kind);
    }

    let string_equality =
        matches!(instruction, IRInstruction::Equal) && ((left_kind == ValueKind::String && right_kind == ValueKind::String) || (left_kind == ValueKind::Keyword && right_kind == ValueKind::Keyword));

    if string_equality {
        instructions.push(IRInstruction::RuntimeCall("_string_equals".to_string(), 2));
    } else {
        instructions.push(instruction);
    }

    left_result.free_retained_slots(&mut instructions, context);
    right_result.free_retained_slots(&mut instructions, context);

    instructions = tracker.apply_liveness_and_release(instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}

fn resolve_operand_kind(node: &Node, fallback: ValueKind, context: &CompileContext) -> ValueKind {
    if fallback != ValueKind::Any {
        return fallback;
    }

    match node {
        Node::Symbol { value } => context.get_variable_type(value).or_else(|| context.get_parameter_type(value)).unwrap_or(fallback),
        _ => fallback,
    }
}

/// Compile if expression
pub fn compile_if(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("if".to_string(), 3, args.len()));
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?.instructions;

    let else_jump_pos = instructions.len();
    instructions.push(IRInstruction::JumpIfZero(0));

    let mut then_result = crate::compiler::compile_node(&args[1], context, program)?;
    let mut then_retained_slots = then_result.take_retained_slots();
    let then_instructions = std::mem::take(&mut then_result.instructions);
    extend_with_offset(&mut instructions, then_instructions);
    ensure_branch_result_owned(&args[1], &mut then_result, &mut instructions, context);

    let end_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let else_start = instructions.len();
    instructions[else_jump_pos] = IRInstruction::JumpIfZero(else_start);

    let mut else_result = crate::compiler::compile_node(&args[2], context, program)?;
    let mut else_retained_slots = else_result.take_retained_slots();
    let else_instructions = std::mem::take(&mut else_result.instructions);
    extend_with_offset(&mut instructions, else_instructions);
    ensure_branch_result_owned(&args[2], &mut else_result, &mut instructions, context);

    let end_pos = instructions.len();
    instructions[end_jump_pos] = IRInstruction::Jump(end_pos);

    let resulting_kind = if then_result.kind == else_result.kind {
        then_result.kind
    } else if (then_result.kind == ValueKind::String && else_result.kind == ValueKind::Nil) || (then_result.kind == ValueKind::Nil && else_result.kind == ValueKind::String) {
        ValueKind::String
    } else if (then_result.kind == ValueKind::Keyword && else_result.kind == ValueKind::Nil) || (then_result.kind == ValueKind::Nil && else_result.kind == ValueKind::Keyword) {
        ValueKind::Keyword
    } else if (then_result.kind == ValueKind::Vector && else_result.kind == ValueKind::Nil) || (then_result.kind == ValueKind::Nil && else_result.kind == ValueKind::Vector) {
        ValueKind::Vector
    } else if (then_result.kind == ValueKind::Map && else_result.kind == ValueKind::Nil) || (then_result.kind == ValueKind::Nil && else_result.kind == ValueKind::Map) {
        ValueKind::Map
    } else if (then_result.kind == ValueKind::Boolean && else_result.kind == ValueKind::Nil) || (then_result.kind == ValueKind::Nil && else_result.kind == ValueKind::Boolean) {
        ValueKind::Boolean
    } else {
        ValueKind::Any
    };

    let ownership = then_result.heap_ownership.combine(else_result.heap_ownership);

    then_retained_slots.extend(else_retained_slots.drain(..));
    dedup_retained_slots(&mut then_retained_slots);

    Ok(CompileResult::with_instructions(instructions, resulting_kind)
        .with_heap_ownership(ownership)
        .with_retained_slots(then_retained_slots))
}

fn ensure_branch_result_owned(branch_node: &Node, branch_result: &mut CompileResult, instructions: &mut Vec<IRInstruction>, context: &CompileContext) {
    if branch_result.heap_ownership == HeapOwnership::Owned {
        return;
    }

    if let Node::Symbol { value } = branch_node {
        if !is_heap_allocated_symbol(value, context) {
            return;
        }

        let inferred_kind = context.get_variable_type(value).or_else(|| context.get_parameter_type(value)).unwrap_or(ValueKind::String);
        let kind = if inferred_kind == ValueKind::Any { ValueKind::String } else { inferred_kind };

        if let Some(runtime) = clone_runtime_for_kind(kind) {
            instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
            branch_result.heap_ownership = HeapOwnership::Owned;
            branch_result.kind = kind;
        }
    }
}

fn clone_runtime_for_kind(kind: ValueKind) -> Option<&'static str> {
    match kind {
        ValueKind::String => Some("_string_clone"),
        ValueKind::Vector => Some("_vector_clone"),
        ValueKind::Map => Some("_map_clone"),
        ValueKind::Set => Some("_set_clone"),
        _ => None,
    }
}

fn compile_variadic_logical(
    args: &[Node],
    context: &mut CompileContext,
    program: &mut IRProgram,
    default_result: i64,
    short_circuit_result: i64,
    short_circuit_on_true: bool,
) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(vec![IRInstruction::Push(default_result)], ValueKind::Boolean));
    }

    let mut first_result = crate::compiler::compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut first_result.instructions);

    if args.len() == 1 {
        first_result.free_retained_slots(&mut instructions, context);
        instructions.extend([IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);
        return Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean));
    }
    first_result.free_retained_slots(&mut instructions, context);

    let mut jump_sites = Vec::new();

    for arg in &args[1..] {
        if short_circuit_on_true {
            instructions.extend([IRInstruction::Push(0), IRInstruction::Equal]);
        } else {
            instructions.extend([IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);
        }

        let jump_site = instructions.len();
        instructions.push(IRInstruction::JumpIfZero(0));
        jump_sites.push(jump_site);

        let mut compiled = crate::compiler::compile_node(arg, context, program)?;
        let compiled_instructions = std::mem::take(&mut compiled.instructions);
        extend_with_offset(&mut instructions, compiled_instructions);
        compiled.free_retained_slots(&mut instructions, context);
    }

    instructions.extend([IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);

    let end_jump = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let short_circuit_label = instructions.len();
    instructions.push(IRInstruction::Push(short_circuit_result));

    let end_label = instructions.len();

    for jump_site in jump_sites {
        instructions[jump_site] = IRInstruction::JumpIfZero(short_circuit_label);
    }

    instructions[end_jump] = IRInstruction::Jump(end_label);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}

fn dedup_retained_slots(slots: &mut Vec<RetainedSlot>) {
    if slots.is_empty() {
        return;
    }
    slots.sort_by_key(|slot| slot.slot);
    slots.dedup_by(|a, b| {
        if a.slot == b.slot {
            a.dependents.extend(b.dependents.drain(..));
            true
        } else {
            false
        }
    });
}

/// Compile logical AND operation with short-circuit evaluation
pub fn compile_logical_and(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    compile_variadic_logical(args, context, program, 1, 0, false)
}

/// Compile logical OR operation with short-circuit evaluation
pub fn compile_logical_or(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    compile_variadic_logical(args, context, program, 0, 1, true)
}

/// Compile logical NOT operation
pub fn compile_logical_not(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("not".to_string(), 1, args.len()));
    }

    let mut operand = crate::compiler::compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut operand.instructions);
    operand.free_retained_slots(&mut instructions, context);
    instructions.push(IRInstruction::Not);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}
