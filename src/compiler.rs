use crate::domain::{Node, Primitive};
use crate::ir::{IRInstruction, IRProgram};

#[derive(Debug, PartialEq)]
pub enum CompileError {
    UnsupportedOperation(String),
    InvalidExpression(String),
    ArityError(String, usize, usize),
}

pub fn compile_to_ir(node: &Node) -> Result<IRProgram, CompileError> {
    let mut program = IRProgram::new();
    compile_node(node, &mut program)?;
    program.add_instruction(IRInstruction::Return);
    Ok(program)
}

fn compile_node(node: &Node, program: &mut IRProgram) -> Result<(), CompileError> {
    match node {
        Node::Primitive { value } => compile_primitive(value, program),
        Node::Symbol { value } => Err(CompileError::UnsupportedOperation(format!(
            "Free variables not supported: {}",
            value
        ))),
        Node::List { root } => compile_list(root, program),
    }
}

fn compile_primitive(primitive: &Primitive, program: &mut IRProgram) -> Result<(), CompileError> {
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

fn compile_list(nodes: &[Box<Node>], program: &mut IRProgram) -> Result<(), CompileError> {
    if nodes.is_empty() {
        program.add_instruction(IRInstruction::Push(0)); // nil = 0
        return Ok(());
    }

    let operator = &nodes[0];
    let args = &nodes[1..];

    match operator.as_ref() {
        Node::Symbol { value } => match value.as_str() {
            "+" => compile_arithmetic_op(args, program, IRInstruction::Add, "+"),
            "-" => compile_arithmetic_op(args, program, IRInstruction::Sub, "-"),
            "*" => compile_arithmetic_op(args, program, IRInstruction::Mul, "*"),
            "/" => compile_arithmetic_op(args, program, IRInstruction::Div, "/"),
            "=" => compile_comparison_op(args, program, IRInstruction::Equal, "="),
            "<" => compile_comparison_op(args, program, IRInstruction::Less, "<"),
            ">" => compile_comparison_op(args, program, IRInstruction::Greater, ">"),
            "<=" => compile_comparison_op(args, program, IRInstruction::LessEqual, "<="),
            ">=" => compile_comparison_op(args, program, IRInstruction::GreaterEqual, ">="),
            "if" => compile_if(args, program),
            "and" => compile_logical_and(args, program),
            "or" => compile_logical_or(args, program),
            "not" => compile_logical_not(args, program),
            op => Err(CompileError::UnsupportedOperation(op.to_string())),
        },
        _ => Err(CompileError::InvalidExpression(
            "First element must be a symbol".to_string(),
        )),
    }
}

fn compile_arithmetic_op(
    args: &[Box<Node>],
    program: &mut IRProgram,
    instruction: IRInstruction,
    op_name: &str,
) -> Result<(), CompileError> {
    if args.len() < 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    // Compile first operand
    compile_node(&args[0], program)?;

    // Compile and apply remaining operands
    for arg in &args[1..] {
        compile_node(arg, program)?;
        program.add_instruction(instruction.clone());
    }

    Ok(())
}

fn compile_comparison_op(
    args: &[Box<Node>],
    program: &mut IRProgram,
    instruction: IRInstruction,
    op_name: &str,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    compile_node(&args[0], program)?;
    compile_node(&args[1], program)?;
    program.add_instruction(instruction);

    Ok(())
}

fn compile_if(args: &[Box<Node>], program: &mut IRProgram) -> Result<(), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("if".to_string(), 3, args.len()));
    }

    // Compile condition
    compile_node(&args[0], program)?;

    // Jump to else clause if condition is false (0)
    let else_jump_pos = program.len();
    program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched

    // Compile then clause
    compile_node(&args[1], program)?;

    // Jump over else clause
    let end_jump_pos = program.len();
    program.add_instruction(IRInstruction::Jump(0)); // Will be patched

    // Patch else jump target
    let else_start = program.len();
    if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[else_jump_pos] {
        *target = else_start;
    }

    // Compile else clause
    compile_node(&args[2], program)?;

    // Patch end jump target
    let end_pos = program.len();
    if let IRInstruction::Jump(ref mut target) = &mut program.instructions[end_jump_pos] {
        *target = end_pos;
    }

    Ok(())
}

fn compile_logical_and(args: &[Box<Node>], program: &mut IRProgram) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(1)); // true
        return Ok(());
    }

    let mut end_jumps = Vec::new();

    for (i, arg) in args.iter().enumerate() {
        compile_node(arg, program)?;

        // For all but the last argument, jump to end if false
        if i < args.len() - 1 {
            // Duplicate value for testing
            program.add_instruction(IRInstruction::Push(0));
            program.add_instruction(IRInstruction::Equal);

            let jump_pos = program.len();
            program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched
            end_jumps.push(jump_pos);

            // Pop the original value since we're continuing
            program.add_instruction(IRInstruction::Pop);
        }
    }

    // Patch all end jumps
    let end_pos = program.len();
    for jump_pos in end_jumps {
        if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[jump_pos] {
            *target = end_pos;
        }
    }

    // Convert final result to boolean (0 or 1)
    program.add_instruction(IRInstruction::Push(0));
    program.add_instruction(IRInstruction::Equal);
    program.add_instruction(IRInstruction::Not);

    Ok(())
}

fn compile_logical_or(args: &[Box<Node>], program: &mut IRProgram) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(0)); // false
        return Ok(());
    }

    let mut end_jumps = Vec::new();

    for (i, arg) in args.iter().enumerate() {
        compile_node(arg, program)?;

        // For all but the last argument, jump to end if true
        if i < args.len() - 1 {
            // Duplicate value for testing
            program.add_instruction(IRInstruction::Push(0));
            program.add_instruction(IRInstruction::Equal);
            program.add_instruction(IRInstruction::Not);

            let jump_pos = program.len();
            program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched
            end_jumps.push(jump_pos);

            // Pop the original value since we're continuing
            program.add_instruction(IRInstruction::Pop);
        }
    }

    // Patch all end jumps
    let end_pos = program.len();
    for jump_pos in end_jumps {
        if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[jump_pos] {
            *target = end_pos;
        }
    }

    // Convert final result to boolean (0 or 1)
    program.add_instruction(IRInstruction::Push(0));
    program.add_instruction(IRInstruction::Equal);
    program.add_instruction(IRInstruction::Not);

    Ok(())
}

fn compile_logical_not(args: &[Box<Node>], program: &mut IRProgram) -> Result<(), CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("not".to_string(), 1, args.len()));
    }

    compile_node(&args[0], program)?;
    program.add_instruction(IRInstruction::Not);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_parser::{AstParser, AstParserTrt};

    fn compile_expression(input: &str) -> Result<IRProgram, CompileError> {
        let ast = AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0);
        compile_to_ir(&ast)
    }

    #[test]
    fn test_compile_number() {
        let program = compile_expression("42").unwrap();
        assert_eq!(
            program.instructions,
            vec![IRInstruction::Push(42), IRInstruction::Return]
        );
    }

    #[test]
    fn test_compile_arithmetic() {
        let program = compile_expression("(+ 2 3)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(2),
                IRInstruction::Push(3),
                IRInstruction::Add,
                IRInstruction::Return
            ]
        );
    }

    #[test]
    fn test_compile_nested() {
        let program = compile_expression("(+ 2 (* 3 4))").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(2),
                IRInstruction::Push(3),
                IRInstruction::Push(4),
                IRInstruction::Mul,
                IRInstruction::Add,
                IRInstruction::Return
            ]
        );
    }
}
