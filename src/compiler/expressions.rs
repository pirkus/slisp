use super::{CompileContext, CompileError};
/// Expression compilation - arithmetic, comparisons, conditionals, logical operations
use crate::ast::{Node, Primitive};
use crate::ir::{IRInstruction, IRProgram};

/// Compile a primitive value (numbers, strings)
pub fn compile_primitive(primitive: &Primitive, program: &mut IRProgram) -> Result<Vec<IRInstruction>, CompileError> {
    match primitive {
        Primitive::Number(n) => Ok(vec![IRInstruction::Push(*n as i64)]),
        Primitive::String(s) => {
            let string_index = program.add_string(s.clone());
            Ok(vec![IRInstruction::PushString(string_index)])
        }
    }
}

/// Compile arithmetic operations (+, -, *, /)
pub fn compile_arithmetic_op(args: &[Node], context: &mut CompileContext, program: &mut IRProgram, instruction: IRInstruction, op_name: &str) -> Result<Vec<IRInstruction>, CompileError> {
    if args.len() < 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;

    for arg in &args[1..] {
        instructions.extend(crate::compiler::compile_node(arg, context, program)?);
        instructions.push(instruction.clone());
    }

    Ok(instructions)
}

/// Compile comparison operations (=, <, >, <=, >=)
pub fn compile_comparison_op(args: &[Node], context: &mut CompileContext, program: &mut IRProgram, instruction: IRInstruction, op_name: &str) -> Result<Vec<IRInstruction>, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;
    instructions.extend(crate::compiler::compile_node(&args[1], context, program)?);
    instructions.push(instruction);

    Ok(instructions)
}

/// Compile if expression
pub fn compile_if(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<Vec<IRInstruction>, CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("if".to_string(), 3, args.len()));
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;

    let else_jump_pos = instructions.len();
    instructions.push(IRInstruction::JumpIfZero(0));

    let then_instructions = crate::compiler::compile_node(&args[1], context, program)?;
    instructions.extend(then_instructions);

    let end_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let else_start = instructions.len();
    instructions[else_jump_pos] = IRInstruction::JumpIfZero(else_start);

    let else_instructions = crate::compiler::compile_node(&args[2], context, program)?;
    instructions.extend(else_instructions);

    let end_pos = instructions.len();
    instructions[end_jump_pos] = IRInstruction::Jump(end_pos);

    Ok(instructions)
}

/// Compile logical AND operation with short-circuit evaluation
pub fn compile_logical_and(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<Vec<IRInstruction>, CompileError> {
    if args.is_empty() {
        return Ok(vec![IRInstruction::Push(1)]);
    }

    if args.len() == 1 {
        let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;
        instructions.extend(vec![IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);
        return Ok(instructions);
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;

    let mut false_jumps = Vec::new();

    for arg in &args[1..] {
        instructions.extend(vec![IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);
        let false_jump = instructions.len();
        instructions.push(IRInstruction::JumpIfZero(0));
        false_jumps.push(false_jump);

        instructions.extend(crate::compiler::compile_node(arg, context, program)?);
    }

    instructions.extend(vec![IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);

    let end_jump = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let false_label = instructions.len();
    instructions.push(IRInstruction::Push(0));

    let end_label = instructions.len();

    for jump_pos in false_jumps {
        instructions[jump_pos] = IRInstruction::JumpIfZero(false_label);
    }

    instructions[end_jump] = IRInstruction::Jump(end_label);

    Ok(instructions)
}

/// Compile logical OR operation with short-circuit evaluation
pub fn compile_logical_or(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<Vec<IRInstruction>, CompileError> {
    if args.is_empty() {
        return Ok(vec![IRInstruction::Push(0)]);
    }

    if args.len() == 1 {
        let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;
        instructions.extend(vec![IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);
        return Ok(instructions);
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;

    let mut true_jumps = Vec::new();

    for arg in &args[1..] {
        instructions.extend(vec![IRInstruction::Push(0), IRInstruction::Equal]);
        let true_jump = instructions.len();
        instructions.push(IRInstruction::JumpIfZero(0));
        true_jumps.push(true_jump);

        instructions.extend(crate::compiler::compile_node(arg, context, program)?);
    }

    instructions.extend(vec![IRInstruction::Push(0), IRInstruction::Equal, IRInstruction::Not]);

    let end_jump = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let true_label = instructions.len();
    instructions.push(IRInstruction::Push(1));

    let end_label = instructions.len();

    for jump_pos in true_jumps {
        instructions[jump_pos] = IRInstruction::JumpIfZero(true_label);
    }

    instructions[end_jump] = IRInstruction::Jump(end_label);

    Ok(instructions)
}

/// Compile logical NOT operation
pub fn compile_logical_not(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<Vec<IRInstruction>, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("not".to_string(), 1, args.len()));
    }

    let mut instructions = crate::compiler::compile_node(&args[0], context, program)?;
    instructions.push(IRInstruction::Not);

    Ok(instructions)
}
