use super::{CompileContext, CompileError, CompileResult, HeapOwnership, ValueKind};
/// Expression compilation - arithmetic, comparisons, conditionals, logical operations
use crate::ast::{Node, Primitive};
use crate::compiler::liveness::{apply_liveness_plan, compute_liveness_plan};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::HashSet;

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

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?.instructions;

    for arg in &args[1..] {
        instructions.extend(crate::compiler::compile_node(arg, context, program)?.instructions);
        instructions.push(instruction.clone());
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Number))
}

/// Compile comparison operations (=, <, >, <=, >=)
pub fn compile_comparison_op(args: &[Node], context: &mut CompileContext, program: &mut IRProgram, instruction: IRInstruction, op_name: &str) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut temp_slots = Vec::new();

    let left_result = crate::compiler::compile_node(&args[0], context, program)?;
    let CompileResult {
        mut instructions,
        kind: mut left_kind,
        heap_ownership: left_ownership,
    } = left_result;

    if left_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        temp_slots.push(slot);
    }

    let right_result = crate::compiler::compile_node(&args[1], context, program)?;
    let CompileResult {
        instructions: right_instructions,
        kind: mut right_kind,
        heap_ownership: right_ownership,
    } = right_result;

    instructions.extend(right_instructions);

    if right_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        temp_slots.push(slot);
    }

    if left_kind == ValueKind::Any {
        left_kind = resolve_operand_kind(&args[0], left_kind, context);
    }
    if right_kind == ValueKind::Any {
        right_kind = resolve_operand_kind(&args[1], right_kind, context);
    }

    // Determine if we need a runtime call for complex equality checks
    let needs_runtime_equality = matches!(instruction, IRInstruction::Equal)
        && match (left_kind, right_kind) {
            (ValueKind::String, ValueKind::String) => true,
            (ValueKind::Keyword, ValueKind::Keyword) => true,
            (ValueKind::Vector, ValueKind::Vector) => true,
            (ValueKind::Set, ValueKind::Set) => true,
            (ValueKind::Map, ValueKind::Map) => true,
            _ => false,
        };

    if needs_runtime_equality {
        let runtime_fn = match (left_kind, right_kind) {
            (ValueKind::String, ValueKind::String) | (ValueKind::Keyword, ValueKind::Keyword) => "_string_equals",
            (ValueKind::Vector, ValueKind::Vector) => "_vector_equals",
            (ValueKind::Set, ValueKind::Set) => "_set_equals",
            (ValueKind::Map, ValueKind::Map) => "_map_equals",
            _ => unreachable!("Runtime equality check with unsupported types"),
        };
        instructions.push(IRInstruction::RuntimeCall(runtime_fn.to_string(), 2));
    } else {
        instructions.push(instruction);
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_liveness_plan(instructions, &plan);
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

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

    let then_result = crate::compiler::compile_node(&args[1], context, program)?;
    let then_instructions = then_result.instructions;
    instructions.extend(then_instructions);

    let end_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let else_start = instructions.len();
    instructions[else_jump_pos] = IRInstruction::JumpIfZero(else_start);

    let else_result = crate::compiler::compile_node(&args[2], context, program)?;
    let else_instructions = else_result.instructions;
    instructions.extend(else_instructions);

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

    Ok(CompileResult::with_instructions(instructions, resulting_kind).with_heap_ownership(ownership))
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

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?.instructions;

    if args.len() == 1 {
        instructions.extend([IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);
        return Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean));
    }

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

        instructions.extend(crate::compiler::compile_node(arg, context, program)?.instructions);
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

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?.instructions;
    instructions.push(IRInstruction::Not);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}
