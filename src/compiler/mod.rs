mod bindings;
/// Compiler module - compiles AST nodes to IR
///
/// This module is organized into:
/// - context: CompileContext for tracking variables, parameters, and functions
/// - expressions: Arithmetic, comparisons, conditionals, and logical operations
/// - functions: Function definitions (defn) and function calls
/// - bindings: Variable bindings (let expressions)
mod context;
mod expressions;
mod functions;
mod liveness;

pub use context::CompileContext;

use crate::ast::Node;
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Any,
    Number,
    Boolean,
    String,
    Nil,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HeapOwnership {
    None,
    Borrowed,
    Owned,
}

impl HeapOwnership {
    pub fn combine(self, other: Self) -> Self {
        use HeapOwnership::*;
        match (self, other) {
            (Owned, Owned) => Owned,
            (None, None) => None,
            (None, Borrowed) | (Borrowed, None) | (Borrowed, Borrowed) => Borrowed,
            _ => Borrowed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompileResult {
    pub instructions: Vec<IRInstruction>,
    pub kind: ValueKind,
    pub heap_ownership: HeapOwnership,
}

impl CompileResult {
    pub fn with_instructions(instructions: Vec<IRInstruction>, kind: ValueKind) -> Self {
        Self {
            instructions,
            kind,
            heap_ownership: HeapOwnership::None,
        }
    }

    pub fn with_heap_ownership(mut self, ownership: HeapOwnership) -> Self {
        self.heap_ownership = ownership;
        self
    }
}

/// Determine if a symbol refers to a heap-allocated local variable in the current context.
pub(crate) fn is_heap_allocated_symbol(name: &str, context: &CompileContext) -> bool {
    (context.get_variable(name).is_some() || context.get_parameter(name).is_some()) && context.is_heap_allocated(name)
}

#[derive(Debug, PartialEq)]
pub enum CompileError {
    UnsupportedOperation(String),
    InvalidExpression(String),
    ArityError(String, usize, usize),
    UndefinedVariable(String),
    DuplicateFunction(String),
}

/// Compile a single expression to IR
pub fn compile_to_ir(node: &Node) -> Result<IRProgram, CompileError> {
    let mut program = IRProgram::new();
    let mut context = CompileContext::new();
    let result = compile_node(node, &mut context, &mut program)?;
    for instruction in result.instructions {
        program.add_instruction(instruction);
    }
    program.add_instruction(IRInstruction::Return);
    Ok(program)
}

/// Compile a program (multiple top-level expressions) to IR
pub fn compile_program(expressions: &[Node]) -> Result<IRProgram, CompileError> {
    let mut program = IRProgram::new();
    let mut context = CompileContext::new();

    // First pass: find all function definitions
    for expr in expressions {
        if let Node::List { root } = expr {
            if !root.is_empty() {
                if let Node::Symbol { value } = &root[0] {
                    if value == "defn" {
                        // Register function in context but don't compile yet
                        if root.len() != 4 {
                            return Err(CompileError::ArityError("defn".to_string(), 3, root.len() - 1));
                        }

                        let func_name = match &root[1] {
                            Node::Symbol { value } => value.clone(),
                            _ => return Err(CompileError::InvalidExpression("Function name must be a symbol".to_string())),
                        };

                        let params = match &root[2] {
                            Node::Vector { root } => root,
                            _ => return Err(CompileError::InvalidExpression("Function parameters must be a vector".to_string())),
                        };

                        let func_info = FunctionInfo {
                            name: func_name.clone(),
                            param_count: params.len(),
                            start_address: 0, // Will be set during compilation
                            local_count: 0,
                        };
                        context.add_function(func_name, func_info)?;
                    }
                }
            }
        }
    }

    // Second pass: compile all expressions
    for expr in expressions {
        if let Node::List { root } = expr {
            if !root.is_empty() {
                if let Node::Symbol { value } = &root[0] {
                    if value == "defn" {
                        let (mut instructions, func_info) = functions::compile_defn(&root[1..], &mut context, &mut program)?;
                        let start_address = program.len();

                        if let IRInstruction::DefineFunction(ref name, ref params, _) = instructions[0] {
                            instructions[0] = IRInstruction::DefineFunction(name.clone(), *params, start_address);
                        }

                        let updated_func_info = crate::ir::FunctionInfo {
                            name: func_info.name,
                            param_count: func_info.param_count,
                            start_address,
                            local_count: func_info.local_count,
                        };

                        for instruction in instructions {
                            program.add_instruction(instruction);
                        }
                        program.add_function(updated_func_info);
                        continue;
                    }
                }
            }
        }

        let result = compile_node(expr, &mut context, &mut program)?;
        for instruction in result.instructions {
            program.add_instruction(instruction);
        }
    }

    if context.get_function("-main").is_some() {
        program.set_entry_point("-main".to_string());
    }

    Ok(program)
}

/// Compile a single AST node to IR
pub(crate) fn compile_node(node: &Node, context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    match node {
        Node::Primitive { value } => expressions::compile_primitive(value, program),
        Node::Symbol { value } => {
            if value == "nil" {
                Ok(CompileResult::with_instructions(vec![IRInstruction::Push(0)], ValueKind::Nil))
            } else if let Some(slot) = context.get_parameter(value) {
                let kind = context.get_parameter_type(value).unwrap_or(ValueKind::Any);
                let ownership = if kind == ValueKind::String && context.is_heap_allocated(value) {
                    HeapOwnership::Borrowed
                } else {
                    HeapOwnership::None
                };
                Ok(CompileResult::with_instructions(vec![IRInstruction::LoadParam(slot)], kind).with_heap_ownership(ownership))
            } else if let Some(slot) = context.get_variable(value) {
                let kind = context.get_variable_type(value).unwrap_or(ValueKind::Any);
                let ownership = if kind == ValueKind::String && context.is_heap_allocated(value) {
                    HeapOwnership::Borrowed
                } else {
                    HeapOwnership::None
                };
                Ok(CompileResult::with_instructions(vec![IRInstruction::LoadLocal(slot)], kind).with_heap_ownership(ownership))
            } else {
                Err(CompileError::UndefinedVariable(value.clone()))
            }
        }
        Node::List { root } => compile_list(root, context, program),
        Node::Vector { root: _ } => Err(CompileError::UnsupportedOperation("Vectors not supported in compilation yet".to_string())),
    }
}

/// Compile count operation (string length)
fn compile_count(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("count".to_string(), 1, args.len()));
    }

    // Compile the argument (should be a string)
    let mut instructions = compile_node(&args[0], context, program)?.instructions;

    // Call _string_count runtime function (takes 1 arg: string pointer)
    instructions.push(IRInstruction::RuntimeCall("_string_count".to_string(), 1));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Number))
}

