use super::{
    compile_node, extend_with_offset, is_heap_allocated_symbol,
    liveness::{apply_liveness_plan, compute_liveness_plan, LivenessPlan},
    slots::SlotTracker,
    CompileContext, CompileError, CompileResult, HeapOwnership, MapKeyLiteral, MapValueTypes, RetainedSlot, ValueKind,
};
use crate::ast::{Node, Primitive};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

pub(super) fn compile_vector_literal(elements: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if elements.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_vector_create".to_string(), 3),
            ],
            ValueKind::Vector,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = elements.len();
    let mut instructions = Vec::new();
    let value_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_value_slots = value_slots.clone();
    ordered_value_slots.sort_unstable();
    ordered_value_slots.reverse();

    let tag_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_tag_slots = tag_slots.clone();
    ordered_tag_slots.sort_unstable();
    ordered_tag_slots.reverse();

    let mut retained_slots: Vec<RetainedSlot> = Vec::new();
    let mut element_kind_accumulator: Option<ValueKind> = None;

    for idx in 0..count {
        let element = &elements[idx];
        let value_slot = ordered_value_slots[idx];
        let tag_slot = ordered_tag_slots[idx];

        let mut element_result = compile_node(element, context, program)?;
        let element_instructions = std::mem::take(&mut element_result.instructions);
        extend_with_offset(&mut instructions, element_instructions);

        let mut element_kind = element_result.kind;
        if element_kind == ValueKind::Any {
            if let Node::Symbol { value } = element {
                if let Some(var_kind) = context.get_variable_type(value) {
                    element_kind = var_kind;
                } else if let Some(param_kind) = context.get_parameter_type(value) {
                    element_kind = param_kind;
                }
            }
        }

        ensure_owned_on_stack(&mut instructions, element_kind, &mut element_result.heap_ownership);
        if element_kind != ValueKind::Any {
            if let Some(existing) = element_kind_accumulator {
                if existing != element_kind {
                    element_kind_accumulator = Some(ValueKind::Any);
                }
            } else {
                element_kind_accumulator = Some(element_kind);
            }
        }
        let element_dependents = element_result.take_retained_slots();
        instructions.push(IRInstruction::StoreLocal(value_slot));
        track_heap_slot(&mut retained_slots, value_slot, element_kind, None, element_dependents);

        instructions.push(IRInstruction::Push(element_kind.runtime_tag()));
        instructions.push(IRInstruction::StoreLocal(tag_slot));
        element_result.free_retained_slots(&mut instructions, context);
    }

    let values_base = ordered_value_slots[0];
    let tags_base = ordered_tag_slots[0];
    instructions.push(IRInstruction::PushLocalAddress(values_base));
    instructions.push(IRInstruction::PushLocalAddress(tags_base));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_vector_create".to_string(), 3));

    dedup_retained_slots(&mut retained_slots);

    value_slots.into_iter().filter(|slot| !retains_slot(&retained_slots, *slot)).for_each(|slot| {
        context.release_temp_slot(slot);
    });
    tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Vector)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_vector_element_kind(element_kind_accumulator)
        .with_retained_slots(retained_slots))
}

pub(super) fn compile_set_literal(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_set_create".to_string(), 3),
            ],
            ValueKind::Set,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = args.len();
    let mut instructions = Vec::new();

    let value_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_value_slots = value_slots.clone();
    ordered_value_slots.sort_unstable();
    ordered_value_slots.reverse();

    let tag_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_tag_slots = tag_slots.clone();
    ordered_tag_slots.sort_unstable();
    ordered_tag_slots.reverse();

    let mut retained_slots: Vec<RetainedSlot> = Vec::new();
    let mut element_kind_accumulator: Option<ValueKind> = None;

    for idx in 0..count {
        let value_node = &args[idx];
        let value_slot = ordered_value_slots[idx];
        let tag_slot = ordered_tag_slots[idx];

        let value_literal = literal_map_key(value_node);
        let mut value_result = compile_node(value_node, context, program)?;
        let value_kind = resolve_map_key_kind(value_node, value_result.kind, context)?;
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
        if value_kind != ValueKind::Any {
            if let Some(existing) = element_kind_accumulator {
                if existing != value_kind {
                    element_kind_accumulator = Some(ValueKind::Any);
                }
            } else {
                element_kind_accumulator = Some(value_kind);
            }
        }
        let value_dependents = value_result.take_retained_slots();
        instructions.push(IRInstruction::StoreLocal(value_slot));
        track_heap_slot(&mut retained_slots, value_slot, value_kind, value_literal.clone(), value_dependents);

        instructions.push(IRInstruction::Push(runtime_tag_for_key(value_kind)));
        instructions.push(IRInstruction::StoreLocal(tag_slot));
        value_result.free_retained_slots(&mut instructions, context);
    }

    instructions.push(IRInstruction::PushLocalAddress(ordered_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_tag_slots[0]));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_set_create".to_string(), 3));

    dedup_retained_slots(&mut retained_slots);

    value_slots.into_iter().filter(|slot| !retains_slot(&retained_slots, *slot)).for_each(|slot| {
        context.release_temp_slot(slot);
    });
    tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_set_element_kind(element_kind_accumulator)
        .with_retained_slots(retained_slots))
}

