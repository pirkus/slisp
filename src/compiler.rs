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

    if args.len() == 1 {
        compile_node(&args[0], program)?;
        // Convert to boolean (0 or 1)
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::Not);
        return Ok(());
    }

    // Compile first argument
    compile_node(&args[0], program)?;

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
        compile_node(arg, program)?;
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

fn compile_logical_or(args: &[Box<Node>], program: &mut IRProgram) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(0)); // false
        return Ok(());
    }

    if args.len() == 1 {
        compile_node(&args[0], program)?;
        // Convert to boolean (0 or 1)
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::Not);
        return Ok(());
    }

    // Compile first argument
    compile_node(&args[0], program)?;

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
        compile_node(arg, program)?;
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

    #[test]
    fn test_compile_if_true() {
        let program = compile_expression("(if (> 5 3) 42 0)").unwrap();
        // Should generate: push 5, push 3, greater, jumpifzero else, push 42, jump end, push 0, return
        println!("If IR: {:?}", program.instructions);
        assert!(program.instructions.len() > 5); // Should have multiple instructions
    }

    #[test]
    fn test_compile_not() {
        let program = compile_expression("(not 0)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(0),
                IRInstruction::Not,
                IRInstruction::Return
            ]
        );
    }

    #[test]
    fn test_compile_and() {
        let program = compile_expression("(and 1 1)").unwrap();
        println!("AND IR: {:?}", program.instructions);
        assert!(program.instructions.len() > 3); // Should have multiple instructions
    }

    #[test]
    fn test_compile_and_false() {
        let program = compile_expression("(and 1 0)").unwrap();
        println!("AND FALSE IR: {:?}", program.instructions);
        assert!(program.instructions.len() > 3); // Should have multiple instructions
    }
}
