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
mod types;

pub use context::CompileContext;
pub use types::{CompileResult, HeapOwnership, MapValueTypes, RetainedSlot, ValueKind};

use self::liveness::{apply_liveness_plan, compute_liveness_plan};
use crate::ast::{Node, Primitive};
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MapKeyLiteral {
    String(String),
    Keyword(String),
    Number(i64),
    Boolean(bool),
    Nil,
}

pub(super) fn is_count_debug_requested() -> bool {
    match std::env::var("SLISP_DEBUG_COUNTS") {
        Ok(value) => matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        Err(_) => false,
    }
}

fn maybe_insert_count_debug_preamble(program: &mut IRProgram) {
    if !is_count_debug_requested() {
        return;
    }

    program.add_instruction(IRInstruction::Push(1));
    program.add_instruction(IRInstruction::RuntimeCall("_count_debug_enable".to_string(), 1));
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
    maybe_insert_count_debug_preamble(&mut program);
    let mut context = CompileContext::new();
    let mut result = compile_node(node, &mut context, &mut program)?;
    let mut expr_instructions = std::mem::take(&mut result.instructions);
    result.free_retained_slots(&mut expr_instructions, &mut context);
    append_with_offset(&mut program, expr_instructions);
    program.add_instruction(IRInstruction::Return);
    Ok(program)
}

/// Compile a program (multiple top-level expressions) to IR
pub fn compile_program(expressions: &[Node]) -> Result<IRProgram, CompileError> {
    let mut program = IRProgram::new();
    maybe_insert_count_debug_preamble(&mut program);
    let mut context = CompileContext::new();
    let mut emitted_toplevel_code = false;

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

    // Prime function metadata (return types/ownership) by compiling each defn in a throwaway context
    // before any non-defn expressions run. This ensures early call sites (e.g. in other defns)
    // can infer accurate types even though full function compilation happens later.
    let mut metadata_context = context.clone();
    let mut metadata_program = IRProgram::new();
    for expr in expressions {
        if let Node::List { root } = expr {
            if let Some(Node::Symbol { value }) = root.first() {
                if value == "defn" {
                    // Skip malformed defns here; they'll be reported in the main compilation loop.
                    if root.len() == 4 {
                        functions::compile_defn(&root[1..], &mut metadata_context, &mut metadata_program)?;
                    }
                }
            }
        }
    }
    context.function_return_types = metadata_context.function_return_types.clone();
    context.function_return_map_value_types = metadata_context.function_return_map_value_types.clone();
    context.function_return_ownership = metadata_context.function_return_ownership.clone();

    let mut pending_defns: Vec<Vec<Node>> = Vec::new();

    // Second pass: compile non-defn expressions, collect function bodies for later
    for expr in expressions {
        if let Node::List { root } = expr {
            if !root.is_empty() {
                if let Node::Symbol { value } = &root[0] {
                    if value == "defn" {
                        pending_defns.push(root.clone());
                        continue;
                    }
                }
            }
        }

        let mut result = compile_node(expr, &mut context, &mut program)?;
        let mut expr_instructions = std::mem::take(&mut result.instructions);
        result.free_retained_slots(&mut expr_instructions, &mut context);
        append_with_offset(&mut program, expr_instructions);
        emitted_toplevel_code = true;
    }

    if emitted_toplevel_code && program.instructions.last() != Some(&IRInstruction::Return) {
        program.add_instruction(IRInstruction::Return);
    }

    // Compile functions in reverse order so parameter inference from later call sites is available.
    for defn in pending_defns.into_iter().rev() {
        let (mut instructions, func_info) = functions::compile_defn(&defn[1..], &mut context, &mut program)?;
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

        append_with_offset(&mut program, instructions);
        program.add_function(updated_func_info);
    }

    if context.get_function("-main").is_some() {
        program.set_entry_point("-main".to_string());
    }

    Ok(program)
}

fn append_with_offset(program: &mut IRProgram, instructions: Vec<IRInstruction>) {
    if instructions.is_empty() {
        return;
    }

    let base = program.len();
    for instruction in instructions {
        let adjusted = match instruction {
            IRInstruction::Jump(target) => IRInstruction::Jump(base + target),
            IRInstruction::JumpIfZero(target) => IRInstruction::JumpIfZero(base + target),
            other => other,
        };
        program.add_instruction(adjusted);
    }
}