/// Compile count operation (string length)
pub(super) fn compile_count(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("count".to_string(), 1, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracker = SlotTracker::new();

    if let Some(slot) = tracker.track_if_owned(&mut instructions, context, arg_result.heap_ownership, ValueKind::Any) {
        let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);
        tracker.set_slot_kind(slot, target_kind);
    }

    let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);

    let runtime = match target_kind {
        ValueKind::Vector => "_vector_count",
        ValueKind::Map => "_map_count",
        ValueKind::Set => "_set_count",
        _ => "_string_count",
    };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));

    instructions = tracker.apply_liveness_and_release(instructions, context);
    arg_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Number))
}

/// Compile get operation (string indexing)
fn resolve_value_kind(node: &Node, initial: ValueKind, context: &CompileContext) -> ValueKind {
    if initial != ValueKind::Any {
        return initial;
    }

    match node {
        Node::Symbol { value } => context.get_variable_type(value).or_else(|| context.get_parameter_type(value)).unwrap_or(initial),
        _ => initial,
    }
}

fn resolve_map_key_kind(node: &Node, initial: ValueKind, context: &CompileContext) -> Result<ValueKind, CompileError> {
    let resolved = resolve_value_kind(node, initial, context);
    match resolved {
        ValueKind::Number | ValueKind::Boolean | ValueKind::String | ValueKind::Keyword | ValueKind::Nil => Ok(resolved),
        ValueKind::Any => Err(CompileError::InvalidExpression("map keys must have a concrete type".to_string())),
        _ => Err(CompileError::InvalidExpression("map keys must be numbers, booleans, strings, keywords, or nil".to_string())),
    }
}

fn runtime_tag_for_key(kind: ValueKind) -> i64 {
    kind.runtime_tag()
}

fn literal_map_key_from_primitive(value: &Primitive) -> Option<MapKeyLiteral> {
    match value {
        Primitive::String(inner) => Some(MapKeyLiteral::String(inner.clone())),
        Primitive::Keyword(inner) => Some(MapKeyLiteral::Keyword(inner.clone())),
        Primitive::Number(num) => Some(MapKeyLiteral::Number(*num as i64)),
        Primitive::Boolean(flag) => Some(MapKeyLiteral::Boolean(*flag)),
    }
}

fn literal_map_key(node: &Node) -> Option<MapKeyLiteral> {
    match node {
        Node::Primitive { value } => literal_map_key_from_primitive(value),
        Node::Symbol { value } if value == "nil" => Some(MapKeyLiteral::Nil),
        _ => None,
    }
}

fn runtime_tag_for_value(kind: ValueKind) -> i64 {
    kind.runtime_tag()
}

pub(super) fn clone_runtime_for_kind(kind: ValueKind) -> Option<&'static str> {
    match kind {
        ValueKind::String => Some("_string_clone"),
        ValueKind::Vector => Some("_vector_clone"),
        ValueKind::Map => Some("_map_clone"),
        ValueKind::Set => Some("_set_clone"),
        _ => None,
    }
}

pub(super) fn runtime_free_for_kind(kind: ValueKind) -> Option<&'static str> {
    match kind {
        ValueKind::Vector => Some("_vector_free"),
        ValueKind::Map => Some("_map_free"),
        ValueKind::Set => Some("_set_free"),
        _ => None,
    }
}

pub(super) fn emit_free_for_slot(instructions: &mut Vec<IRInstruction>, slot: usize, kind: ValueKind) {
    if let Some(runtime) = runtime_free_for_kind(kind) {
        instructions.push(IRInstruction::FreeLocalWithRuntime(slot, runtime.to_string()));
    } else {
        instructions.push(IRInstruction::FreeLocal(slot));
    }
}

pub(super) fn free_retained_slot(slot: RetainedSlot, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    slot.dependents.into_iter().for_each(|dependent| {
        free_retained_slot(dependent, instructions, context);
    });
    emit_free_for_slot(instructions, slot.slot, slot.kind);
    context.release_temp_slot(slot.slot);
}

pub(super) fn free_retained_dependents(slot: &mut RetainedSlot, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    slot.dependents.drain(..).for_each(|dependent| {
        free_retained_slot(dependent, instructions, context);
    });
}

fn apply_plan_with_slot_kinds(instructions: Vec<IRInstruction>, plan: &LivenessPlan, slot_kinds: &HashMap<usize, ValueKind>) -> Vec<IRInstruction> {
    apply_liveness_plan(instructions, plan, |insts, slot| {
        let kind = slot_kinds.get(&slot).copied().unwrap_or(ValueKind::Any);
        emit_free_for_slot(insts, slot, kind);
    })
}

fn ensure_owned_on_stack(instructions: &mut Vec<IRInstruction>, kind: ValueKind, ownership: &mut HeapOwnership) {
    if *ownership == HeapOwnership::Owned {
        return;
    }

    if let Some(runtime) = clone_runtime_for_kind(kind) {
        instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
        *ownership = HeapOwnership::Owned;
    }
}

