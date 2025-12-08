use super::{
    builtins::{emit_free_for_slot, free_retained_dependents, free_retained_slot},
    extend_with_offset,
    CompileContext,
    CompileError,
    CompileResult,
    HeapOwnership,
    RetainedSlot,
    ValueKind,
};
/// Variable binding compilation (let expressions)
use crate::ast::Node;
use crate::compiler::liveness::{apply_liveness_plan, compute_liveness_plan};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
struct BindingInfo {
    slot: usize,
    owns_heap: bool,
    kind: ValueKind,
    retained_slots: Vec<RetainedSlot>,
}

struct BindingCollection {
    instructions: Vec<IRInstruction>,
    added_variables: Vec<String>,
    binding_infos: Vec<BindingInfo>,
}

impl BindingCollection {
    fn new() -> Self {
        Self {
            instructions: Vec::new(),
            added_variables: Vec::new(),
            binding_infos: Vec::new(),
        }
    }
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

    let mut collected = collect_bindings(bindings, context, program)?;
    let mut instructions = std::mem::take(&mut collected.instructions);
    let added_variables = std::mem::take(&mut collected.added_variables);
    let mut binding_infos = std::mem::take(&mut collected.binding_infos);

    let mut body_result = crate::compiler::compile_node(&args[1], context, program)?;
    let mut body_kind = body_result.kind;
    let mut body_heap_ownership = body_result.heap_ownership;
    let mut body_instructions = std::mem::take(&mut body_result.instructions);
    let body_retained_slots = body_result.take_retained_slots();

    apply_body_symbol_clone(&args[1], &added_variables, context, &mut body_instructions, &mut body_kind, &mut body_heap_ownership);

    let mut slot_kinds_for_plan: HashMap<usize, ValueKind> = HashMap::new();
    let mut tracked_slots_for_plan: HashSet<usize> = HashSet::new();
    collect_slot_tracking(&binding_infos, &mut tracked_slots_for_plan, &mut slot_kinds_for_plan);

    let freed_on_all_paths = plan_liveness(&mut body_instructions, &tracked_slots_for_plan, &slot_kinds_for_plan);

    extend_with_offset(&mut instructions, body_instructions);

    emit_scope_cleanup(&mut instructions, &mut binding_infos, &freed_on_all_paths, context);

    context.remove_variables(&added_variables);

    Ok(CompileResult::with_instructions(instructions, body_kind)
        .with_heap_ownership(body_heap_ownership)
        .with_retained_slots(body_retained_slots))
}

fn collect_bindings(bindings: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<BindingCollection, CompileError> {
    let mut collected = BindingCollection::new();

    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        let var_name = match var_node {
            Node::Symbol { value } => value,
            _ => return Err(CompileError::InvalidExpression("let binding variables must be symbols".to_string())),
        };

        let mut value_result = crate::compiler::compile_node(val_node, context, program)?;
        let mut cloned_map_value_types = None;

        let mut cloned_from_existing: Option<ValueKind> = None;
        if let Node::Symbol { value } = val_node {
            if crate::compiler::is_heap_allocated_symbol(value, context) {
                let source_kind = context.get_variable_type(value).or_else(|| context.get_parameter_type(value)).unwrap_or(ValueKind::String);
                let runtime = match source_kind {
                    ValueKind::Vector => "_vector_clone",
                    ValueKind::Map => "_map_clone",
                    ValueKind::Set => "_set_clone",
                    _ => "_string_clone",
                };
                value_result.instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
                value_result.heap_ownership = HeapOwnership::Owned;
                cloned_from_existing = Some(source_kind);
                if source_kind == ValueKind::Map {
                    cloned_map_value_types = context.get_variable_map_value_types(value).cloned();
                }
            }
        }

        let value_map_value_types = value_result.map_value_types.clone();
        let retained_slots = value_result.take_retained_slots();
        extend_with_offset(&mut collected.instructions, value_result.instructions);

        let slot = context.add_variable(var_name.clone());
        collected.instructions.push(IRInstruction::StoreLocal(slot));

        let mut value_kind = match value_result.kind {
            ValueKind::Any => cloned_from_existing.unwrap_or(ValueKind::Any),
            other => other,
        };
        let mut inferred_heap_ownership = None;
        let mut inferred_map_value_types = None;
        let mut inferred_set_element_kind = None;
        let mut inferred_vector_element_kind = None;
        if let Some((inferred_kind, inferred_owner, inferred_map_types, set_element_kind, vector_element_kind)) =
            context.consume_local_binding_metadata(var_name)
        {
            if inferred_kind != ValueKind::Any {
                value_kind = inferred_kind;
            }
            inferred_heap_ownership = Some(inferred_owner);
            inferred_map_value_types = inferred_map_types;
            inferred_set_element_kind = set_element_kind;
            inferred_vector_element_kind = vector_element_kind;
        }
        context.set_variable_type(var_name, value_kind);
        if value_kind == ValueKind::Map {
            let mut combined = value_map_value_types.or(cloned_map_value_types);
            if let Some(mut inferred) = inferred_map_value_types.filter(|m| !m.is_empty()) {
                if let Some(existing) = combined.as_mut() {
                    for (k, v) in inferred.drain() {
                        existing.insert(k, v);
                    }
                } else {
                    combined = Some(inferred);
                }
            }
            context.set_variable_map_value_types(var_name, combined);
        } else {
            context.set_variable_map_value_types(var_name, None);
        }
        let set_element_kind = value_result.set_element_kind.or(inferred_set_element_kind);
        let vector_element_kind = value_result.vector_element_kind.or(inferred_vector_element_kind);
        context.set_variable_set_element_kind(var_name, set_element_kind);
        context.set_variable_vector_element_kind(var_name, vector_element_kind);
        context.set_variable_set_element_kind(var_name, inferred_set_element_kind);
        context.set_variable_vector_element_kind(var_name, inferred_vector_element_kind);

        // Mark variable as heap-allocated if needed
        let heap_owned = value_result.heap_ownership == HeapOwnership::Owned || cloned_from_existing.is_some() || matches!(inferred_heap_ownership, Some(HeapOwnership::Owned));
        if heap_owned {
            context.mark_heap_allocated(var_name, value_kind);
        }

        collected.added_variables.push(var_name.clone());
        collected.binding_infos.push(BindingInfo {
            slot,
            owns_heap: heap_owned,
            kind: value_kind,
            retained_slots,
        });
    }

    Ok(collected)
}

