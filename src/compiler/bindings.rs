use super::{CompileContext, CompileError};
/// Variable binding compilation (let expressions)
use crate::ast::Node;
use crate::ir::{IRInstruction, IRProgram};

/// Compile a let binding expression
pub fn compile_let(
    args: &[Node],
    context: &mut CompileContext,
    program: &mut IRProgram,
) -> Result<Vec<IRInstruction>, CompileError> {
    if args.len() != 2 {
        return Err(CompileError::ArityError("let".to_string(), 2, args.len()));
    }

    let bindings = match &args[0] {
        Node::Vector { root } => root,
        _ => {
            return Err(CompileError::InvalidExpression(
                "let requires a vector of bindings".to_string(),
            ))
        }
    };

    if bindings.len() % 2 != 0 {
        return Err(CompileError::InvalidExpression(
            "let bindings must have even number of elements".to_string(),
        ));
    }

    let mut instructions = Vec::new();
    let mut added_variables = Vec::new();

    for chunk in bindings.chunks(2) {
        let var_node = &chunk[0];
        let val_node = &chunk[1];

        let var_name = match var_node {
            Node::Symbol { value } => value,
            _ => {
                return Err(CompileError::InvalidExpression(
                    "let binding variables must be symbols".to_string(),
                ))
            }
        };

        instructions.extend(crate::compiler::compile_node(val_node, context, program)?);

        let slot = context.add_variable(var_name.clone());
        instructions.push(IRInstruction::StoreLocal(slot));
        added_variables.push(var_name.clone());
    }

    instructions.extend(crate::compiler::compile_node(&args[1], context, program)?);

    context.remove_variables(&added_variables);

    Ok(instructions)
}