fn track_heap_slot(retained_slots: &mut Vec<RetainedSlot>, slot: usize, kind: ValueKind, key: Option<MapKeyLiteral>, dependents: Vec<RetainedSlot>) {
    if kind.is_heap_kind() {
        retained_slots.push(RetainedSlot { slot, key, kind, dependents });
    }
}

fn release_slots_for_literal(retained_slots: &mut Vec<RetainedSlot>, literal: &MapKeyLiteral, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    let (to_free, kept): (Vec<_>, Vec<_>) = retained_slots.drain(..).partition(|slot| slot.key.as_ref() == Some(literal));
    *retained_slots = kept;
    for slot_info in to_free {
        free_retained_slot(slot_info, instructions, context);
    }
}

fn take_slots_for_literal(retained_slots: &mut Vec<RetainedSlot>, literal: &MapKeyLiteral) -> Vec<RetainedSlot> {
    let (removed, kept): (Vec<_>, Vec<_>) = retained_slots.drain(..).partition(|slot| slot.key.as_ref() == Some(literal));
    *retained_slots = kept;
    removed
}

fn dedup_retained_slots(slots: &mut Vec<RetainedSlot>) {
    if slots.is_empty() {
        return;
    }
    slots.sort_by_key(|info| info.slot);
    slots.dedup_by(|a, b| {
        if a.slot == b.slot {
            a.dependents.extend(b.dependents.drain(..));
            true
        } else {
            false
        }
    });
}

fn retains_slot(slots: &[RetainedSlot], slot: usize) -> bool {
    slots.iter().any(|info| info.slot == slot)
}

struct DefaultValue {
    slot: usize,
    owned: bool,
    kind: ValueKind,
    retained_slots: Vec<RetainedSlot>,
}

enum DefaultHandling {
    None,
    Some(DefaultValue),
}

impl DefaultHandling {
    fn from_parts(slot: Option<usize>, owned: bool, kind: ValueKind, mut retained_slots: Vec<RetainedSlot>) -> Self {
        dedup_retained_slots(&mut retained_slots);
        match slot {
            Some(slot) => DefaultHandling::Some(DefaultValue { slot, owned, kind, retained_slots }),
            None => DefaultHandling::None,
        }
    }

    fn has_value(&self) -> bool {
        matches!(self, DefaultHandling::Some(_))
    }

    fn success_cleanup(&mut self, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
        if let DefaultHandling::Some(default) = self {
            if default.owned {
                emit_free_for_slot(instructions, default.slot, default.kind);
            }
            default.retained_slots.drain(..).for_each(|slot| {
                free_retained_slot(slot, instructions, context);
            });
        }
    }

    fn emit_fallback(&self, instructions: &mut Vec<IRInstruction>) {
        match self {
            DefaultHandling::Some(default) => {
                instructions.push(IRInstruction::LoadLocal(default.slot));
                if let Some(runtime) = clone_runtime_for_kind(default.kind) {
                    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));
                    if default.owned {
                        emit_free_for_slot(instructions, default.slot, default.kind);
                    }
                } else if default.owned {
                    emit_free_for_slot(instructions, default.slot, default.kind);
                }
            }
            DefaultHandling::None => instructions.push(IRInstruction::Push(0)),
        }
    }

    fn release_slot(&self, context: &mut CompileContext) {
        if let DefaultHandling::Some(default) = self {
            context.release_temp_slot(default.slot);
        }
    }

    fn inferred_kind(&self) -> Option<ValueKind> {
        match self {
            DefaultHandling::Some(default) => Some(default.kind),
            DefaultHandling::None => None,
        }
    }

    fn take_retained_slots(&mut self) -> Vec<RetainedSlot> {
        match self {
            DefaultHandling::Some(default) => std::mem::take(&mut default.retained_slots),
            DefaultHandling::None => Vec::new(),
        }
    }
}

fn emit_vector_get(
    instructions: &mut Vec<IRInstruction>,
    context: &mut CompileContext,
    tracked_slots: &mut HashSet<usize>,
    slot_kinds: &mut HashMap<usize, ValueKind>,
    owned_arg_slot: Option<usize>,
    default: &mut DefaultHandling,
) {
    if let Some(slot) = owned_arg_slot {
        tracked_slots.remove(&slot);
        slot_kinds.remove(&slot);
    }

    let out_slot = context.allocate_temp_slot();
    instructions.push(IRInstruction::Push(0));
    instructions.push(IRInstruction::StoreLocal(out_slot));
    instructions.push(IRInstruction::PushLocalAddress(out_slot));
    instructions.push(IRInstruction::RuntimeCall("_vector_get".to_string(), 3));

    let failure_jump_pos = instructions.len();
    instructions.push(IRInstruction::JumpIfZero(0));

    instructions.push(IRInstruction::LoadLocal(out_slot));
    default.success_cleanup(instructions, context);
    let success_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let failure_block_pos = instructions.len();
    instructions[failure_jump_pos] = IRInstruction::JumpIfZero(failure_block_pos);

    default.emit_fallback(instructions);

    let end_pos = instructions.len();
    instructions[success_jump_pos] = IRInstruction::Jump(end_pos);

    context.release_temp_slot(out_slot);
}

