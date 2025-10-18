use super::{CompileContext, CompileError};
/// Variable binding compilation (let expressions)
use crate::ast::Node;
use crate::ir::{IRInstruction, IRProgram};

/// Compile a let binding expression
pub fn compile_let(args: &[Node], context: &mut CompileContext, program: &mut IRProgram) -> Result<Vec<IRInstruction>, CompileError> {
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

    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        let var_name = match var_node {
            Node::Symbol { value } => value,
            _ => return Err(CompileError::InvalidExpression("let binding variables must be symbols".to_string())),
        };

        // Check if the value expression produces a heap-allocated result
        let is_heap_allocated = is_heap_allocating_expression(val_node);

        let mut value_instructions = crate::compiler::compile_node(val_node, context, program)?;

        let mut cloned_from_existing = false;
        if let Node::Symbol { value } = val_node {
            if crate::compiler::is_heap_allocated_symbol(value, context) {
                value_instructions.push(IRInstruction::RuntimeCall("_string_clone".to_string(), 1));
                cloned_from_existing = true;
            }
        }

        instructions.extend(value_instructions);

        let slot = context.add_variable(var_name.clone());
        instructions.push(IRInstruction::StoreLocal(slot));

        // Mark variable as heap-allocated if needed
        if is_heap_allocated || cloned_from_existing {
            context.mark_heap_allocated(var_name);
        }

        added_variables.push(var_name.clone());
    }

    let mut body_instructions = crate::compiler::compile_node(&args[1], context, program)?;

    if let Node::Symbol { value } = &args[1] {
        if added_variables.iter().any(|name| name == value) && crate::compiler::is_heap_allocated_symbol(value, context) {
            body_instructions.push(IRInstruction::RuntimeCall("_string_clone".to_string(), 1));
        }
    }

    instructions.extend(body_instructions);

    // Free heap-allocated variables before removing them from scope
    // Use FreeLocal to avoid pushing values onto stack and preserve return value in RAX
    let heap_vars = context.get_heap_allocated_vars(&added_variables);
    for var_name in &heap_vars {
        if let Some(slot) = context.get_variable(var_name) {
            instructions.push(IRInstruction::FreeLocal(slot));
        }
    }

    context.remove_variables(&added_variables);

    Ok(instructions)
}

/// Check if an expression produces a heap-allocated result
fn is_heap_allocating_expression(node: &Node) -> bool {
    match node {
        Node::List { root } if !root.is_empty() => {
            // Check if it's a call to a heap-allocating function
            if let Node::Symbol { value } = &root[0] {
                matches!(value.as_str(), "str")
            } else {
                false
            }
        }
        _ => false,
    }
}
