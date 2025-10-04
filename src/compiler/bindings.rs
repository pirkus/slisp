/// Variable binding compilation (let expressions)

use crate::domain::Node;
use crate::ir::{IRInstruction, IRProgram};
use super::{CompileContext, CompileError};

/// Compile a let binding expression
pub fn compile_let(
    args: &[Box<Node>],
    program: &mut IRProgram,
    context: &mut CompileContext,
) -> Result<(), CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("let".to_string(), 2, args.len()));
    }

    // First argument should be a vector of bindings [var1 val1 var2 val2 ...]
    let bindings = match args[0].as_ref() {
        Node::Vector { root } => root,
        _ => {
            return Err(CompileError::InvalidExpression(
                "let requires a vector of bindings".to_string(),
            ))
        }
    };

    // Check that we have an even number of binding elements
    if bindings.len() % 2 != 0 {
        return Err(CompileError::InvalidExpression(
            "let bindings must have even number of elements".to_string(),
        ));
    }

    // Track variables added in this let scope for cleanup
    let mut added_variables = Vec::new();

    // Process bindings in pairs [var val var val ...]
    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        // Variable must be a symbol
        let var_name = match var_node.as_ref() {
            Node::Symbol { value } => value,
            _ => {
                return Err(CompileError::InvalidExpression(
                    "let binding variables must be symbols".to_string(),
                ))
            }
        };

        // Compile the value expression
        crate::compiler::compile_node(val_node, program, context)?;

        // Add variable to context and store it
        let slot = context.add_variable(var_name.clone());
        program.add_instruction(IRInstruction::StoreLocal(slot));
        added_variables.push(var_name.clone());
    }

    // Compile body in the new environment
    crate::compiler::compile_node(&args[1], program, context)?;

    // Clean up variables added in this scope (proper scoping and memory management)
    context.remove_variables(&added_variables);

    Ok(())
}