/// Compile get operation (string indexing)
fn compile_get(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("get".to_string(), 2, args.len()));
    }

    let mut instructions = compile_node(&args[0], context, program)?.instructions;
    instructions.extend(compile_node(&args[1], context, program)?.instructions);
    instructions.push(IRInstruction::RuntimeCall("_string_get".to_string(), 2));

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}

/// Compile subs operation (substring extraction)
fn compile_subs(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("subs".to_string(), 2, args.len()));
    }

    let mut instructions = compile_node(&args[0], context, program)?.instructions;
    instructions.extend(compile_node(&args[1], context, program)?.instructions);

    if args.len() == 3 {
        instructions.extend(compile_node(&args[2], context, program)?.instructions);
    } else {
        instructions.push(IRInstruction::Push(-1));
    }

    instructions.push(IRInstruction::RuntimeCall("_string_subs".to_string(), 3));

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}

/// Compile str operation (string concatenation)
fn compile_str(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![IRInstruction::Push(0), IRInstruction::Push(0), IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2)],
            ValueKind::String,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = args.len();
    let mut instructions = Vec::new();
    let mut temp_slots = Vec::with_capacity(count);
    let mut needs_free = vec![false; count];

    for _ in 0..count {
        temp_slots.push(context.allocate_temp_slot());
    }

    for (index, arg) in args.iter().enumerate() {
        let arg_result = compile_node(arg, context, program)?;
        instructions.extend(arg_result.instructions);

        let slot_index = count - 1 - index;
        let slot = temp_slots[slot_index];

        let mut slot_needs_free = arg_result.heap_ownership == HeapOwnership::Owned;

        match arg_result.kind {
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
                    slot_needs_free = true;
                }
            }
            ValueKind::Nil => {
                instructions.push(IRInstruction::Push(0));
                instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
                slot_needs_free = false;
            }
            ValueKind::Boolean => {
                instructions.push(IRInstruction::RuntimeCall("_string_from_boolean".to_string(), 1));
                slot_needs_free = false;
            }
            ValueKind::Number | ValueKind::Any => {
                instructions.push(IRInstruction::RuntimeCall("_string_from_number".to_string(), 1));
                slot_needs_free = true;
            }
        }

        instructions.push(IRInstruction::StoreLocal(slot));
        needs_free[slot_index] = slot_needs_free;
    }

    let base_slot = temp_slots[count - 1];
    instructions.push(IRInstruction::PushLocalAddress(base_slot));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2));

    for (slot, free) in temp_slots.iter().zip(needs_free.iter()) {
        if *free {
            instructions.push(IRInstruction::FreeLocal(*slot));
        }
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}

