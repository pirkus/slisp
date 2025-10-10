/// Expression compilation - arithmetic, comparisons, conditionals, logical operations

use crate::domain::{Node, Primitive};
use crate::ir::{IRInstruction, IRProgram};
use super::{CompileContext, CompileError};

/// Compile a primitive value (numbers, strings)
pub fn compile_primitive(primitive: &Primitive, program: &mut IRProgram) -> Result<(), CompileError> {
    match primitive {
        Primitive::Number(n) => {
            program.add_instruction(IRInstruction::Push(*n as i64));
            Ok(())
        }
        Primitive::_Str(_) => Err(CompileError::UnsupportedOperation(
            "String literals not supported".to_string(),
        )),
    }
}

/// Compile arithmetic operations (+, -, *, /)
pub fn compile_arithmetic_op(
    args: &[Node],
    program: &mut IRProgram,
    context: &mut CompileContext,
    instruction: IRInstruction,
    op_name: &str,
) -> Result<(), CompileError> {
    if args.len() < 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    // Compile first operand
    crate::compiler::compile_node(&args[0], program, context)?;

    // Compile and apply remaining operands
    for arg in &args[1..] {
        crate::compiler::compile_node(arg, program, context)?;
        program.add_instruction(instruction.clone());
    }

    Ok(())
}

/// Compile comparison operations (=, <, >, <=, >=)
pub fn compile_comparison_op(
    args: &[Node],
    program: &mut IRProgram,
    context: &mut CompileContext,
    instruction: IRInstruction,
    op_name: &str,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    crate::compiler::compile_node(&args[0], program, context)?;
    crate::compiler::compile_node(&args[1], program, context)?;
    program.add_instruction(instruction);

    Ok(())
}

/// Compile if expression
pub fn compile_if(
    args: &[Node],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("if".to_string(), 3, args.len()));
    }

    // Compile condition
    crate::compiler::compile_node(&args[0], program, context)?;

    // Jump to else clause if condition is false (0)
    let else_jump_pos = program.len();
    program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched

    // Compile then clause
    crate::compiler::compile_node(&args[1], program, context)?;

    // Jump over else clause
    let end_jump_pos = program.len();
    program.add_instruction(IRInstruction::Jump(0)); // Will be patched

    // Patch else jump target
    let else_start = program.len();
    if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[else_jump_pos] {
        *target = else_start;
    }

    // Compile else clause
    crate::compiler::compile_node(&args[2], program, context)?;

    // Patch end jump target
    let end_pos = program.len();
    if let IRInstruction::Jump(ref mut target) = &mut program.instructions[end_jump_pos] {
        *target = end_pos;
    }

    Ok(())
}

/// Compile logical AND operation with short-circuit evaluation
pub fn compile_logical_and(
    args: &[Node],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(1)); // true
        return Ok(());
    }

    if args.len() == 1 {
        crate::compiler::compile_node(&args[0], program, context)?;
        // Convert to boolean (0 or 1)
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::Not);
        return Ok(());
    }

    // Compile first argument
    super::compile_node(&args[0], program, context)?;

    // For each additional argument, short-circuit if current result is false
    let mut false_jumps = Vec::new();

    for arg in &args[1..] {
        // Test if current value is false (0) - if so, short-circuit to false
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);

        // If current value is false (Equal result will be 1), we need to NOT jump
        // So we invert the test: jump if Equal result is 0 (meaning value was NOT 0)
        program.add_instruction(IRInstruction::Not);
        let false_jump = program.len();
        program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched
        false_jumps.push(false_jump);

        // Current value is true, so evaluate next argument
        crate::compiler::compile_node(arg, program, context)?;
    }

    // Convert final result to boolean and jump to end
    program.add_instruction(IRInstruction::Push(0));
    program.add_instruction(IRInstruction::Equal);
    program.add_instruction(IRInstruction::Not);

    let end_jump = program.len();
    program.add_instruction(IRInstruction::Jump(0)); // Will be patched

    // False result path: push 0
    let false_label = program.len();
    program.add_instruction(IRInstruction::Push(0));

    // Patch all jumps
    let end_label = program.len();

    // Patch false jumps to false result
    for jump_pos in false_jumps {
        if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[jump_pos] {
            *target = false_label;
        }
    }

    // Patch end jump
    if let IRInstruction::Jump(ref mut target) = &mut program.instructions[end_jump] {
        *target = end_label;
    }

    Ok(())
}

/// Compile logical OR operation with short-circuit evaluation
pub fn compile_logical_or(
    args: &[Node],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(0)); // false
        return Ok(());
    }

    if args.len() == 1 {
        crate::compiler::compile_node(&args[0], program, context)?;
        // Convert to boolean (0 or 1)
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::Not);
        return Ok(());
    }

    // Compile first argument
    super::compile_node(&args[0], program, context)?;

    // For each additional argument, short-circuit if current result is true
    let mut true_jumps = Vec::new();

    for arg in &args[1..] {
        // Test if current value is true (non-zero) - if so, short-circuit to true
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);

        // If current value is true (Equal result will be 0), jump to true result
        let true_jump = program.len();
        program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched
        true_jumps.push(true_jump);

        // Current value is false, so evaluate next argument
        crate::compiler::compile_node(arg, program, context)?;
    }

    // Convert final result to boolean and jump to end
    program.add_instruction(IRInstruction::Push(0));
    program.add_instruction(IRInstruction::Equal);
    program.add_instruction(IRInstruction::Not);

    let end_jump = program.len();
    program.add_instruction(IRInstruction::Jump(0)); // Will be patched

    // True result path: push 1
    let true_label = program.len();
    program.add_instruction(IRInstruction::Push(1));

    // Patch all jumps
    let end_label = program.len();

    // Patch true jumps to true result
    for jump_pos in true_jumps {
        if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[jump_pos] {
            *target = true_label;
        }
    }

    // Patch end jump
    if let IRInstruction::Jump(ref mut target) = &mut program.instructions[end_jump] {
        *target = end_label;
    }

    Ok(())
}

/// Compile logical NOT operation
pub fn compile_logical_not(
    args: &[Node],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("not".to_string(), 1, args.len()));
    }

    super::compile_node(&args[0], program, context)?;
    program.add_instruction(IRInstruction::Not);

    Ok(())
}
