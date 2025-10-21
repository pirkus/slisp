use super::{CompileContext, CompileError, CompileResult, HeapOwnership, ValueKind};
/// Variable binding compilation (let expressions)
use crate::ast::Node;
use crate::compiler::liveness::{apply_liveness_plan, compute_liveness_plan};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::HashSet;

#[derive(Clone, Debug)]
struct BindingInfo {
    slot: usize,
    owns_heap: bool,
}

/// Compile a let binding expression
pub fn compile_let(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("let".to_string(), 2, args.len()));
    }

    let bindings = match &args[0] {
        Node::Vector { root } => root,
        _ => return Err(CompileError::InvalidExpression("let requires a vector of bindings".to_string())),
    };

    if bindings.len() % 2 != 0 {
        return Err(CompileError::InvalidExpression("let bindings must have even number of elements".to_string()));
    }

    let mut instructions = Vec::new();
    let mut added_variables = Vec::new();
    let mut binding_infos: Vec<BindingInfo> = Vec::new();

    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        let var_name = match var_node {
            Node::Symbol { value } => value,
            _ => return Err(CompileError::InvalidExpression("let binding variables must be symbols".to_string())),
        };

        let mut value_result = crate::compiler::compile_node(val_node, context, program)?;

        let mut cloned_from_existing = false;
        if let Node::Symbol { value } = val_node {
            if crate::compiler::is_heap_allocated_symbol(value, context) {
                value_result.instructions.push(IRInstruction::RuntimeCall("_string_clone".to_string(), 1));
                value_result.heap_ownership = HeapOwnership::Owned;
                cloned_from_existing = true;
            }
        }

        instructions.extend(value_result.instructions);

        let slot = context.add_variable(var_name.clone());
        instructions.push(IRInstruction::StoreLocal(slot));

        let value_kind = match value_result.kind {
            ValueKind::Any if cloned_from_existing => ValueKind::String,
            other => other,
        };
        context.set_variable_type(var_name, value_kind);

        // Mark variable as heap-allocated if needed
        if value_result.heap_ownership == HeapOwnership::Owned || cloned_from_existing {
            context.mark_heap_allocated(var_name);
        }

        added_variables.push(var_name.clone());
        binding_infos.push(BindingInfo {
            slot,
            owns_heap: value_result.heap_ownership == HeapOwnership::Owned || cloned_from_existing,
        });
    }

    let mut body_result = crate::compiler::compile_node(&args[1], context, program)?;
    let mut body_kind = body_result.kind;

    if let Node::Symbol { value } = &args[1] {
        if added_variables.iter().any(|name| name == value) && crate::compiler::is_heap_allocated_symbol(value, context) {
            body_result.instructions.push(IRInstruction::RuntimeCall("_string_clone".to_string(), 1));
            body_result.heap_ownership = HeapOwnership::Owned;
            body_kind = ValueKind::String;
        }
    }

    let mut body_instructions = body_result.instructions;

    let mut freed_on_all_paths: HashSet<usize> = HashSet::new();

    let tracked_slots_for_plan: HashSet<usize> = binding_infos.iter().filter(|info| info.owns_heap).map(|info| info.slot).collect();

    if !tracked_slots_for_plan.is_empty() {
        let plan = compute_liveness_plan(&body_instructions, &tracked_slots_for_plan);
        if !plan.insert_after.is_empty() {
            body_instructions = apply_liveness_plan(body_instructions, &plan);
        }
        freed_on_all_paths.extend(plan.freed_everywhere.iter().copied());
    }

    instructions.extend(body_instructions);

    // Fallback to scope-exit frees for any owned locals not handled by liveness or immediate free.
    for info in binding_infos.iter().filter(|info| info.owns_heap) {
        if freed_on_all_paths.contains(&info.slot) {
            continue;
        }
        instructions.push(IRInstruction::FreeLocal(info.slot));
    }

    context.remove_variables(&added_variables);

    Ok(CompileResult::with_instructions(instructions, body_kind).with_heap_ownership(body_result.heap_ownership))
}
