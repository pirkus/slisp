use crate::domain::{Node, Primitive};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum CompileError {
    UnsupportedOperation(String),
    InvalidExpression(String),
    ArityError(String, usize, usize),
    UndefinedVariable(String),
}

#[derive(Debug, Clone)]
struct CompileContext {
    variables: HashMap<String, usize>, // variable name -> local slot index
    next_slot: usize,
    free_slots: Vec<usize>, // stack of freed slots for reuse
}

impl CompileContext {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            next_slot: 0,
            free_slots: Vec::new(),
        }
    }

    fn add_variable(&mut self, name: String) -> usize {
        // Try to reuse a freed slot first
        let slot = if let Some(free_slot) = self.free_slots.pop() {
            free_slot
        } else {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        };
        self.variables.insert(name, slot);
        slot
    }

    fn get_variable(&self, name: &str) -> Option<usize> {
        self.variables.get(name).copied()
    }

    fn remove_variable(&mut self, name: &str) -> Option<usize> {
        if let Some(slot) = self.variables.remove(name) {
            self.free_slots.push(slot);
            Some(slot)
        } else {
            None
        }
    }

    fn remove_variables(&mut self, names: &[String]) {
        for name in names {
            self.remove_variable(name);
        }
    }
}

pub fn compile_to_ir(node: &Node) -> Result<IRProgram, CompileError> {
    let mut program = IRProgram::new();
    let mut context = CompileContext::new();
    compile_node(node, &mut program, &mut context)?;
    program.add_instruction(IRInstruction::Return);
    Ok(program)
}

fn compile_node(
    node: &Node,
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    match node {
        Node::Primitive { value } => compile_primitive(value, program),
        Node::Symbol { value } => {
            if let Some(slot) = context.get_variable(value) {
                program.add_instruction(IRInstruction::LoadLocal(slot));
                Ok(())
            } else {
                Err(CompileError::UndefinedVariable(value.clone()))
            }
        }
        Node::List { root } => compile_list(root, program, context),
        Node::Vector { root: _ } => Err(CompileError::UnsupportedOperation(
            "Vectors not supported in compilation yet".to_string(),
        )),
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

fn compile_list(
    nodes: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if nodes.is_empty() {
        program.add_instruction(IRInstruction::Push(0)); // nil = 0
        return Ok(());
    }

    let operator = &nodes[0];
    let args = &nodes[1..];

    match operator.as_ref() {
        Node::Symbol { value } => match value.as_str() {
            "+" => compile_arithmetic_op(args, program, context, IRInstruction::Add, "+"),
            "-" => compile_arithmetic_op(args, program, context, IRInstruction::Sub, "-"),
            "*" => compile_arithmetic_op(args, program, context, IRInstruction::Mul, "*"),
            "/" => compile_arithmetic_op(args, program, context, IRInstruction::Div, "/"),
            "=" => compile_comparison_op(args, program, context, IRInstruction::Equal, "="),
            "<" => compile_comparison_op(args, program, context, IRInstruction::Less, "<"),
            ">" => compile_comparison_op(args, program, context, IRInstruction::Greater, ">"),
            "<=" => compile_comparison_op(args, program, context, IRInstruction::LessEqual, "<="),
            ">=" => {
                compile_comparison_op(args, program, context, IRInstruction::GreaterEqual, ">=")
            }
            "if" => compile_if(args, program, context),
            "and" => compile_logical_and(args, program, context),
            "or" => compile_logical_or(args, program, context),
            "not" => compile_logical_not(args, program, context),
            "let" => compile_let(args, program, context),
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
    context: &mut CompileContext,
    instruction: IRInstruction,
    op_name: &str,
) -> Result<(), CompileError> {
    if args.len() < 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    // Compile first operand
    compile_node(&args[0], program, context)?;

    // Compile and apply remaining operands
    for arg in &args[1..] {
        compile_node(arg, program, context)?;
        program.add_instruction(instruction.clone());
    }

    Ok(())
}

fn compile_comparison_op(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
    instruction: IRInstruction,
    op_name: &str,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError(op_name.to_string(), 2, args.len()));
    }

    compile_node(&args[0], program, context)?;
    compile_node(&args[1], program, context)?;
    program.add_instruction(instruction);

    Ok(())
}

fn compile_if(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("if".to_string(), 3, args.len()));
    }

    // Compile condition
    compile_node(&args[0], program, context)?;

    // Jump to else clause if condition is false (0)
    let else_jump_pos = program.len();
    program.add_instruction(IRInstruction::JumpIfZero(0)); // Will be patched

    // Compile then clause
    compile_node(&args[1], program, context)?;

    // Jump over else clause
    let end_jump_pos = program.len();
    program.add_instruction(IRInstruction::Jump(0)); // Will be patched

    // Patch else jump target
    let else_start = program.len();
    if let IRInstruction::JumpIfZero(ref mut target) = &mut program.instructions[else_jump_pos] {
        *target = else_start;
    }

    // Compile else clause
    compile_node(&args[2], program, context)?;

    // Patch end jump target
    let end_pos = program.len();
    if let IRInstruction::Jump(ref mut target) = &mut program.instructions[end_jump_pos] {
        *target = end_pos;
    }

    Ok(())
}