fn emit_string_get(instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, default: &mut DefaultHandling) {
    instructions.push(IRInstruction::RuntimeCall("_string_get".to_string(), 2));

    let result_slot = context.allocate_temp_slot();
    instructions.push(IRInstruction::StoreLocal(result_slot));
    instructions.push(IRInstruction::LoadLocal(result_slot));
    let fallback_jump_pos = instructions.len();
    instructions.push(IRInstruction::JumpIfZero(0));

    instructions.push(IRInstruction::LoadLocal(result_slot));
    default.success_cleanup(instructions, context);
    let success_jump_pos = instructions.len();
    instructions.push(IRInstruction::Jump(0));

    let fallback_block_pos = instructions.len();
    instructions[fallback_jump_pos] = IRInstruction::JumpIfZero(fallback_block_pos);

    default.emit_fallback(instructions);

    let end_pos = instructions.len();
    instructions[success_jump_pos] = IRInstruction::Jump(end_pos);

    context.release_temp_slot(result_slot);
}

pub(super) fn compile_get(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("get".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let target_map_value_types = target_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut owned_arg_slot: Option<usize> = None;
    let mut owned_key_slot: Option<usize> = None;

    if target_result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Any);
        temp_slots.push(slot);
        owned_arg_slot = Some(slot);
    }

    let mut key_result = compile_node(&args[1], context, program)?;
    let key_ownership = key_result.heap_ownership;
    let mut key_kind = key_result.kind;
    let key_instructions = std::mem::take(&mut key_result.instructions);
    extend_with_offset(&mut instructions, key_instructions);
    key_result.free_retained_slots(&mut instructions, context);

    let mut default_slot = None;
    let mut default_owned = false;
    let mut default_kind = ValueKind::Any;
    let mut default_retained_slots = Vec::new();

    if args.len() == 3 {
        let mut default_result = compile_node(&args[2], context, program)?;
        default_kind = resolve_value_kind(&args[2], default_result.kind, context);
        default_retained_slots = default_result.take_retained_slots();
        extend_with_offset(&mut instructions, default_result.instructions);
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        default_owned = default_result.heap_ownership == HeapOwnership::Owned;
        default_slot = Some(slot);
    }

    let mut default_handling = DefaultHandling::from_parts(default_slot, default_owned, default_kind, default_retained_slots);
    let target_kind = resolve_value_kind(&args[0], target_result.kind, context);

    if let Some(slot) = owned_arg_slot {
        slot_kinds.insert(slot, target_kind);
    }

    match target_kind {
        ValueKind::Vector => {
            emit_vector_get(&mut instructions, context, &mut tracked_slots, &mut slot_kinds, owned_arg_slot, &mut default_handling);
        }
        ValueKind::Map => {
            if key_ownership == HeapOwnership::Owned {
                let slot = context.allocate_temp_slot();
                instructions.push(IRInstruction::StoreLocal(slot));
                instructions.push(IRInstruction::LoadLocal(slot));
                tracked_slots.insert(slot);
                slot_kinds.insert(slot, ValueKind::Any);
                temp_slots.push(slot);
                owned_key_slot = Some(slot);
            }

            key_kind = resolve_map_key_kind(&args[1], key_kind, context)?;
            if let Some(slot) = owned_key_slot {
                slot_kinds.insert(slot, key_kind);
            }
            let key_tag = runtime_tag_for_key(key_kind);
            instructions.push(IRInstruction::Push(key_tag));

            let value_slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::Push(0));
            instructions.push(IRInstruction::StoreLocal(value_slot));
            instructions.push(IRInstruction::PushLocalAddress(value_slot));
            temp_slots.push(value_slot);

            let tag_slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::Push(0));
            instructions.push(IRInstruction::StoreLocal(tag_slot));
            instructions.push(IRInstruction::PushLocalAddress(tag_slot));
            temp_slots.push(tag_slot);

            instructions.push(IRInstruction::RuntimeCall("_map_get".to_string(), 5));

            let failure_jump_pos = instructions.len();
            instructions.push(IRInstruction::JumpIfZero(0));

            let inferred_value_kind = target_map_value_types.as_ref().and_then(|types| literal_map_key(&args[1]).and_then(|key| types.get(&key).copied()));
            let needs_clone = inferred_value_kind.map(|kind| kind.is_heap_kind()).unwrap_or(true);
            if needs_clone {
                instructions.push(IRInstruction::LoadLocal(value_slot));
                instructions.push(IRInstruction::LoadLocal(tag_slot));
                instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
                instructions.push(IRInstruction::StoreLocal(value_slot));
            }

            instructions.push(IRInstruction::LoadLocal(value_slot));
            default_handling.success_cleanup(&mut instructions, context);
            let success_jump_pos = instructions.len();
            instructions.push(IRInstruction::Jump(0));

            let failure_block_pos = instructions.len();
            instructions[failure_jump_pos] = IRInstruction::JumpIfZero(failure_block_pos);

            default_handling.emit_fallback(&mut instructions);

            let end_pos = instructions.len();
            instructions[success_jump_pos] = IRInstruction::Jump(end_pos);
        }
        _ => {
            emit_string_get(&mut instructions, context, &mut default_handling);
        }
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    default_handling.release_slot(context);
    target_result.free_retained_slots(&mut instructions, context);

    let inferred_map_value_kind = target_map_value_types.as_ref().and_then(|types| literal_map_key(&args[1]).and_then(|key| types.get(&key).copied()));

    let result_kind = match target_kind {
        ValueKind::Vector => default_handling.inferred_kind().unwrap_or(ValueKind::Any),
        ValueKind::Map => inferred_map_value_kind.or_else(|| default_handling.inferred_kind()).unwrap_or(ValueKind::Any),
        _ if default_handling.has_value() => default_handling.inferred_kind().unwrap_or(ValueKind::String),
        _ => ValueKind::String,
    };

    let map_needs_clone_flag = match target_kind {
        ValueKind::Map => inferred_map_value_kind.map(|kind| kind.is_heap_kind()).unwrap_or(true),
        _ => false,
    };

    let heap_ownership = match target_kind {
        ValueKind::Vector => HeapOwnership::None,
        ValueKind::Map => match (inferred_map_value_kind, target_result.heap_ownership) {
            (Some(kind), HeapOwnership::Owned) if kind.is_heap_kind() => HeapOwnership::Owned,
            (Some(kind), _) if kind.is_heap_kind() => HeapOwnership::Borrowed,
            _ if result_kind.is_heap_clone_kind() || map_needs_clone_flag => HeapOwnership::Owned,
            _ => HeapOwnership::None,
        },
        _ => HeapOwnership::Owned,
    };

    let retained_slots = default_handling.take_retained_slots();

    Ok(CompileResult::with_instructions(instructions, result_kind)
        .with_heap_ownership(heap_ownership)
        .with_retained_slots(retained_slots))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Node, Primitive};

    #[test]
    fn map_literal_carries_value_metadata() {
        let mut context = CompileContext::new();
        let mut program = IRProgram::new();
        let args = vec![
            Node::Primitive {
                value: Primitive::Keyword("nums".to_string()),
            },
            Node::Set {
                root: vec![Node::Primitive { value: Primitive::Number(1) }, Node::Primitive { value: Primitive::Number(2) }],
            },
            Node::Primitive {
                value: Primitive::Keyword("letters".to_string()),
            },
            Node::Vector {
                root: vec![
                    Node::Primitive {
                        value: Primitive::String("a".to_string()),
                    },
                    Node::Primitive {
                        value: Primitive::String("b".to_string()),
                    },
                ],
            },
        ];

        let result = compile_hash_map(&args, &mut context, &mut program).unwrap();
        let metadata = result.map_value_types.expect("expected map metadata");
        assert_eq!(metadata.get(&MapKeyLiteral::Keyword("nums".to_string())), Some(&ValueKind::Set));
        assert_eq!(metadata.get(&MapKeyLiteral::Keyword("letters".to_string())), Some(&ValueKind::Vector));
    }

    #[test]
    fn compile_get_uses_map_metadata_for_literals() {
        let mut context = CompileContext::new();
        let slot = context.add_variable("m".to_string());
        context.set_variable_type("m", ValueKind::Map);
        context.mark_heap_allocated("m", ValueKind::Map);
        let mut metadata = MapValueTypes::new();
        metadata.insert(MapKeyLiteral::String("a".to_string()), ValueKind::String);
        context.set_variable_map_value_types("m", Some(metadata));
        let mut program = IRProgram::new();
        let args = vec![
            Node::Symbol { value: "m".to_string() },
            Node::Primitive {
                value: Primitive::String("a".to_string()),
            },
        ];
        let result = compile_get(&args, &mut context, &mut program).unwrap();
        assert_eq!(result.kind, ValueKind::String);
        assert_eq!(result.heap_ownership, HeapOwnership::Borrowed);
        assert!(context.get_variable("m").is_some());
        // ensure slot still allocated
        assert_eq!(slot, 0);
    }
}
/// Compile subs operation (substring extraction)
pub(super) fn compile_subs(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("subs".to_string(), 2, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracker = SlotTracker::new();

    let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);

    if let Some(slot) = tracker.track_if_owned(&mut instructions, context, arg_result.heap_ownership, ValueKind::Any) {
        tracker.set_slot_kind(slot, target_kind);
    }

    let mut start_result = compile_node(&args[1], context, program)?;
    let start_instructions = std::mem::take(&mut start_result.instructions);
    extend_with_offset(&mut instructions, start_instructions);
    start_result.free_retained_slots(&mut instructions, context);

    if args.len() == 3 {
        let mut end_result = compile_node(&args[2], context, program)?;
        let end_instructions = std::mem::take(&mut end_result.instructions);
        extend_with_offset(&mut instructions, end_instructions);
        end_result.free_retained_slots(&mut instructions, context);
    } else {
        instructions.push(IRInstruction::Push(-1));
    }

    let runtime = if target_kind == ValueKind::Vector { "_vector_slice" } else { "_string_subs" };

    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));

    instructions = tracker.apply_liveness_and_release(instructions, context);
    arg_result.free_retained_slots(&mut instructions, context);

    let result_kind = if target_kind == ValueKind::Vector { ValueKind::Vector } else { ValueKind::String };

    Ok(CompileResult::with_instructions(instructions, result_kind).with_heap_ownership(HeapOwnership::Owned))
}