/// Compile a list (function call or special form) to IR
fn compile_list(nodes: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if nodes.is_empty() {
        return Ok(CompileResult::with_instructions(vec![IRInstruction::Push(0)], ValueKind::Nil));
    }

    let operator = &nodes[0];
    let args = &nodes[1..];

    match operator {
        Node::Symbol { value } => match value.as_str() {
            "+" => expressions::compile_arithmetic_op(args, context, program, IRInstruction::Add, "+"),
            "-" => expressions::compile_arithmetic_op(args, context, program, IRInstruction::Sub, "-"),
            "*" => expressions::compile_arithmetic_op(args, context, program, IRInstruction::Mul, "*"),
            "/" => expressions::compile_arithmetic_op(args, context, program, IRInstruction::Div, "/"),
            "=" => expressions::compile_comparison_op(args, context, program, IRInstruction::Equal, "="),
            "<" => expressions::compile_comparison_op(args, context, program, IRInstruction::Less, "<"),
            ">" => expressions::compile_comparison_op(args, context, program, IRInstruction::Greater, ">"),
            "<=" => expressions::compile_comparison_op(args, context, program, IRInstruction::LessEqual, "<="),
            ">=" => expressions::compile_comparison_op(args, context, program, IRInstruction::GreaterEqual, ">="),
            "if" => expressions::compile_if(args, context, program),
            "and" => expressions::compile_logical_and(args, context, program),
            "or" => expressions::compile_logical_or(args, context, program),
            "not" => expressions::compile_logical_not(args, context, program),
            "let" => bindings::compile_let(args, context, program),
            "defn" => {
                let (instructions, _) = functions::compile_defn(args, context, program)?;
                Ok(CompileResult::with_instructions(instructions, ValueKind::Nil))
            }
            "count" => compile_count(args, context, program),
            "get" => compile_get(args, context, program),
            "subs" => compile_subs(args, context, program),
            "str" => compile_str(args, context, program),
            op => {
                if let Some(func_info) = context.get_function(op) {
                    functions::compile_function_call(op, args, context, program, func_info.param_count)
                } else {
                    Err(CompileError::UnsupportedOperation(op.to_string()))
                }
            }
        },
        _ => Err(CompileError::InvalidExpression("First element must be a symbol".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstParser, AstParserTrt};

    fn compile_expression(input: &str) -> Result<IRProgram, CompileError> {
        let ast = AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0);
        compile_to_ir(&ast)
    }

    #[test]
    fn test_compile_number() {
        let program = compile_expression("42").unwrap();
        assert_eq!(program.instructions, vec![IRInstruction::Push(42), IRInstruction::Return]);
    }

    #[test]
    fn test_compile_boolean_literal() {
        let program = compile_expression("true").unwrap();
        assert_eq!(program.instructions, vec![IRInstruction::Push(1), IRInstruction::Return]);
    }

    #[test]
    fn test_compile_arithmetic() {
        let program = compile_expression("(+ 2 3)").unwrap();
        assert_eq!(program.instructions, vec![IRInstruction::Push(2), IRInstruction::Push(3), IRInstruction::Add, IRInstruction::Return]);
    }

    #[test]
    fn test_compile_str_zero_args() {
        let program = compile_expression("(str)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2),
                IRInstruction::Return,
            ]
        );
    }

    #[test]
    fn test_compile_str_single_arg() {
        let program = compile_expression("(str \"hi\")").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::PushString(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_string_normalize".to_string(), 2),
                IRInstruction::StoreLocal(0),
                IRInstruction::PushLocalAddress(0),
                IRInstruction::Push(1),
                IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2),
                IRInstruction::Return,
            ]
        );
        assert_eq!(program.string_literals, vec!["hi".to_string()]);
    }

    #[test]
    fn test_compile_str_three_args() {
        let program = compile_expression("(str \"a\" \"b\" \"c\")").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::PushString(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_string_normalize".to_string(), 2),
                IRInstruction::StoreLocal(2),
                IRInstruction::PushString(1),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_string_normalize".to_string(), 2),
                IRInstruction::StoreLocal(1),
                IRInstruction::PushString(2),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_string_normalize".to_string(), 2),
                IRInstruction::StoreLocal(0),
                IRInstruction::PushLocalAddress(2),
                IRInstruction::Push(3),
                IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2),
                IRInstruction::Return,
            ]
        );
        assert_eq!(program.string_literals, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_compile_str_with_number() {
        let program = compile_expression("(str 42)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(42),
                IRInstruction::RuntimeCall("_string_from_number".to_string(), 1),
                IRInstruction::StoreLocal(0),
                IRInstruction::PushLocalAddress(0),
                IRInstruction::Push(1),
                IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2),
                IRInstruction::FreeLocal(0),
                IRInstruction::Return,
            ]
        );
    }

    #[test]
    fn test_compile_str_with_boolean() {
        let program = compile_expression("(str (= 1 1))").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(1),
                IRInstruction::Push(1),
                IRInstruction::Equal,
                IRInstruction::RuntimeCall("_string_from_boolean".to_string(), 1),
                IRInstruction::StoreLocal(0),
                IRInstruction::PushLocalAddress(0),
                IRInstruction::Push(1),
                IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2),
                IRInstruction::Return,
            ]
        );
    }

    #[test]
    fn test_compile_str_with_nil() {
        let program = compile_expression("(str ())").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_string_normalize".to_string(), 2),
                IRInstruction::StoreLocal(0),
                IRInstruction::PushLocalAddress(0),
                IRInstruction::Push(1),
                IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2),
                IRInstruction::Return,
            ]
        );
    }

    #[test]
    fn test_compile_get_string_index() {
        let program = compile_expression("(get \"abc\" 1)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::PushString(0),
                IRInstruction::Push(1),
                IRInstruction::RuntimeCall("_string_get".to_string(), 2),
                IRInstruction::Return,
            ]
        );
        assert_eq!(program.string_literals, vec!["abc".to_string()]);
    }

    #[test]
    fn test_compile_subs_with_end() {
        let program = compile_expression("(subs \"hello\" 1 3)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::PushString(0),
                IRInstruction::Push(1),
                IRInstruction::Push(3),
                IRInstruction::RuntimeCall("_string_subs".to_string(), 3),
                IRInstruction::Return,
            ]
        );
        assert_eq!(program.string_literals, vec!["hello".to_string()]);
    }

    #[test]
    fn test_compile_subs_without_end() {
        let program = compile_expression("(subs \"hello\" 2)").unwrap();
        assert_eq!(
            program.instructions,
            vec![
                IRInstruction::PushString(0),
                IRInstruction::Push(2),
                IRInstruction::Push(-1),
                IRInstruction::RuntimeCall("_string_subs".to_string(), 3),
                IRInstruction::Return,
            ]
        );
        assert_eq!(program.string_literals, vec!["hello".to_string()]);
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
        assert_eq!(program.instructions, vec![IRInstruction::Push(0), IRInstruction::Not, IRInstruction::Return]);
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
        assert!(matches!(compile_expression("(let [x 5])"), Err(CompileError::ArityError(_, 2, 1))));

        // Non-vector bindings
        assert!(matches!(compile_expression("(let (x 5) x)"), Err(CompileError::InvalidExpression(_))));

        // Odd number of binding elements
        assert!(matches!(compile_expression("(let [x] x)"), Err(CompileError::InvalidExpression(_))));

        // Non-symbol in binding
        assert!(matches!(compile_expression("(let [5 x] x)"), Err(CompileError::InvalidExpression(_))));
    }

    #[test]
    fn test_compile_defn() {
        let program = compile_expression("(defn add [x y] (+ x y))").unwrap();
        println!("DEFN IR: {:?}", program.instructions);

        // Should have DefineFunction instruction
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::DefineFunction(name, param_count, _)
            if name == "add" && *param_count == 2
        )));

        // Should have parameter loads and arithmetic
        assert!(program.instructions.contains(&IRInstruction::LoadParam(0))); // x
        assert!(program.instructions.contains(&IRInstruction::LoadParam(1))); // y
        assert!(program.instructions.contains(&IRInstruction::Add));
        assert!(program.instructions.contains(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_function_call() {
        // This test requires a two-pass compilation since we need the function definition first
        let expressions = vec![
            AstParser::parse_sexp_new_domain("(defn inc [x] (+ x 1))".as_bytes(), &mut 0),
            AstParser::parse_sexp_new_domain("(inc 5)".as_bytes(), &mut 0),
        ];

        let program = compile_program(&expressions).unwrap();
        println!("FUNCTION CALL IR: {:?}", program.instructions);

        // Should have function definition
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::DefineFunction(name, param_count, _)
            if name == "inc" && *param_count == 1
        )));

        // Should have function call
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::Call(name, arg_count)
            if name == "inc" && *arg_count == 1
        )));

        // Should push argument before call
        assert!(program.instructions.contains(&IRInstruction::Push(5)));
    }

    #[test]
    fn test_clone_returned_local_string() {
        let program = compile_expression("(let [s (str \"a\" \"b\")] s)").unwrap();
        let clone_pos = program
            .instructions
            .iter()
            .position(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 1) if name == "_string_clone"));
        assert!(clone_pos.is_some(), "expected clone runtime call in instructions: {:?}", program.instructions);

        let free_pos = program
            .instructions
            .iter()
            .position(|inst| matches!(inst, IRInstruction::FreeLocal(_)))
            .expect("expected FreeLocal instruction");
        assert!(clone_pos.unwrap() < free_pos, "clone should occur before FreeLocal");
    }

    #[test]
    fn test_clone_argument_for_function_call() {
        let expressions = vec![
            AstParser::parse_sexp_new_domain("(defn id [x] x)".as_bytes(), &mut 0),
            AstParser::parse_sexp_new_domain("(let [s (str \"a\" \"b\")] (id s))".as_bytes(), &mut 0),
        ];

        let program = compile_program(&expressions).unwrap();

        let call_pos = program
            .instructions
            .iter()
            .position(|inst| matches!(inst, IRInstruction::Call(name, 1) if name == "id"))
            .expect("expected call instruction for id");

        assert!(
            !program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 1) if name == "_string_clone")),
            "arguments should be passed by borrowing without cloning"
        );

        assert!(call_pos >= 2, "call should have preceding store/load for borrowed arg");
        let store_slot = match &program.instructions[call_pos - 2] {
            IRInstruction::StoreLocal(slot) => *slot,
            other => panic!("expected StoreLocal before call, found {:?}", other),
        };

        match &program.instructions[call_pos - 1] {
            IRInstruction::LoadLocal(slot) if *slot == store_slot => {}
            other => panic!("expected LoadLocal for slot {} before call, found {:?}", store_slot, other),
        }

        match &program.instructions[call_pos + 1] {
            IRInstruction::FreeLocal(slot) if *slot == store_slot => {}
            other => panic!("expected FreeLocal for slot {} after call, found {:?}", store_slot, other),
        }
    }

    #[test]
    fn test_compile_main_function() {
        let expressions = vec![
            AstParser::parse_sexp_new_domain("(defn add [x y] (+ x y))".as_bytes(), &mut 0),
            AstParser::parse_sexp_new_domain("(defn -main [] (add 3 4))".as_bytes(), &mut 0),
        ];

        let program = compile_program(&expressions).unwrap();
        println!("MAIN FUNCTION IR: {:?}", program);

        // Should have entry point set
        assert_eq!(program.entry_point, Some("-main".to_string()));

        // Should have both function definitions
        assert!(program.functions.iter().any(|f| f.name == "add"));
        assert!(program.functions.iter().any(|f| f.name == "-main"));
    }

    #[test]
    fn test_compile_function_error_cases() {
        // Wrong arity
        assert!(matches!(compile_expression("(defn add [x])"), Err(CompileError::ArityError(_, 3, 2))));

        // Non-symbol function name
        assert!(matches!(compile_expression("(defn 123 [x] x)"), Err(CompileError::InvalidExpression(_))));

        // Non-vector parameters
        assert!(matches!(compile_expression("(defn add (x y) (+ x y))"), Err(CompileError::InvalidExpression(_))));

        // Non-symbol parameter
        assert!(matches!(compile_expression("(defn add [x 123] (+ x 123))"), Err(CompileError::InvalidExpression(_))));
    }
}
