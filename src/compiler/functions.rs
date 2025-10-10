/// Function definition and call compilation

use crate::domain::Node;
use crate::ir::{FunctionInfo, IRInstruction};
use super::{CompileContext, CompileError};

/// Compile a function definition (defn)
pub fn compile_defn(
    args: &[Node],
    context: &mut CompileContext,
) -> Result<(Vec<IRInstruction>, FunctionInfo), CompileError> {
    if args.len() != 3 {
        return Err(CompileError::ArityError("defn".to_string(), 3, args.len()));
    }

    let func_name = match &args[0] {
        Node::Symbol { value } => value.clone(),
        _ => {
            return Err(CompileError::InvalidExpression(
                "Function name must be a symbol".to_string(),
            ))
        }
    };

    let params = match &args[1] {
        Node::Vector { root } => root,
        _ => {
            return Err(CompileError::InvalidExpression(
                "Function parameters must be a vector".to_string(),
            ))
        }
    };

    let mut param_names = Vec::new();
    for param in params {
        match param {
            Node::Symbol { value } => param_names.push(value.clone()),
            _ => {
                return Err(CompileError::InvalidExpression(
                    "Function parameters must be symbols".to_string(),
                ))
            }
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

    let mut func_context = context.clone();
    func_context.in_function = true;
    func_context.parameters.clear();
    func_context.variables.clear();
    func_context.next_slot = 0;
    func_context.free_slots.clear();

    for (i, param_name) in param_names.iter().enumerate() {
        func_context.add_parameter(param_name.clone(), i);
    }

    let mut instructions = vec![IRInstruction::DefineFunction(
        func_name.clone(),
        param_count,
        0, // Will be set by caller
    )];

    instructions.extend(crate::compiler::compile_node(&args[2], &mut func_context)?);
    instructions.push(IRInstruction::Return);

    let func_info = FunctionInfo {
        name: func_name,
        param_count,
        start_address: 0,
        local_count: func_context.next_slot,
    };

    Ok((instructions, func_info))
}

/// Compile a function call
pub fn compile_function_call(
    func_name: &str,
    args: &[Node],
    context: &mut CompileContext,
    expected_param_count: usize,
) -> Result<Vec<IRInstruction>, CompileError> {
    if args.len() != expected_param_count {
        return Err(CompileError::ArityError(
            func_name.to_string(),
            expected_param_count,
            args.len(),
        ));
    }

    let mut instructions = Vec::new();

    for arg in args {
        instructions.extend(crate::compiler::compile_node(arg, context)?);
    }

    instructions.push(IRInstruction::Call(func_name.to_string(), args.len()));

    Ok(instructions)
}