/// Compile str operation (string concatenation)
pub(super) fn compile_str(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Ok(CompileResult::with_instructions(
            vec![IRInstruction::Push(0), IRInstruction::Push(0), IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2)],
            ValueKind::String,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let count = args.len();
    let mut instructions = Vec::new();
    let temp_slots = context.allocate_contiguous_temp_slots(count);
    let mut ordered_slots = temp_slots.clone();
    ordered_slots.sort_unstable();
    ordered_slots.reverse();

    let mut needs_free = Vec::with_capacity(count);

    for (arg, slot) in args.iter().zip(ordered_slots.iter()) {
        let mut arg_result = compile_node(arg, context, program)?;
        let arg_instructions = std::mem::take(&mut arg_result.instructions);
        extend_with_offset(&mut instructions, arg_instructions);

        let mut slot_needs_free = arg_result.heap_ownership == HeapOwnership::Owned;

        let mut arg_kind = arg_result.kind;
        if arg_kind == ValueKind::Any {
            if let Node::Symbol { value } = arg {
                if let Some(var_kind) = context.get_variable_type(value).or_else(|| context.get_parameter_type(value)) {
                    if var_kind != ValueKind::Any {
                        arg_kind = var_kind;
                    } else if context.get_parameter(value).is_some() {
                        context.mark_heap_allocated(value, ValueKind::String);
                        arg_kind = ValueKind::String;
                    }
                } else if context.get_parameter(value).is_some() {
                    context.mark_heap_allocated(value, ValueKind::String);
                    arg_kind = ValueKind::String;
                }
            }
        }

        match arg_kind {
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
            ValueKind::Keyword => {
                instructions.push(IRInstruction::Push(0));
                instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
                slot_needs_free = false;
            }
            ValueKind::Nil => {
                instructions.push(IRInstruction::Push(0));
                instructions.push(IRInstruction::RuntimeCall("_string_normalize".to_string(), 2));
                slot_needs_free = false;
            }
            ValueKind::Vector => {
                instructions.push(IRInstruction::RuntimeCall("_vector_to_string".to_string(), 1));
                slot_needs_free = true;
            }
            ValueKind::Map => {
                instructions.push(IRInstruction::RuntimeCall("_map_to_string".to_string(), 1));
                slot_needs_free = true;
            }
            ValueKind::Set => {
                instructions.push(IRInstruction::RuntimeCall("_set_to_string".to_string(), 1));
                slot_needs_free = true;
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

        instructions.push(IRInstruction::StoreLocal(*slot));
        needs_free.push(slot_needs_free);
        arg_result.free_retained_slots(&mut instructions, context);
    }

    let base_slot = ordered_slots[0];
    instructions.push(IRInstruction::PushLocalAddress(base_slot));
    instructions.push(IRInstruction::Push(count as i64));
    instructions.push(IRInstruction::RuntimeCall("_string_concat_n".to_string(), 2));

    ordered_slots.iter().zip(needs_free.iter()).filter(|(_, free)| **free).for_each(|(slot, _)| {
        instructions.push(IRInstruction::FreeLocal(*slot));
    });

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}

pub(super) fn compile_hash_map(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() % 2 != 0 {
        return Err(CompileError::InvalidExpression("hash-map requires key/value pairs".to_string()));
    }

    let pair_count = args.len() / 2;
    if pair_count == 0 {
        return Ok(CompileResult::with_instructions(
            vec![
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::Push(0),
                IRInstruction::RuntimeCall("_map_create".to_string(), 5),
            ],
            ValueKind::Map,
        )
        .with_heap_ownership(HeapOwnership::Owned));
    }

    let mut instructions = Vec::new();
    let mut map_value_types: Option<MapValueTypes> = None;

    let key_value_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_key_value_slots = key_value_slots.clone();
    ordered_key_value_slots.sort_unstable();
    ordered_key_value_slots.reverse();

    let key_tag_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_key_tag_slots = key_tag_slots.clone();
    ordered_key_tag_slots.sort_unstable();
    ordered_key_tag_slots.reverse();

    let value_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_value_slots = value_slots.clone();
    ordered_value_slots.sort_unstable();
    ordered_value_slots.reverse();

    let value_tag_slots = context.allocate_contiguous_temp_slots(pair_count);
    let mut ordered_value_tag_slots = value_tag_slots.clone();
    ordered_value_tag_slots.sort_unstable();
    ordered_value_tag_slots.reverse();

    for idx in 0..pair_count {
        let key_node = &args[idx * 2];
        let value_node = &args[idx * 2 + 1];
        let key_literal = literal_map_key(key_node);

        let key_slot = ordered_key_value_slots[idx];
        let key_tag_slot = ordered_key_tag_slots[idx];
        let value_slot = ordered_value_slots[idx];
        let value_tag_slot = ordered_value_tag_slots[idx];

        let mut key_result = compile_node(key_node, context, program)?;
        let key_kind = resolve_map_key_kind(key_node, key_result.kind, context)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        ensure_owned_on_stack(&mut instructions, key_kind, &mut key_result.heap_ownership);
        instructions.push(IRInstruction::StoreLocal(key_slot));
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_kind)));
        instructions.push(IRInstruction::StoreLocal(key_tag_slot));
        key_result.free_retained_slots(&mut instructions, context);

        let mut value_result = compile_node(value_node, context, program)?;
        let value_kind = resolve_value_kind(value_node, value_result.kind, context);
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
        instructions.push(IRInstruction::StoreLocal(value_slot));
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_kind)));
        instructions.push(IRInstruction::StoreLocal(value_tag_slot));
        if value_kind.is_heap_kind() {
            instructions.push(IRInstruction::LoadLocal(value_slot));
            instructions.push(IRInstruction::LoadLocal(value_tag_slot));
            instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
            instructions.push(IRInstruction::StoreLocal(value_slot));
        }
        value_result.free_retained_slots(&mut instructions, context);

        if let Some(key_literal) = key_literal {
            if value_kind == ValueKind::Any {
                if let Some(types) = map_value_types.as_mut() {
                    types.remove(&key_literal);
                }
            } else {
                map_value_types.get_or_insert_with(HashMap::new).insert(key_literal, value_kind);
            }
        } else {
            map_value_types = None;
        }
    }

    instructions.push(IRInstruction::PushLocalAddress(ordered_key_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_key_tag_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_value_slots[0]));
    instructions.push(IRInstruction::PushLocalAddress(ordered_value_tag_slots[0]));
    instructions.push(IRInstruction::Push(pair_count as i64));
    instructions.push(IRInstruction::RuntimeCall("_map_create".to_string(), 5));

    key_value_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
    key_tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
    value_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));
    value_tag_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types))
}

