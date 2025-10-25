use super::{CompileContext, CompileError, CompileResult, ValueKind};
/// Expression compilation - arithmetic, comparisons, conditionals, logical operations
use crate::ast::{Node, Primitive};
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

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?.instructions;
    instructions.extend(crate::compiler::compile_node(&args[1], context, program)?.instructions);
    instructions.push(instruction);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
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
    } else if (then_result.kind == ValueKind::Vector && else_result.kind == ValueKind::Nil) || (then_result.kind == ValueKind::Nil && else_result.kind == ValueKind::Vector) {
        ValueKind::Vector
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