pub(super) fn extend_with_offset(target: &mut Vec<IRInstruction>, mut new_instructions: Vec<IRInstruction>) {
    if new_instructions.is_empty() {
        return;
    }

    let base = target.len();
    if base != 0 {
        for instruction in &mut new_instructions {
            match instruction {
                IRInstruction::Jump(target_idx) | IRInstruction::JumpIfZero(target_idx) => {
                    *target_idx += base;
                }
                _ => {}
            }
        }
    }

    target.extend(new_instructions);
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
                let ownership = if kind.is_heap_kind() && context.is_heap_allocated(value) {
                    HeapOwnership::Borrowed
                } else {
                    HeapOwnership::None
                };
                Ok(CompileResult::with_instructions(vec![IRInstruction::LoadParam(slot)], kind).with_heap_ownership(ownership))
            } else if let Some(slot) = context.get_variable(value) {
                let kind = context.get_variable_type(value).unwrap_or(ValueKind::Any);
                let ownership = if kind.is_heap_kind() && context.is_heap_allocated(value) {
                    HeapOwnership::Borrowed
                } else {
                    HeapOwnership::None
                };
                let map_value_types = context.get_variable_map_value_types(value).cloned();
                Ok(CompileResult::with_instructions(vec![IRInstruction::LoadLocal(slot)], kind)
                    .with_heap_ownership(ownership)
                    .with_map_value_types(map_value_types))
            } else {
                Err(CompileError::UndefinedVariable(value.clone()))
            }
        }
        Node::List { root } => compile_list(root, context, program),
        Node::Vector { root } => compile_vector_literal(root, context, program),
        Node::Map { entries } => {
            let mut flattened = Vec::with_capacity(entries.len() * 2);
            for (key, value) in entries {
                flattened.push(key.clone());
                flattened.push(value.clone());
            }
            compile_hash_map(&flattened, context, program)
        }
        Node::Set { root } => compile_set_literal(root, context, program),
    }
}

fn compile_vector_literal(elements: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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

    for slot in value_slots {
        if !retains_slot(&retained_slots, slot) {
            context.release_temp_slot(slot);
        }
    }
    for slot in tag_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Vector)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}

fn compile_set_literal(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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

    for slot in value_slots {
        if !retains_slot(&retained_slots, slot) {
            context.release_temp_slot(slot);
        }
    }

    for slot in tag_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}

/// Compile count operation (string length)
fn compile_count(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 1 {
        return Err(CompileError::ArityError("count".to_string(), 1, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    #[cfg(debug_assertions)]
    if std::env::var("SLISP_DEBUG_COUNT").is_ok() {
        eprintln!("[slisp:debug_count] arg ownership={:?} kind={:?}", arg_result.heap_ownership, arg_result.kind);
    }
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();

    if arg_result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Any);
        temp_slots.push(slot);
    }

    let mut target_kind = arg_result.kind;
    if target_kind == ValueKind::Any {
        if let Node::Symbol { value } = &args[0] {
            if let Some(var_kind) = context.get_variable_type(value) {
                target_kind = var_kind;
            } else if let Some(param_kind) = context.get_parameter_type(value) {
                target_kind = param_kind;
            }
        }
    }

    let runtime = match target_kind {
        ValueKind::Vector => "_vector_count",
        ValueKind::Map => "_map_count",
        ValueKind::Set => "_set_count",
        _ => "_string_count",
    };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 1));

    for slot in &temp_slots {
        slot_kinds.insert(*slot, target_kind);
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }
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

fn runtime_free_for_kind(kind: ValueKind) -> Option<&'static str> {
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
    for dependent in slot.dependents {
        free_retained_slot(dependent, instructions, context);
    }
    emit_free_for_slot(instructions, slot.slot, slot.kind);
    context.release_temp_slot(slot.slot);
}

pub(super) fn free_retained_dependents(slot: &mut RetainedSlot, instructions: &mut Vec<IRInstruction>, context: &mut CompileContext) {
    for dependent in slot.dependents.drain(..) {
        free_retained_slot(dependent, instructions, context);
    }
}

