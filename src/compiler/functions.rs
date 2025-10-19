use super::{CompileContext, CompileError, CompileResult, ValueKind};
/// Function definition and call compilation
use crate::ast::Node;
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};

/// Compile a function definition (defn)
pub fn compile_defn(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<(Vec<IRInstruction>, FunctionInfo), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("defn".to_string(), 3, args.len()));
    }

    let func_name = match &args[0] {
        Node::Symbol { value } => value.clone(),
        _ => return Err(CompileError::InvalidExpression("Function name must be a symbol".to_string())),
    };

    let params = match &args[1] {
        Node::Vector { root } => root,
        _ => return Err(CompileError::InvalidExpression("Function parameters must be a vector".to_string())),
    };

    let mut param_names = Vec::new();
    for param in params {
        match param {
            Node::Symbol { value } => param_names.push(value.clone()),
            _ => return Err(CompileError::InvalidExpression("Function parameters must be symbols".to_string())),
        }
    }

    let param_count = param_names.len();

    if context.get_function(&func_name).is_none() {
        let func_info = FunctionInfo {
            name: func_name.clone(),
            param_count,
            start_address: 0,
            local_count: 0,
        };
        context.add_function(func_name.clone(), func_info)?;
    }

    let mut func_context = context.new_function_scope();

    for (i, param_name) in param_names.iter().enumerate() {
        func_context.add_parameter(param_name.clone(), i);
        if let Some(kind) = context.get_function_parameter_type(&func_name, i) {
            func_context.set_parameter_type(param_name, kind);
            if kind == ValueKind::String {
                func_context.mark_heap_allocated(param_name);
            }
        }
    }

    let mut instructions = vec![IRInstruction::DefineFunction(
        func_name.clone(),
        param_count,
        0, // Will be set by caller
    )];

    let body_result = crate::compiler::compile_node(&args[2], &mut func_context, program)?;
    let body_kind = body_result.kind;
    instructions.extend(body_result.instructions);
    instructions.push(IRInstruction::Return);

    let func_info = FunctionInfo {
        name: func_name,
        param_count,
        start_address: 0,
        local_count: func_context.next_slot,
    };

    // Propagate inferred return type back to parent context
    context.set_function_return_type(&func_info.name, body_kind);

    Ok((instructions, func_info))
}

/// Compile a function call
pub fn compile_function_call(func_name: &str, args: &[Node], context: &mut CompileContext, program: &mut IRProgram, expected_param_count: usize) -> Result<CompileResult, CompileError> {
    if args.len() != expected_param_count {
        return Err(CompileError::ArityError(func_name.to_string(), expected_param_count, args.len()));
    }

    let mut instructions = Vec::new();

    for (index, arg) in args.iter().enumerate() {
        let mut arg_result = crate::compiler::compile_node(arg, context, program)?;
        if let Node::Symbol { value } = arg {
            if crate::compiler::is_heap_allocated_symbol(value, context) {
                arg_result.instructions.push(IRInstruction::RuntimeCall("_string_clone".to_string(), 1));
                arg_result.owns_heap = true;
            }
        }
        context.record_function_parameter_type(func_name, index, arg_result.kind);
        instructions.extend(arg_result.instructions);
    }

    instructions.push(IRInstruction::Call(func_name.to_string(), args.len()));

    // Without full type inference, assume any return kind for user-defined functions.
    Ok(CompileResult::with_instructions(instructions, context.get_function_return_type(func_name).unwrap_or(ValueKind::Any)))
}
