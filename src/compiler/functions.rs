/// Function definition and call compilation

use crate::domain::Node;
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use super::{CompileContext, CompileError};

/// Compile a function definition (defn)
pub fn compile_defn(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("defn".to_string(), 3, args.len()));
    }

    // Function name
    let func_name = match args[0].as_ref() {
        Node::Symbol { value } => value.clone(),
        _ => {
            return Err(CompileError::InvalidExpression(
                "Function name must be a symbol".to_string(),
            ))
        }
    };

    // Parameters vector
    let params = match args[1].as_ref() {
        Node::Vector { root } => root,
        _ => {
            return Err(CompileError::InvalidExpression(
                "Function parameters must be a vector".to_string(),
            ))
        }
    };

    // Extract parameter names
    let mut param_names = Vec::new();
    for param in params {
        match param.as_ref() {
            Node::Symbol { value } => param_names.push(value.clone()),
            _ => {
                return Err(CompileError::InvalidExpression(
                    "Function parameters must be symbols".to_string(),
                ))
            }
        }
    }

    let param_count = param_names.len();
    let start_address = program.len();

    // Create function info and add to context if not already present
    let func_info = FunctionInfo {
        name: func_name.clone(),
        param_count,
        start_address,
        local_count: 0, // Will be updated during compilation
    };

    // Only add if not already in context (could be pre-registered by compile_program)
    if context.get_function(&func_name).is_none() {
        context.add_function(func_name.clone(), func_info.clone())?;
    }

    // Create new context for function compilation
    let mut func_context = context.clone();
    func_context.in_function = true;
    func_context.parameters.clear();
    func_context.variables.clear();
    func_context.next_slot = 0;
    func_context.free_slots.clear();

    // Add parameters to function context
    for (i, param_name) in param_names.iter().enumerate() {
        func_context.add_parameter(param_name.clone(), i);
    }

    // Add function definition instruction
    program.add_instruction(IRInstruction::DefineFunction(
        func_name.clone(),
        param_count,
        start_address,
    ));

    // Compile function body
    crate::compiler::compile_node(&args[2], program, &mut func_context)?;

    // Add return instruction
    program.add_instruction(IRInstruction::Return);

    // Update function info in program
    let mut updated_func_info = func_info;
    updated_func_info.local_count = func_context.next_slot;
    program.add_function(updated_func_info);

    Ok(())
}

/// Compile a function call
pub fn compile_function_call(
    func_name: &str,
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
    expected_param_count: usize,
) -> Result<(), CompileError> {
    if args.len() != expected_param_count {
        return Err(CompileError::ArityError(
            func_name.to_string(),
            expected_param_count,
            args.len(),
        ));
    }

    // Compile arguments (they will be pushed onto stack)
    for arg in args {
        crate::compiler::compile_node(arg, program, context)?;
    }

    // Call the function
    program.add_instruction(IRInstruction::Call(func_name.to_string(), args.len()));

    Ok(())
}
