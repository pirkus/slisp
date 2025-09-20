#[derive(Debug, Clone, PartialEq)]
pub enum IRInstruction {
    // Stack operations
    Push(i64), // Push immediate value
    Pop,       // Pop and discard top value

    // Arithmetic operations
    Add, // Pop two values, push sum
    Sub, // Pop two values, push difference (second - first)
    Mul, // Pop two values, push product
    Div, // Pop two values, push quotient (second / first)

    // Comparison operations
    Equal,        // Pop two values, push 1 if equal, 0 otherwise
    Less,         // Pop two values, push 1 if second < first, 0 otherwise
    Greater,      // Pop two values, push 1 if second > first, 0 otherwise
    LessEqual,    // Pop two values, push 1 if second <= first, 0 otherwise
    GreaterEqual, // Pop two values, push 1 if second >= first, 0 otherwise

    // Logical operations
    And, // Pop two values, push logical AND
    Or,  // Pop two values, push logical OR
    Not, // Pop one value, push logical NOT

    // Control flow
    JumpIfZero(usize), // Jump to instruction index if top of stack is 0
    Jump(usize),       // Unconditional jump to instruction index

    // Program flow
    Return, // Return top of stack as program result
}

#[derive(Debug, Clone)]
pub struct IRProgram {
    pub instructions: Vec<IRInstruction>,
}

impl IRProgram {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
        }
    }

    pub fn add_instruction(&mut self, instruction: IRInstruction) {
        self.instructions.push(instruction);
    }

    pub fn len(&self) -> usize {
        self.instructions.len()
    }
}
