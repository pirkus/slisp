use super::{
    builtins::{emit_free_for_slot, free_retained_dependents},
    extend_with_offset,
    CompileContext,
    CompileError,
    CompileResult,
    HeapOwnership,
    RetainedSlot,
    ValueKind,
};
/// Function definition and call compilation
use crate::ast::Node;
use crate::compiler::liveness::{apply_liveness_plan, compute_liveness_plan};
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

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

    let mut func_context = context.new_function_scope(&func_name);

    for (i, param_name) in param_names.iter().enumerate() {
        func_context.add_parameter(param_name.clone(), i);
        if let Some(kind) = context.get_function_parameter_type(&func_name, i) {
            func_context.set_parameter_type(param_name, kind);
            if kind.is_heap_kind() {
                func_context.mark_heap_allocated(param_name, kind);
            }
        }
        if let Some(map_types) = context.get_function_parameter_map_value_types(&func_name, i) {
            func_context.set_parameter_map_value_types(param_name, Some(map_types.clone()));
        }
    }

    let mut instructions = vec![IRInstruction::DefineFunction(
        func_name.clone(),
        param_count,
        0, // Will be set by caller
    )];

    let mut body_result = crate::compiler::compile_node(&args[2], &mut func_context, program)?;
    let mut body_kind = body_result.kind;
    let body_map_value_types = body_result.map_value_types.clone();

    if body_result.heap_ownership == HeapOwnership::Borrowed {
        let clone_runtime = match body_kind {
            ValueKind::String => Some("_string_clone"),
            ValueKind::Vector => Some("_vector_clone"),
            ValueKind::Map => Some("_map_clone"),
            ValueKind::Set => Some("_set_clone"),
            ValueKind::Any => {
                body_kind = ValueKind::String;
                Some("_string_clone")
            }
            _ => None,
        };

        if let Some(runtime) = clone_runtime {
            body_result.instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
            body_result.heap_ownership = HeapOwnership::Owned;
        }
    }

    let body_ownership = body_result.heap_ownership;

    let body_instructions = std::mem::take(&mut body_result.instructions);
    extend_with_offset(&mut instructions, body_instructions);
    body_result.free_retained_slots(&mut instructions, &mut func_context);
    instructions.push(IRInstruction::Return);

    let func_info = FunctionInfo {
        name: func_name,
        param_count,
        start_address: 0,
        local_count: func_context.next_slot,
    };

    context.absorb_parameter_inference(&func_context);

    // Propagate inferred return metadata back to parent context
    context.set_function_return_type(&func_info.name, body_kind);
    context.set_function_return_ownership(&func_info.name, body_ownership);
    context.set_function_return_map_value_types(&func_info.name, body_map_value_types);

    Ok((instructions, func_info))
}

/// Compile a function call
pub fn compile_function_call(func_name: &str, args: &[Node], context: &mut CompileContext, program: &mut IRProgram, expected_param_count: usize) -> Result<CompileResult, CompileError> {
    if args.len() != expected_param_count {
        return Err(CompileError::ArityError(func_name.to_string(), expected_param_count, args.len()));
    }

    let mut instructions = Vec::new();
    let mut owned_argument_slots: Vec<usize> = Vec::new();
    let mut retained_argument_slots: Vec<RetainedSlot> = Vec::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();

    for (index, arg) in args.iter().enumerate() {
        let mut arg_result = crate::compiler::compile_node(arg, context, program)?;
        context.record_function_parameter_type(func_name, index, arg_result.kind);
        retained_argument_slots.extend(arg_result.take_retained_slots());
        let arg_instructions = std::mem::take(&mut arg_result.instructions);
        extend_with_offset(&mut instructions, arg_instructions);
        if arg_result.heap_ownership == HeapOwnership::Owned {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            owned_argument_slots.push(slot);
            slot_kinds.insert(slot, arg_result.kind);
        }
    }

    instructions.push(IRInstruction::Call(func_name.to_string(), args.len()));

    if !owned_argument_slots.is_empty() || !retained_argument_slots.is_empty() {
        let mut tracked_slots: HashSet<usize> = owned_argument_slots.iter().copied().collect();
        tracked_slots.extend(retained_argument_slots.iter().map(|slot| slot.slot));
        for slot in &retained_argument_slots {
            slot_kinds.insert(slot.slot, slot.kind);
        }
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_liveness_plan(instructions, &plan, |insts, slot| {
            let kind = slot_kinds.get(&slot).copied().unwrap_or(ValueKind::Any);
            emit_free_for_slot(insts, slot, kind);
        });
    }

    for slot in owned_argument_slots {
        context.release_temp_slot(slot);
    }
    for mut slot in retained_argument_slots {
        free_retained_dependents(&mut slot, &mut instructions, context);
        context.release_temp_slot(slot.slot);
    }

    // Without full type inference, assume any return kind for user-defined functions.
    let return_kind = context.get_function_return_type(func_name).unwrap_or(ValueKind::Any);
    let return_ownership = context.get_function_return_ownership(func_name).unwrap_or(HeapOwnership::None);
    let map_value_types = context.get_function_return_map_value_types(func_name).cloned();

    Ok(CompileResult::with_instructions(instructions, return_kind)
        .with_heap_ownership(return_ownership)
        .with_map_value_types(map_value_types))
}
