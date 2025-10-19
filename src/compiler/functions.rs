use super::{CompileContext, CompileError, CompileResult, HeapOwnership, ValueKind};
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

    let mut body_result = crate::compiler::compile_node(&args[2], &mut func_context, program)?;
    let mut body_kind = body_result.kind;

    if body_result.heap_ownership == HeapOwnership::Borrowed {
        body_result.instructions.push(IRInstruction::RuntimeCall("_string_clone".to_string(), 1));
        body_result.heap_ownership = HeapOwnership::Owned;
        if body_kind == ValueKind::Any {
            body_kind = ValueKind::String;
        }
    }

    let body_ownership = body_result.heap_ownership;

    instructions.extend(body_result.instructions);
    instructions.push(IRInstruction::Return);

    let func_info = FunctionInfo {
        name: func_name,
        param_count,
        start_address: 0,
        local_count: func_context.next_slot,
    };

    // Propagate inferred return metadata back to parent context
    context.set_function_return_type(&func_info.name, body_kind);
    context.set_function_return_ownership(&func_info.name, body_ownership);

    Ok((instructions, func_info))
}

/// Compile a function call
pub fn compile_function_call(func_name: &str, args: &[Node], context: &mut CompileContext, program: &mut IRProgram, expected_param_count: usize) -> Result<CompileResult, CompileError> {
    if args.len() != expected_param_count {
        return Err(CompileError::ArityError(func_name.to_string(), expected_param_count, args.len()));
    }

    let mut instructions = Vec::new();
    let mut owned_argument_slots: Vec<Option<usize>> = Vec::with_capacity(args.len());

    for (index, arg) in args.iter().enumerate() {
        let arg_result = crate::compiler::compile_node(arg, context, program)?;
        context.record_function_parameter_type(func_name, index, arg_result.kind);
        instructions.extend(arg_result.instructions);
        if arg_result.heap_ownership == HeapOwnership::Owned {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            owned_argument_slots.push(Some(slot));
        } else {
            owned_argument_slots.push(None);
        }
    }

    instructions.push(IRInstruction::Call(func_name.to_string(), args.len()));

    for slot in owned_argument_slots.into_iter().flatten() {
        instructions.push(IRInstruction::FreeLocal(slot));
        context.release_temp_slot(slot);
    }

    // Without full type inference, assume any return kind for user-defined functions.
    let return_kind = context.get_function_return_type(func_name).unwrap_or(ValueKind::Any);
    let return_ownership = context.get_function_return_ownership(func_name).unwrap_or(HeapOwnership::None);

    Ok(CompileResult::with_instructions(instructions, return_kind).with_heap_ownership(return_ownership))
}
