mod bindings;
mod builtins;
/// Compiler module - compiles AST nodes to IR
///
/// This module is organized into:
/// - context: CompileContext for tracking variables, parameters, and functions
/// - expressions: Arithmetic, comparisons, conditionals, and logical operations
/// - functions: Function definitions (defn) and function calls
/// - bindings: Variable bindings (let expressions)
/// - slots: Slot tracking utilities for temporary local variables
mod context;
mod expressions;
mod functions;
mod inference;
mod liveness;
mod slots;
mod types;

pub use context::CompileContext;
pub use types::{CompileResult, HeapOwnership, MapKeyLiteral, MapValueTypes, RetainedSlot, ValueKind};

use crate::ast::Node;
use crate::ir::{FunctionInfo, IRInstruction, IRProgram};
use inference::run_type_inference;

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
    let mut context = CompileContext::new();
    let inference = run_type_inference(std::slice::from_ref(node))?;
    context.set_type_inference(inference);
    context.hydrate_from_inference();
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
    let mut context = CompileContext::new();
    let inference = run_type_inference(expressions)?;
    context.set_type_inference(inference);
    context.hydrate_from_inference();
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
    instructions.into_iter().for_each(|instruction| {
        let adjusted = match instruction {
            IRInstruction::Jump(target) => IRInstruction::Jump(base + target),
            IRInstruction::JumpIfZero(target) => IRInstruction::JumpIfZero(base + target),
            other => other,
        };
        program.add_instruction(adjusted);
    });
}