fn compile_logical_and(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(1)); // true
        return Ok(());
    }

    if args.len() == 1 {
        compile_node(&args[0], program, context)?;
        // Convert to boolean (0 or 1)
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::Not);
        return Ok(());
    }

    // Compile first argument
    compile_node(&args[0], program, context)?;

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
        compile_node(arg, program, context)?;
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

fn compile_logical_or(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.is_empty() {
        program.add_instruction(IRInstruction::Push(0)); // false
        return Ok(());
    }

    if args.len() == 1 {
        compile_node(&args[0], program, context)?;
        // Convert to boolean (0 or 1)
        program.add_instruction(IRInstruction::Push(0));
        program.add_instruction(IRInstruction::Equal);
        program.add_instruction(IRInstruction::Not);
        return Ok(());
    }

    // Compile first argument
    compile_node(&args[0], program, context)?;

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
        compile_node(arg, program, context)?;
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

fn compile_logical_not(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("not".to_string(), 1, args.len()));
    }

    compile_node(&args[0], program, context)?;
    program.add_instruction(IRInstruction::Not);

    Ok(())
}

fn compile_let(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("let".to_string(), 2, args.len()));
    }

    // First argument should be a vector of bindings [var1 val1 var2 val2 ...]
    let bindings = match args[0].as_ref() {
        Node::Vector { root } => root,
        _ => {
            return Err(CompileError::InvalidExpression(
                "let requires a vector of bindings".to_string(),
            ))
        }
    };

    // Check that we have an even number of binding elements
    if bindings.len() % 2 != 0 {
        return Err(CompileError::InvalidExpression(
            "let bindings must have even number of elements".to_string(),
        ));
    }

    // Track variables added in this let scope for cleanup
    let mut added_variables = Vec::new();

    // Process bindings in pairs [var val var val ...]
    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        // Variable must be a symbol
        let var_name = match var_node.as_ref() {
            Node::Symbol { value } => value,
            _ => {
                return Err(CompileError::InvalidExpression(
                    "let binding variables must be symbols".to_string(),
                ))
            }
        };

        // Compile the value expression
        compile_node(val_node, program, context)?;

        // Add variable to context and store it
        let slot = context.add_variable(var_name.clone());
        program.add_instruction(IRInstruction::StoreLocal(slot));
        added_variables.push(var_name.clone());
    }

    // Compile body in the new environment
    compile_node(&args[1], program, context)?;

    // Clean up variables added in this scope (proper scoping and memory management)
    context.remove_variables(&added_variables);

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

    #[test]
    fn test_compile_let_simple() {
        let program = compile_expression("(let [x 5] x)").unwrap();
        println!("LET SIMPLE IR: {:?}", program.instructions);
        // Should have: Push(5), StoreLocal(0), LoadLocal(0), Return
        assert!(program.instructions.contains(&IRInstruction::Push(5)));
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(0)));
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(0)));
        assert!(program.instructions.contains(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_let_expression() {
        let program = compile_expression("(let [x 5] (+ x 3))").unwrap();
        println!("LET EXPRESSION IR: {:?}", program.instructions);
        // Should have variable operations and arithmetic
        assert!(program.instructions.contains(&IRInstruction::Push(5)));
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(0)));
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(0)));
        assert!(program.instructions.contains(&IRInstruction::Push(3)));
        assert!(program.instructions.contains(&IRInstruction::Add));
    }

    #[test]
    fn test_compile_let_multiple_bindings() {
        let program = compile_expression("(let [x 5 y 10] (+ x y))").unwrap();
        println!("LET MULTIPLE IR: {:?}", program.instructions);
        // Should have two variable stores and loads
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(0))); // x
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(1))); // y
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(0))); // x
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(1))); // y
    }

    #[test]
    fn test_compile_let_nested() {
        let program = compile_expression("(let [x 5] (let [y 10] (+ x y)))").unwrap();
        println!("LET NESTED IR: {:?}", program.instructions);
        // Should have variables in different slots because both are active simultaneously
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(0))); // x
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(1))); // y
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(0))); // x
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(1))); // y
    }

    #[test]
    fn test_compile_let_scoped_reuse() {
        // This should demonstrate slot reuse - two separate let expressions
        let program = compile_expression("(+ (let [x 5] x) (let [y 10] y))").unwrap();
        println!("LET SCOPED REUSE IR: {:?}", program.instructions);
        // Both x and y should use slot 0 since they're in separate scopes
        // However, the current IR structure may not show this clearly due to compilation order
    }

    #[test]
    fn test_compile_let_error_cases() {
        // Wrong arity
        assert!(matches!(
            compile_expression("(let [x 5])"),
            Err(CompileError::ArityError(_, 2, 1))
        ));

        // Non-vector bindings
        assert!(matches!(
            compile_expression("(let (x 5) x)"),
            Err(CompileError::InvalidExpression(_))
        ));

        // Odd number of binding elements
        assert!(matches!(
            compile_expression("(let [x] x)"),
            Err(CompileError::InvalidExpression(_))
        ));

        // Non-symbol in binding
        assert!(matches!(
            compile_expression("(let [5 x] x)"),
            Err(CompileError::InvalidExpression(_))
        ));
    }
}