pub(super) fn compile_assoc(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 3 {
        return Err(CompileError::ArityError("assoc".to_string(), 3, args.len()));
    }
    if (args.len() - 1) % 2 != 0 {
        return Err(CompileError::InvalidExpression("assoc expects key/value pairs".to_string()));
    }

    let mut base_result = compile_node(&args[0], context, program)?;
    let base_heap_ownership = base_result.heap_ownership;
    let mut map_value_types = base_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut base_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();

    if base_heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Map);
        temp_slots.push(slot);
    }

    for pair_idx in 0..((args.len() - 1) / 2) {
        let key_index = 1 + pair_idx * 2;
        let value_index = key_index + 1;

        let key_literal = literal_map_key(&args[key_index]);
        let mut slots_to_free_after_call = Vec::new();
        if let Some(literal) = key_literal.as_ref() {
            slots_to_free_after_call = take_slots_for_literal(&mut retained_slots, literal);
        }
        let mut key_result = compile_node(&args[key_index], context, program)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        key_result.kind = resolve_map_key_kind(&args[key_index], key_result.kind, context)?;
        ensure_owned_on_stack(&mut instructions, key_result.kind, &mut key_result.heap_ownership);
        let key_dependents = key_result.take_retained_slots();
        if key_result.kind.is_heap_kind() {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            temp_slots.push(slot);
            // dependents are owned by the map entry, so we intentionally do not free or track them here
            drop(key_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
        key_result.free_retained_slots(&mut instructions, context);

        let mut value_result = compile_node(&args[value_index], context, program)?;
        value_result.kind = resolve_value_kind(&args[value_index], value_result.kind, context);
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_result.kind, &mut value_result.heap_ownership);
        let value_dependents = value_result.take_retained_slots();
        if value_result.kind.is_heap_kind() {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            instructions.push(IRInstruction::Push(runtime_tag_for_value(value_result.kind)));
            instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            temp_slots.push(slot);
            drop(value_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_result.kind)));
        value_result.free_retained_slots(&mut instructions, context);

        instructions.push(IRInstruction::RuntimeCall("_map_assoc".to_string(), 5));
        slots_to_free_after_call.into_iter().for_each(|slot| {
            free_retained_slot(slot, &mut instructions, context);
        });

        if let Some(key_literal) = literal_map_key(&args[key_index]) {
            if value_result.kind == ValueKind::Any {
                if let Some(types) = map_value_types.as_mut() {
                    types.remove(&key_literal);
                }
            } else {
                map_value_types.get_or_insert_with(HashMap::new).insert(key_literal, value_result.kind);
            }
        } else {
            map_value_types = None;
        }
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    dedup_retained_slots(&mut retained_slots);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

pub(super) fn compile_dissoc(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Err(CompileError::ArityError("dissoc".to_string(), 1, 0));
    }

    let mut base_result = compile_node(&args[0], context, program)?;
    if args.len() == 1 {
        return Ok(base_result);
    }

    let base_heap_ownership = base_result.heap_ownership;
    let mut map_value_types = base_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut base_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();

    if base_heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Map);
        temp_slots.push(slot);
    }

    for key_idx in 1..args.len() {
        let mut key_result = compile_node(&args[key_idx], context, program)?;
        let key_instructions = std::mem::take(&mut key_result.instructions);
        extend_with_offset(&mut instructions, key_instructions);
        let mut owned_key_slot: Option<usize> = None;
        if key_result.heap_ownership == HeapOwnership::Owned {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            tracked_slots.insert(slot);
            slot_kinds.insert(slot, ValueKind::Any);
            temp_slots.push(slot);
            owned_key_slot = Some(slot);
        }
        key_result.kind = resolve_map_key_kind(&args[key_idx], key_result.kind, context)?;
        if let Some(slot) = owned_key_slot {
            slot_kinds.insert(slot, key_result.kind);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
        instructions.push(IRInstruction::RuntimeCall("_map_dissoc".to_string(), 3));
        key_result.free_retained_slots(&mut instructions, context);

        if let Some(key_literal) = literal_map_key(&args[key_idx]) {
            release_slots_for_literal(&mut retained_slots, &key_literal, &mut instructions, context);
            if let Some(types) = map_value_types.as_mut() {
                types.remove(&key_literal);
            }
        } else {
            map_value_types = None;
        }
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

pub(super) fn compile_disj(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.is_empty() {
        return Err(CompileError::ArityError("disj".to_string(), 1, 0));
    }

    let mut base_result = compile_node(&args[0], context, program)?;
    if args.len() == 1 {
        let mut result = base_result;
        if result.kind != ValueKind::Set {
            result.kind = ValueKind::Set;
        }
        return Ok(result);
    }

    let mut instructions = std::mem::take(&mut base_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let mut retained_slots = base_result.take_retained_slots();
    if base_result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Set);
        temp_slots.push(slot);
    }

    for value_idx in 1..args.len() {
        let value_literal = literal_map_key(&args[value_idx]);
        let mut value_result = compile_node(&args[value_idx], context, program)?;
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        let mut owned_value_slot: Option<usize> = None;
        if value_result.heap_ownership == HeapOwnership::Owned {
            let slot = context.allocate_temp_slot();
            instructions.push(IRInstruction::StoreLocal(slot));
            instructions.push(IRInstruction::LoadLocal(slot));
            tracked_slots.insert(slot);
            slot_kinds.insert(slot, ValueKind::Any);
            temp_slots.push(slot);
            owned_value_slot = Some(slot);
        }
        value_result.kind = resolve_map_key_kind(&args[value_idx], value_result.kind, context)?;
        if let Some(slot) = owned_value_slot {
            slot_kinds.insert(slot, value_result.kind);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_key(value_result.kind)));
        instructions.push(IRInstruction::RuntimeCall("_set_disj".to_string(), 3));
        value_result.free_retained_slots(&mut instructions, context);

        if let Some(literal) = value_literal {
            release_slots_for_literal(&mut retained_slots, &literal, &mut instructions, context);
        }
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    temp_slots.into_iter().for_each(|slot| context.release_temp_slot(slot));

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}

pub(super) fn compile_contains(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("contains?".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracker = SlotTracker::new();

    let owned_target_slot = tracker.track_if_owned(&mut instructions, context, target_result.heap_ownership, ValueKind::Any);

    let target_kind = resolve_value_kind(&args[0], target_result.kind, context);
    if let Some(slot) = owned_target_slot {
        tracker.set_slot_kind(slot, target_kind);
    }

    if target_kind == ValueKind::Map {
        if let Some(map_value_types) = target_result.map_value_types.as_ref() {
            if let Some(key_literal) = literal_map_key(&args[1]) {
                if map_value_types.contains_key(&key_literal) {
                    discard_loaded_target(&mut instructions, context, owned_target_slot);
                    instructions.push(IRInstruction::Push(1));

                    instructions = tracker.apply_liveness_and_release(instructions, context);
                    target_result.free_retained_slots(&mut instructions, context);

                    return Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean));
                }
            }
        }
    }

    let mut key_result = compile_node(&args[1], context, program)?;
    let key_instructions = std::mem::take(&mut key_result.instructions);
    extend_with_offset(&mut instructions, key_instructions);

    let owned_key_slot = tracker.track_if_owned(&mut instructions, context, key_result.heap_ownership, ValueKind::Any);

    key_result.kind = resolve_map_key_kind(&args[1], key_result.kind, context)?;
    if let Some(slot) = owned_key_slot {
        tracker.set_slot_kind(slot, key_result.kind);
    }
    instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
    let runtime = if target_kind == ValueKind::Set { "_set_contains" } else { "_map_contains" };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));
    key_result.free_retained_slots(&mut instructions, context);

    instructions = tracker.apply_liveness_and_release(instructions, context);
    target_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
}
fn discard_loaded_target(instructions: &mut Vec<IRInstruction>, context: &mut CompileContext, owned_slot: Option<usize>) {
    if let Some(slot) = owned_slot {
        instructions.push(IRInstruction::StoreLocal(slot));
    } else {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        context.release_temp_slot(slot);
    }
}