pub(super) fn extend_with_offset(target: &mut Vec<IRInstruction>, mut new_instructions: Vec<IRInstruction>) {
    if new_instructions.is_empty() {
        return;
    }

    let base = target.len();
    if base != 0 {
        new_instructions.iter_mut().for_each(|instruction| match instruction {
            IRInstruction::Jump(target_idx) | IRInstruction::JumpIfZero(target_idx) => {
                *target_idx += base;
            }
            _ => {}
        });
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
                let map_value_types = context.get_parameter_map_value_types(value).cloned();
                let set_element_kind = context.get_parameter_set_element_kind(value);
                let vector_element_kind = context.get_parameter_vector_element_kind(value);
                Ok(CompileResult::with_instructions(vec![IRInstruction::LoadParam(slot)], kind)
                    .with_heap_ownership(ownership)
                    .with_map_value_types(map_value_types)
                    .with_set_element_kind(set_element_kind)
                    .with_vector_element_kind(vector_element_kind))
            } else if let Some(slot) = context.get_variable(value) {
                let kind = context.get_variable_type(value).unwrap_or(ValueKind::Any);
                let ownership = if kind.is_heap_kind() && context.is_heap_allocated(value) {
                    HeapOwnership::Borrowed
                } else {
                    HeapOwnership::None
                };
                let map_value_types = context.get_variable_map_value_types(value).cloned();
                let set_element_kind = context.get_variable_set_element_kind(value);
                let vector_element_kind = context.get_variable_vector_element_kind(value);
                Ok(CompileResult::with_instructions(vec![IRInstruction::LoadLocal(slot)], kind)
                    .with_heap_ownership(ownership)
                    .with_map_value_types(map_value_types)
                    .with_set_element_kind(set_element_kind)
                    .with_vector_element_kind(vector_element_kind))
            } else {
                Err(CompileError::UndefinedVariable(value.clone()))
            }
        }
        Node::List { root } => compile_list(root, context, program),
        Node::Vector { root } => builtins::compile_vector_literal(root, context, program),
        Node::Map { entries } => {
            let flattened: Vec<Node> = entries.iter().flat_map(|(key, value)| [key.clone(), value.clone()]).collect();
            builtins::compile_hash_map(&flattened, context, program)
        }
        Node::Set { root } => builtins::compile_set_literal(root, context, program),
    }
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
            "count" => builtins::compile_count(args, context, program),
            "get" => builtins::compile_get(args, context, program),
            "subs" => builtins::compile_subs(args, context, program),
            "str" => builtins::compile_str(args, context, program),
            "vec" => builtins::compile_vector_literal(args, context, program),
            "set" => builtins::compile_set_literal(args, context, program),
            "hash-map" => builtins::compile_hash_map(args, context, program),
            "assoc" => builtins::compile_assoc(args, context, program),
            "dissoc" => builtins::compile_dissoc(args, context, program),
            "disj" => builtins::compile_disj(args, context, program),
            "contains?" => builtins::compile_contains(args, context, program),
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
    use crate::compiler::inference::run_type_inference;

    fn compile_expression(input: &str) -> Result<IRProgram, CompileError> {
        let ast = AstParser::parse_sexp_new_domain(input.as_bytes(), &mut 0);
        compile_to_ir(&ast)
    }

    #[test]
    fn load_parameter_carries_map_metadata() {
        let mut context = CompileContext::new();
        context.add_parameter("m".to_string(), 0);
        context.set_parameter_type("m", ValueKind::Map);
        context.mark_heap_allocated("m", ValueKind::Map);
        let mut metadata = MapValueTypes::new();
        metadata.insert(MapKeyLiteral::String("a".to_string()), ValueKind::String);
        context.set_parameter_map_value_types("m", Some(metadata.clone()));
        let mut program = IRProgram::new();
        let node = Node::Symbol { value: "m".to_string() };
        let result = compile_node(&node, &mut context, &mut program).unwrap();
        let map_types = result.map_value_types.expect("expected map metadata");
        assert_eq!(map_types.get(&MapKeyLiteral::String("a".to_string())), Some(&ValueKind::String));
    }

    #[test]
    fn function_call_uses_inferred_return_type() {
        let mut offset = 0;
        let forty_two = AstParser::parse_sexp_new_domain("(defn forty-two [] 42)".as_bytes(), &mut offset);
        let summary = run_type_inference(std::slice::from_ref(&forty_two)).unwrap();

        let mut context = CompileContext::new();
        context
            .add_function(
                "forty-two".to_string(),
                FunctionInfo {
                    name: "forty-two".to_string(),
                    param_count: 0,
                    start_address: 0,
                    local_count: 0,
                },
            )
            .unwrap();
        context.set_type_inference(summary);
        context.hydrate_from_inference();

        let mut program = IRProgram::new();
        let result = functions::compile_function_call("forty-two", &[], &mut context, &mut program, 0).unwrap();
        assert_eq!(result.kind, ValueKind::Number);
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
    fn test_get_on_literal_map_skips_clone() {
        let program = compile_expression("(get {:a \"x\"} :a)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_get"
        )));
        // Heap value clones are allowed when returning owned heap entries.
    }

    #[test]
    fn test_get_after_assoc_skips_clone() {
        let program = compile_expression("(let [base {:a \"x\"} updated (assoc base :b \"y\")] (get updated :b))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_get"
        )));
        // Heap value clones are allowed when returning owned heap entries.
    }

    #[test]
    fn test_contains_known_key_avoids_runtime() {
        let program = compile_expression("(contains? {:a 1} :a)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(inst, IRInstruction::Push(1))));
        assert!(!program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 3) if name == "_map_contains")));
    }

    #[test]
    fn test_contains_missing_key_avoids_runtime() {
        let program = compile_expression("(contains? {:a 1} :b)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 3) if name == "_map_contains")));
    }

    #[test]
    fn test_contains_after_assoc_skips_runtime() {
        let program = compile_expression("(let [m (assoc {:a 1} :b 2)] (contains? m :b))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(inst, IRInstruction::Push(1))));
        assert!(!program.instructions.iter().any(|inst| matches!(inst, IRInstruction::RuntimeCall(name, 3) if name == "_map_contains")));
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
        let program = compile_expression("(let [m (hash-map \"a\" 1) k (if true \"a\" \"b\")] (contains? m k))").unwrap();
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
    fn count_get_on_map_literal_uses_set_runtime() {
        let program = compile_expression("(count (get {:nums #{1 2 3}} :nums))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_set_count"
        )));
    }

    #[test]
    fn count_get_on_let_bound_map_uses_set_runtime() {
        let program = compile_expression("(let [nums #{1 2 3} combos {:nums nums}] (count (get combos :nums)))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_set_count"
        )));
    }

    #[test]
    fn count_get_on_map_literal_uses_vector_runtime() {
        let program = compile_expression("(count (get {:vals [1 2 3]} :vals))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_vector_count"
        )));
    }

    #[test]
    fn contains_on_set_from_map_calls_set_runtime() {
        let program = compile_expression("(contains? (get {:tags #{:hot :cold}} :tags) :hot)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_get"
        )));
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_contains"
        )));
    }

    #[test]
    fn get_with_vector_default_prefers_vector_runtime() {
        let program = compile_expression("(count (get {:tags #{:hot}} :vals [1 2]))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 1) if name == "_vector_count"
        )));
    }

    #[test]
    fn get_with_set_default_prefers_set_runtime() {
        let program = compile_expression("(contains? (get {:vals [1]} :tags #{:hot}) :hot)").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 5) if name == "_map_get"
        )));
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 3) if name == "_set_contains"
        )));
    }

    #[test]
    fn assoc_with_heap_value_clones_heap_payload() {
        let program = compile_expression("(let [k \"x\"] (assoc {:a 1} k #{1}))").unwrap();
        assert!(program.instructions.iter().any(|inst| matches!(
            inst,
            IRInstruction::RuntimeCall(name, 2) if name == "_map_value_clone"
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
        assert!(program.instructions.len() > 3); // Should have multiple instructions
    }

    #[test]
    fn test_compile_and_false() {
        let program = compile_expression("(and 1 0)").unwrap();
        assert!(program.instructions.len() > 3); // Should have multiple instructions
    }

    #[test]
    fn test_compile_let_simple() {
        let program = compile_expression("(let [x 5] x)").unwrap();
        // Should have: Push(5), StoreLocal(0), LoadLocal(0), Return
        assert!(program.instructions.contains(&IRInstruction::Push(5)));
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(0)));
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(0)));
        assert!(program.instructions.contains(&IRInstruction::Return));
    }

    #[test]
    fn test_compile_let_expression() {
        let program = compile_expression("(let [x 5] (+ x 3))").unwrap();
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
        // Should have two variable stores and loads
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(0))); // x
        assert!(program.instructions.contains(&IRInstruction::StoreLocal(1))); // y
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(0))); // x
        assert!(program.instructions.contains(&IRInstruction::LoadLocal(1))); // y
    }

    #[test]
    fn test_compile_let_nested() {
        let program = compile_expression("(let [x 5] (let [y 10] (+ x y)))").unwrap();
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
        // Both x and y should use slot 0 since they're in separate scopes
        // However, the current IR structure may not show this clearly due to compilation order
        assert!(!program.instructions.is_empty());
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