fn apply_body_symbol_clone(
    body_node: &Node,
    added_variables: &[String],
    context: &mut CompileContext,
    body_instructions: &mut Vec<IRInstruction>,
    body_kind: &mut ValueKind,
    body_heap_ownership: &mut HeapOwnership,
) {
    if let Node::Symbol { value } = body_node {
        if added_variables.iter().any(|name| name == value) && crate::compiler::is_heap_allocated_symbol(value, context) {
            let symbol_kind = context.get_variable_type(value).unwrap_or(ValueKind::String);
            let runtime = match symbol_kind {
                ValueKind::Vector => "_vector_clone",
                ValueKind::Map => "_map_clone",
                ValueKind::Set => "_set_clone",
                _ => "_string_clone",
            };
            body_instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
            *body_heap_ownership = HeapOwnership::Owned;
            *body_kind = symbol_kind;
        }
    }
}
fn collect_slot_tracking(binding_infos: &[BindingInfo], tracked: &mut HashSet<usize>, slot_kinds: &mut HashMap<usize, ValueKind>) {
    for info in binding_infos {
        if info.owns_heap {
            tracked.insert(info.slot);
            slot_kinds.insert(info.slot, info.kind);
        }
        for retained in &info.retained_slots {
            tracked.insert(retained.slot);
            slot_kinds.insert(retained.slot, retained.kind);
        }
    }
}

fn plan_liveness(body_instructions: &mut Vec<IRInstruction>, tracked: &HashSet<usize>, slot_kinds: &HashMap<usize, ValueKind>) -> HashSet<usize> {
    let mut freed_on_all_paths = HashSet::new();
    if tracked.is_empty() {
        return freed_on_all_paths;
    }

    let plan = compute_liveness_plan(body_instructions, tracked);
    if !plan.insert_after.is_empty() {
        *body_instructions = apply_liveness_plan(std::mem::take(body_instructions), &plan, |insts, slot| {
            let kind = slot_kinds.get(&slot).copied().unwrap_or(ValueKind::Any);
            emit_free_for_slot(insts, slot, kind);
        });
    }
    freed_on_all_paths.extend(plan.freed_everywhere.iter().copied());
    #[cfg(debug_assertions)]
    if std::env::var("SLISP_DEBUG_LET").is_ok() {
        eprintln!("[slisp:debug_let] freed_on_all_paths={:?}", plan.freed_everywhere);
    }

    freed_on_all_paths
}

fn emit_scope_cleanup(instructions: &mut Vec<IRInstruction>, binding_infos: &mut [BindingInfo], freed_on_all_paths: &HashSet<usize>, context: &mut CompileContext) {
    for info in binding_infos.iter().filter(|info| info.owns_heap) {
        if freed_on_all_paths.contains(&info.slot) {
            continue;
        }
        emit_free_for_slot(instructions, info.slot, info.kind);
    }

    for info in binding_infos {
        for slot in &mut info.retained_slots {
            if freed_on_all_paths.contains(&slot.slot) {
                free_retained_dependents(slot, instructions, context);
            } else {
                free_retained_slot(slot.clone(), instructions, context);
            }
        }
    }
}