fn apply_plan_with_slot_kinds(instructions: Vec<IRInstruction>, plan: &liveness::LivenessPlan, slot_kinds: &HashMap<usize, ValueKind>) -> Vec<IRInstruction> {
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
    let mut idx = 0;
    while idx < retained_slots.len() {
        if retained_slots[idx].key.as_ref() == Some(literal) {
            let slot_info = retained_slots.remove(idx);
            free_retained_slot(slot_info, instructions, context);
        } else {
            idx += 1;
        }
    }
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

fn compile_get(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

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

/// Compile subs operation (substring extraction)
fn compile_subs(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(CompileError::ArityError("subs".to_string(), 2, args.len()));
    }

    let mut arg_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut arg_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();

    if arg_result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Any);
        temp_slots.push(slot);
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

    let mut target_kind = arg_result.kind;
    if target_kind == ValueKind::Any {
        if let Node::Symbol { value } = &args[0] {
            if let Some(var_kind) = context.get_variable_type(value) {
                target_kind = var_kind;
            } else if let Some(param_kind) = context.get_parameter_type(value) {
                target_kind = param_kind;
            }
        }
    }

    let runtime = if target_kind == ValueKind::Vector { "_vector_slice" } else { "_string_subs" };

    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));

    for slot in &temp_slots {
        slot_kinds.insert(*slot, target_kind);
    }

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }
    arg_result.free_retained_slots(&mut instructions, context);

    let result_kind = if target_kind == ValueKind::Vector { ValueKind::Vector } else { ValueKind::String };

    Ok(CompileResult::with_instructions(instructions, result_kind).with_heap_ownership(HeapOwnership::Owned))
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

    for (slot, free) in ordered_slots.iter().zip(needs_free.iter()) {
        if *free {
            instructions.push(IRInstruction::FreeLocal(*slot));
        }
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::String).with_heap_ownership(HeapOwnership::Owned))
}

fn compile_hash_map(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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
    let mut retained_slots: Vec<RetainedSlot> = Vec::new();

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
        let key_dependents = key_result.take_retained_slots();
        instructions.push(IRInstruction::StoreLocal(key_slot));
        track_heap_slot(&mut retained_slots, key_slot, key_kind, key_literal.clone(), key_dependents);
        instructions.push(IRInstruction::Push(runtime_tag_for_key(key_kind)));
        instructions.push(IRInstruction::StoreLocal(key_tag_slot));
        key_result.free_retained_slots(&mut instructions, context);

        let mut value_result = compile_node(value_node, context, program)?;
        let value_kind = resolve_value_kind(value_node, value_result.kind, context);
        let value_instructions = std::mem::take(&mut value_result.instructions);
        extend_with_offset(&mut instructions, value_instructions);
        ensure_owned_on_stack(&mut instructions, value_kind, &mut value_result.heap_ownership);
        let value_dependents = value_result.take_retained_slots();
        instructions.push(IRInstruction::StoreLocal(value_slot));
        track_heap_slot(&mut retained_slots, value_slot, value_kind, key_literal.clone(), value_dependents);
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_kind)));
        instructions.push(IRInstruction::StoreLocal(value_tag_slot));
        value_result.free_retained_slots(&mut instructions, context);

        if let Some(key_literal) = literal_map_key(key_node) {
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

    dedup_retained_slots(&mut retained_slots);

    for slot in key_value_slots {
        if !retains_slot(&retained_slots, slot) {
            context.release_temp_slot(slot);
        }
    }
    for slot in key_tag_slots {
        context.release_temp_slot(slot);
    }
    for slot in value_slots {
        if !retains_slot(&retained_slots, slot) {
            context.release_temp_slot(slot);
        }
    }
    for slot in value_tag_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

fn compile_assoc(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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
        if let Some(literal) = key_literal.as_ref() {
            release_slots_for_literal(&mut retained_slots, literal, &mut instructions, context);
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
            track_heap_slot(&mut retained_slots, slot, key_result.kind, key_literal.clone(), key_dependents);
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
            track_heap_slot(&mut retained_slots, slot, value_result.kind, key_literal.clone(), value_dependents);
        }
        instructions.push(IRInstruction::Push(runtime_tag_for_value(value_result.kind)));
        value_result.free_retained_slots(&mut instructions, context);

        instructions.push(IRInstruction::RuntimeCall("_map_assoc".to_string(), 5));

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

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    dedup_retained_slots(&mut retained_slots);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

fn compile_dissoc(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Map)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_map_value_types(map_value_types)
        .with_retained_slots(retained_slots))
}

fn compile_disj(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
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

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }

    Ok(CompileResult::with_instructions(instructions, ValueKind::Set)
        .with_heap_ownership(HeapOwnership::Owned)
        .with_retained_slots(retained_slots))
}

