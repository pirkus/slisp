use super::super::ownership::{apply_plan_with_slot_kinds, clone_runtime_for_kind, dedup_retained_slots, emit_free_for_slot, free_retained_slot};
use super::common::{literal_map_key, release_temp_slots, resolve_map_key_kind, resolve_value_kind, runtime_tag_for_key, track_owned_argument};
use crate::ast::Node;
use crate::compiler::{compile_node, extend_with_offset, liveness::compute_liveness_plan, CompileContext, CompileError, CompileResult, HeapOwnership, RetainedSlot, ValueKind};
use crate::ir::{IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

pub(crate) fn compile_count(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("count".to_string(), 1, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();

    track_owned_argument(&arg_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);

    let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);

    let runtime = match target_kind {
        ValueKind::Vector => "_vector_count",
        ValueKind::Map => "_map_count",
        ValueKind::Set => "_set_count",
        _ => "_string_count",
    };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));

    temp_slots.iter().for_each(|slot| {
        slot_kinds.insert(*slot, target_kind);
    });

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    release_temp_slots(temp_slots, context);
    arg_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Number))
}

pub(crate) fn compile_get(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("get".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let target_map_value_types = target_result.map_value_types.clone();
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();
    let owned_arg_slot = track_owned_argument(&target_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);

    let mut key_result = compile_node(&args[1], context, program)?;
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

    let owned_key_slot = track_owned_argument(&key_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);

    match target_kind {
        ValueKind::Vector => {
            emit_vector_get(&mut instructions, context, &mut tracked_slots, &mut slot_kinds, owned_arg_slot, &mut default_handling);
        }
        ValueKind::Map => {
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

            instructions.push(IRInstruction::LoadLocal(value_slot));
            instructions.push(IRInstruction::LoadLocal(tag_slot));
            instructions.push(IRInstruction::RuntimeCall("_map_value_clone".to_string(), 2));
            instructions.push(IRInstruction::StoreLocal(value_slot));

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

    release_temp_slots(temp_slots, context);

    default_handling.release_slot(context);
    target_result.free_retained_slots(&mut instructions, context);

    let inferred_map_value_kind = target_map_value_types.as_ref().and_then(|types| literal_map_key(&args[1]).and_then(|key| types.get(&key).copied()));

    let result_kind = match target_kind {
        ValueKind::Vector => default_handling.inferred_kind().unwrap_or(ValueKind::Any),
        ValueKind::Map => inferred_map_value_kind.or_else(|| default_handling.inferred_kind()).unwrap_or(ValueKind::Any),
        _ if default_handling.has_value() => default_handling.inferred_kind().unwrap_or(ValueKind::String),
        _ => ValueKind::String,
    };

    let heap_ownership = match target_kind {
        ValueKind::Vector => HeapOwnership::None,
        ValueKind::Map => {
            if result_kind.is_heap_clone_kind() {
                HeapOwnership::Owned
            } else {
                HeapOwnership::None
            }
        }
        _ => HeapOwnership::Owned,
    };

    let retained_slots = default_handling.take_retained_slots();

    Ok(CompileResult::with_instructions(instructions, result_kind)
        .with_heap_ownership(heap_ownership)
        .with_retained_slots(retained_slots))
}

pub(crate) fn compile_subs(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("subs".to_string(), 2, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();

    track_owned_argument(&arg_result, &mut instructions, context, &mut slot_kinds, &mut tracked_slots, &mut temp_slots, ValueKind::Any);

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

    let target_kind = resolve_value_kind(&args[0], arg_result.kind, context);

    let runtime = if target_kind == ValueKind::Vector { "_vector_slice" } else { "_string_subs" };

    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));

    temp_slots.iter().for_each(|slot| {
        slot_kinds.insert(*slot, target_kind);
    });

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    release_temp_slots(temp_slots, context);
    arg_result.free_retained_slots(&mut instructions, context);

    let result_kind = if target_kind == ValueKind::Vector { ValueKind::Vector } else { ValueKind::String };

    Ok(CompileResult::with_instructions(instructions, result_kind).with_heap_ownership(HeapOwnership::Owned))
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
            for slot in default.retained_slots.drain(..) {
                free_retained_slot(slot, instructions, context);
            }
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
