#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // InitHeap and Allocate variants are used but clippy doesn't track through derived traits
pub enum IRInstruction {
    // Stack operations
    Push(i64),         // Push immediate value
    PushString(usize), // Push string address (index into string table)

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
    LoadParam(usize),                     // Load parameter from current frame

    // Memory allocation
    InitHeap,        // Initialize heap (mmap syscall to get memory region)
    Allocate(usize), // Allocate N bytes, push address onto stack
    Free,            // Pop address from stack and free it

    // Runtime function calls
    RuntimeCall(String, usize), // (function_name, arg_count) - Call a runtime support function

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
    pub entry_point: Option<String>,  // Name of the main function
    pub string_literals: Vec<String>, // String literals in the program
}

impl IRProgram {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            functions: Vec::new(),
            entry_point: None,
            string_literals: Vec::new(),
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

    /// Add a string literal and return its index
    pub fn add_string(&mut self, s: String) -> usize {
        if let Some(index) = self
            .string_literals
            .iter()
            .position(|existing| existing == &s)
        {
            return index;
        }

        let index = self.string_literals.len();
        self.string_literals.push(s);
        index
    }

    pub fn len(&self) -> usize {
        self.instructions.len()
    }
}