fn compile_contains(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<CompileResult, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("contains?".to_string(), 2, args.len()));
    }

    let mut target_result = compile_node(&args[0], context, program)?;
    let mut instructions = std::mem::take(&mut target_result.instructions);
    let mut tracked_slots: HashSet<usize> = HashSet::new();
    let mut slot_kinds: HashMap<usize, ValueKind> = HashMap::new();
    let mut temp_slots = Vec::new();

    if target_result.heap_ownership == HeapOwnership::Owned {
        let slot = context.allocate_temp_slot();
        instructions.push(IRInstruction::StoreLocal(slot));
        instructions.push(IRInstruction::LoadLocal(slot));
        tracked_slots.insert(slot);
        slot_kinds.insert(slot, ValueKind::Any);
        temp_slots.push(slot);
    }

    let target_kind = resolve_value_kind(&args[0], target_result.kind, context);
    if let Some(slot) = temp_slots.first() {
        slot_kinds.insert(*slot, target_kind);
    }

    let mut key_result = compile_node(&args[1], context, program)?;
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
    key_result.kind = resolve_map_key_kind(&args[1], key_result.kind, context)?;
    if let Some(slot) = owned_key_slot {
        slot_kinds.insert(slot, key_result.kind);
    }
    instructions.push(IRInstruction::Push(runtime_tag_for_key(key_result.kind)));
    let runtime = if target_kind == ValueKind::Set { "_set_contains" } else { "_map_contains" };
    instructions.push(IRInstruction::RuntimeCall(runtime.to_string(), 3));
    key_result.free_retained_slots(&mut instructions, context);

    if !tracked_slots.is_empty() {
        let plan = compute_liveness_plan(&instructions, &tracked_slots);
        instructions = apply_plan_with_slot_kinds(instructions, &plan, &slot_kinds);
    }

    for slot in temp_slots {
        context.release_temp_slot(slot);
    }
    target_result.free_retained_slots(&mut instructions, context);

    Ok(CompileResult::with_instructions(instructions, ValueKind::Boolean))
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
            "vec" => compile_vector_literal(args, context, program),
            "set" => compile_set_literal(args, context, program),
            "hash-map" => compile_hash_map(args, context, program),
            "assoc" => compile_assoc(args, context, program),
            "dissoc" => compile_dissoc(args, context, program),
            "disj" => compile_disj(args, context, program),
            "contains?" => compile_contains(args, context, program),
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
    fn test_compile_string_equality_runtime_call() {
        let program = compile_expression("(= (str \"a\") (str \"a\"))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 2) if name == "_string_equals")));
    }

    #[test]
    fn test_compile_simple_string_equality_program() {
        let program = compile_expression("(if (= \"alpha\" \"alpha\") 1 0)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 2) if name == "_string_equals")));
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
    fn test_compile_keyword_literal() {
        let program = compile_expression(":kw").unwrap();
        assert_eq!(program.instructions, vec![IRInstruction::PushString(0), IRInstruction::Return]);
        assert_eq!(program.string_literals, vec![":kw".to_string()]);
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
    fn test_compile_vector_literal() {
        let program = compile_expression("[1 2]").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_vector_create"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_vec_builtin() {
        let program = compile_expression("(vec 4 5)").unwrap();
        assert!(program.instructions.contains(&IRInstruction::RuntimeCall("_vector_create".to_string(), 3)));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_set_builtin_calls_runtime() {
        let program = compile_expression("(set 1 2)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_create"
        )));
    }

    #[test]
    fn test_compile_count_vector_calls_runtime() {
        let program = compile_expression("(count (vec 1 2 3))").unwrap();
        assert!(program.instructions.contains(&IRInstruction::RuntimeCall("_vector_count".to_string(), 1)));
    }

    #[test]
    fn test_compile_count_set_calls_runtime() {
        let program = compile_expression("(count (set 1 2))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_set_count"
        )));
    }

    #[test]
    fn test_compile_vector_subs() {
        let program = compile_expression("(subs (vec 1 2 3) 1 2)").unwrap();
        assert!(program.instructions.contains(&IRInstruction::RuntimeCall("_vector_slice".to_string(), 3)));
    }

    #[test]
    fn test_compile_vector_get_runtime_call() {
        let program = compile_expression("(get (vec 9 8) 0)").unwrap();
        assert!(program.instructions.contains(&IRInstruction::RuntimeCall("_vector_get".to_string(), 3)));
    }

    #[test]
    fn test_compile_hash_map_literal_runtime_call() {
        let program = compile_expression("(hash-map \"a\" 1 \"b\" 2)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_create"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_map_literal_syntax_runtime_call() {
        let program = compile_expression("{\"a\" 1 \"b\" 2}").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_create"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_get_on_map_literal_infers_vector_kind() {
        let program = compile_expression("(count (get {:nums (vec 1 2)} :nums))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_vector_count"
        )));
    }

    #[test]
    fn test_assoc_followed_by_get_emits_map_get() {
        let program = compile_expression("(let [numbers #{1 2 3} combos {:nums numbers} trimmed (assoc combos :nums (disj numbers 2))] (get trimmed :nums))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_get"
        )));
    }

    #[test]
    fn test_compile_set_literal_syntax_runtime_call() {
        let program = compile_expression("#{1 2}").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_create"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_empty_set_literal_syntax_runtime_call() {
        let program = compile_expression("#{}").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_create"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_keyword_map_key_emits_keyword_tag() {
        let program = compile_expression("{:name 1}").unwrap();
        assert!(program.string_literals.contains(&":name".to_string()));
        let expected = ValueKind::Keyword.runtime_tag();
        assert!(program.instructions.iter().any(|inst| matches!(inst, IRInstruction::Push(value) if *value == expected)));
    }

    #[test]
    fn test_compile_assoc_runtime_call() {
        let program = compile_expression("(assoc (hash-map) \"a\" 1)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_assoc"
        )));
    }

    #[test]
    fn test_compile_map_get_runtime_call() {
        let program = compile_expression("(get (hash-map \"a\" 1) \"a\" 0)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_get"
        )));
    }

    #[test]
    fn test_compile_contains_runtime_call() {
        let program = compile_expression("(contains? (hash-map \"a\" 1) \"a\")").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_map_contains"
        )));
    }

    #[test]
    fn test_compile_contains_set_runtime_call() {
        let program = compile_expression("(contains? (set 1 2) 1)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_contains"
        )));
    }

    #[test]
    fn test_compile_disj_runtime_call() {
        let program = compile_expression("(disj (set 1 2) 1)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_disj"
        )));
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
    fn test_compile_str_with_set_invokes_runtime() {
        let program = compile_expression("(str (set 1))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_set_to_string"
        )));
    }

    #[test]
    fn test_subs_string_equality_calls_runtime() {
        let program = compile_expression("(= (subs \"alphabet\" 5) \"bet\")").unwrap();
        assert!(
            program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 2) if name == "_string_equals")),
            "expected string equality: {:#?}",
            program.instructions
        );
    }

    #[test]
    fn test_if_with_string_equality_generates_expected_flow() {
        let program = compile_expression("(if (= (subs \"alphabet\" 5) \"bet\") 0 1)").unwrap();
        assert!(
            program
                .instructions
                .iter()
                .filter(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 2) if name == "_string_equals"))
                .count()
                == 1,
            "expected exactly one string equality call: {:#?}",
            program.instructions
        );
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_let_with_string_equality_condition() {
        let program = compile_expression("(let [fallback (subs \"alphabet\" 5)] (if (= fallback \"bet\") 0 1))").unwrap();
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_get_string_index() {
        let program = compile_expression("(get \"abc\" 1)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 2) if name == "_string_get"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
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
    fn test_count_frees_owned_argument() {
        let program = compile_expression("(count (str 42))").unwrap();
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
                IRInstruction::StoreLocal(0),
                IRInstruction::LoadLocal(0),
                IRInstruction::RuntimeCall("_string_count".to_string(), 1),
                IRInstruction::FreeLocal(0),
                IRInstruction::Return,
            ]
        );
    }

    #[test]
    fn test_get_frees_owned_argument() {
        let program = compile_expression("(get (str 42) 0)").unwrap();
        let free_count = program.instructions.iter().filter(|inst| matches!(inst, IRInstruction::FreeLocal(0))).count();
        assert!(free_count >= 1, "expected owned argument to be freed at least once");
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 2) if name == "_string_get"
        )));
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
    }

    #[test]
    fn test_subs_frees_owned_argument() {
        let program = compile_expression("(subs (str 42) 0 1)").unwrap();
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
                IRInstruction::StoreLocal(0),
                IRInstruction::LoadLocal(0),
                IRInstruction::Push(0),
                IRInstruction::Push(1),
                IRInstruction::RuntimeCall("_string_subs".to_string(), 3),
                IRInstruction::FreeLocal(0),
                IRInstruction::Return,
            ]
        );
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
        std::fs::write("id_program.debug", format!("{:#?}", program.instructions)).unwrap();
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
            !program.instructions[..call_pos]
                .iter()
                .any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 1) if name == "_string_clone")),
            "call site should pass argument by borrowing without cloning: {:?}",
            program.instructions
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
    fn test_compile_program_top_level_expression_emits_return() {
        let expressions = vec![AstParser::parse_sexp_new_domain("(let [a 1] (+ a 2))".as_bytes(), &mut 0)];
        let program = compile_program(&expressions).unwrap();
        assert_eq!(program.instructions.last(), Some(&IRInstruction::Return));
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
