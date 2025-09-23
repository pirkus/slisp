#[derive(Debug, Clone, PartialEq)]
pub enum IRInstruction {
    // Stack operations
    Push(i64), // Push immediate value

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
    Not, // Pop one value, push logical NOT

    // Control flow
    JumpIfZero(usize), // Jump to instruction index if top of stack is 0
    Jump(usize),       // Unconditional jump to instruction index

    // Variable operations
    StoreLocal(usize), // Pop value and store in local variable slot
    LoadLocal(usize),  // Push value from local variable slot

    // Function operations
    DefineFunction(String, usize, usize), // (name, param_count, start_address)
    Call(String, usize),                  // (function_name, arg_count)
    CallIndirect(usize),                  // Call function at address (arg_count on stack)
    PushFrame(usize),                     // Create new stack frame with N local slots
    PopFrame,                             // Restore previous stack frame
    StoreParam(usize),                    // Store parameter in current frame
    LoadParam(usize),                     // Load parameter from current frame

    // Program flow
    Return, // Return top of stack as program result
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionInfo {
    pub name: String,
    pub param_count: usize,
    pub start_address: usize,
    pub local_count: usize, // Number of local variable slots needed
}

#[derive(Debug, Clone)]
pub struct IRProgram {
    pub instructions: Vec<IRInstruction>,
    pub functions: Vec<FunctionInfo>,
    pub entry_point: Option<String>, // Name of the main function
}

impl IRProgram {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            functions: Vec::new(),
            entry_point: None,
        }
    }

    pub fn add_instruction(&mut self, instruction: IRInstruction) {
        self.instructions.push(instruction);
    }

    pub fn add_function(&mut self, function: FunctionInfo) {
        self.functions.push(function);
    }

    pub fn set_entry_point(&mut self, name: String) {
        self.entry_point = Some(name);
    }

    pub fn get_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.iter().find(|f| f.name == name)
    }

    pub fn len(&self) -> usize {
        self.instructions.len()
    }
}
