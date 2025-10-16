/// Compilation context for tracking variables, parameters, and functions
use crate::ir::FunctionInfo;
use std::collections::HashMap;

/// Context maintained during compilation
/// Tracks variables, parameters, and function definitions
#[derive(Debug, Clone)]
pub struct CompileContext {
    pub variables: HashMap<String, usize>,          // variable name -> local slot index
    pub heap_allocated_vars: HashMap<String, bool>, // tracks if variable holds heap pointer
    pub parameters: HashMap<String, usize>,         // parameter name -> param slot index
    pub functions: HashMap<String, FunctionInfo>,   // function name -> function info
    pub next_slot: usize,
    pub free_slots: Vec<usize>, // stack of freed slots for reuse
    pub in_function: bool,      // true when compiling inside a function
}

impl CompileContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            heap_allocated_vars: HashMap::new(),
            parameters: HashMap::new(),
            functions: HashMap::new(),
            next_slot: 0,
            free_slots: Vec::new(),
            in_function: false,
        }
    }

    /// Add a variable to the context and return its slot index
    pub fn add_variable(&mut self, name: String) -> usize {
        // Try to reuse a freed slot first
        let slot = if let Some(free_slot) = self.free_slots.pop() {
            free_slot
        } else {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        };
        self.variables.insert(name, slot);
        slot
    }

    /// Get the slot index for a variable
    pub fn get_variable(&self, name: &str) -> Option<usize> {
        self.variables.get(name).copied()
    }

    /// Add a parameter to the context
    pub fn add_parameter(&mut self, name: String, slot: usize) {
        self.parameters.insert(name, slot);
    }

    /// Get the slot index for a parameter
    pub fn get_parameter(&self, name: &str) -> Option<usize> {
        self.parameters.get(name).copied()
    }

    /// Add a function to the context
    pub fn add_function(&mut self, name: String, info: FunctionInfo) -> Result<(), super::CompileError> {
        if self.functions.contains_key(&name) {
            return Err(super::CompileError::DuplicateFunction(name));
        }
        self.functions.insert(name, info);
        Ok(())
    }

    /// Get function info by name
    pub fn get_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.get(name)
    }

    /// Remove a variable and free its slot for reuse
    pub fn remove_variable(&mut self, name: &str) -> Option<usize> {
        if let Some(slot) = self.variables.remove(name) {
            self.free_slots.push(slot);
            Some(slot)
        } else {
            None
        }
    }

    /// Remove multiple variables (for cleaning up let bindings)
    pub fn remove_variables(&mut self, names: &[String]) {
        for name in names {
            self.remove_variable(name);
        }
    }

    /// Mark a variable as holding a heap-allocated pointer
    pub fn mark_heap_allocated(&mut self, name: &str) {
        self.heap_allocated_vars.insert(name.to_string(), true);
    }

    /// Check if a variable holds a heap-allocated pointer
    pub fn is_heap_allocated(&self, name: &str) -> bool {
        self.heap_allocated_vars.get(name).copied().unwrap_or(false)
    }

    /// Get list of heap-allocated variables from a list of names
    pub fn get_heap_allocated_vars(&self, names: &[String]) -> Vec<String> {
        names.iter().filter(|name| self.is_heap_allocated(name)).cloned().collect()
    }
}
