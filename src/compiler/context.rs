/// Compilation context for tracking variables, parameters, and functions
use super::{HeapOwnership, ValueKind};
use crate::ir::FunctionInfo;
use std::collections::HashMap;

/// Context maintained during compilation
/// Tracks variables, parameters, and function definitions
#[derive(Debug, Clone)]
pub struct CompileContext {
    pub variables: HashMap<String, usize>,                         // variable name -> local slot index
    pub heap_allocated_vars: HashMap<String, bool>,                // tracks if variable holds heap pointer
    pub variable_types: HashMap<String, ValueKind>,                // tracks inferred variable types
    pub parameters: HashMap<String, usize>,                        // parameter name -> param slot index
    pub parameter_types: HashMap<String, ValueKind>,               // inferred parameter types
    pub functions: HashMap<String, FunctionInfo>,                  // function name -> function info
    pub function_return_types: HashMap<String, ValueKind>,         // function name -> return kind
    pub function_parameter_types: HashMap<String, Vec<ValueKind>>, // function name -> parameter kinds
    pub function_return_ownership: HashMap<String, HeapOwnership>, // function name -> heap ownership semantics
    pub next_slot: usize,
    pub free_slots: Vec<usize>, // stack of freed slots for reuse
    pub in_function: bool,      // true when compiling inside a function
}

impl CompileContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            heap_allocated_vars: HashMap::new(),
            variable_types: HashMap::new(),
            parameters: HashMap::new(),
            parameter_types: HashMap::new(),
            functions: HashMap::new(),
            function_return_types: HashMap::new(),
            function_parameter_types: HashMap::new(),
            function_return_ownership: HashMap::new(),
            next_slot: 0,
            free_slots: Vec::new(),
            in_function: false,
        }
    }

    /// Create a fresh context for compiling a function body.
    ///
    /// The returned context shares the function table with the parent context
    /// but clears out any function-local state such as variables, parameters,
    /// heap-allocation markers, and slot tracking. This ensures that newly
    /// added fields to the compilation context receive the appropriate
    /// initialization for function scopes in one place.
    pub fn new_function_scope(&self) -> Self {
        Self {
            variables: HashMap::new(),
            heap_allocated_vars: HashMap::new(),
            variable_types: HashMap::new(),
            parameters: HashMap::new(),
            parameter_types: HashMap::new(),
            functions: self.functions.clone(),
            function_return_types: self.function_return_types.clone(),
            function_parameter_types: self.function_parameter_types.clone(),
            function_return_ownership: self.function_return_ownership.clone(),
            next_slot: 0,
            free_slots: Vec::new(),
            in_function: true,
        }
    }

    /// Add a variable to the context and return its slot index
    ///
    /// Variables get fresh slots and never reuse temp slots from free_slots
    /// to prevent use-after-free bugs where a temp's value is overwritten
    /// while still being referenced.
    pub fn add_variable(&mut self, name: String) -> usize {
        let slot = self.next_slot;
        self.next_slot += 1;
        self.variables.insert(name, slot);
        slot
    }

    /// Set the inferred type for a variable
    pub fn set_variable_type(&mut self, name: &str, kind: ValueKind) {
        self.variable_types.insert(name.to_string(), kind);
    }

    /// Get the inferred type for a variable
    pub fn get_variable_type(&self, name: &str) -> Option<ValueKind> {
        self.variable_types.get(name).copied()
    }

    /// Get the slot index for a variable
    pub fn get_variable(&self, name: &str) -> Option<usize> {
        self.variables.get(name).copied()
    }

    /// Add a parameter to the context
    pub fn add_parameter(&mut self, name: String, slot: usize) {
        self.parameters.insert(name.clone(), slot);
        self.parameter_types.insert(name.clone(), ValueKind::Any);
        self.heap_allocated_vars.entry(name).or_insert(false);
    }

    /// Get the slot index for a parameter
    pub fn get_parameter(&self, name: &str) -> Option<usize> {
        self.parameters.get(name).copied()
    }

    /// Get the inferred type for a parameter
    pub fn get_parameter_type(&self, name: &str) -> Option<ValueKind> {
        self.parameter_types.get(name).copied()
    }

    /// Set the inferred type for a parameter
    pub fn set_parameter_type(&mut self, name: &str, kind: ValueKind) {
        self.parameter_types.insert(name.to_string(), kind);
    }

    /// Add a function to the context
    pub fn add_function(&mut self, name: String, info: FunctionInfo) -> Result<(), super::CompileError> {
        debug_assert!(!self.in_function, "function declarations must be registered on the root context");
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

    /// Record the return type inferred for a function
    pub fn set_function_return_type(&mut self, name: &str, kind: ValueKind) {
        if kind != ValueKind::Any {
            self.function_return_types.insert(name.to_string(), kind);
        }
    }

    pub fn set_function_return_ownership(&mut self, name: &str, ownership: HeapOwnership) {
        self.function_return_ownership.insert(name.to_string(), ownership);
    }

    /// Retrieve the inferred return type for a function if one is known
    pub fn get_function_return_type(&self, name: &str) -> Option<ValueKind> {
        self.function_return_types.get(name).copied()
    }

    /// Retrieve the heap ownership semantics of a function return if known
    pub fn get_function_return_ownership(&self, name: &str) -> Option<HeapOwnership> {
        self.function_return_ownership.get(name).copied()
    }

    /// Update the inferred parameter type for a function at a specific position
    pub fn record_function_parameter_type(&mut self, name: &str, index: usize, kind: ValueKind) {
        if kind == ValueKind::Any {
            return;
        }

        let entry = self.function_parameter_types.entry(name.to_string()).or_insert_with(Vec::new);
        if entry.len() <= index {
            entry.resize(index + 1, ValueKind::Any);
        }

        let slot = &mut entry[index];
        if *slot == ValueKind::Any {
            *slot = kind;
        } else if *slot != kind {
            *slot = ValueKind::Any;
        }
    }

    /// Get the recorded parameter types for a function if available
    pub fn get_function_parameter_type(&self, name: &str, index: usize) -> Option<ValueKind> {
        self.function_parameter_types.get(name).and_then(|params| params.get(index)).copied()
    }

    /// Remove a variable and free its slot for reuse
    pub fn remove_variable(&mut self, name: &str) -> Option<usize> {
        if let Some(slot) = self.variables.remove(name) {
            self.free_slots.push(slot);
            self.variable_types.remove(name);
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

    /// Allocate an anonymous temporary slot.
    ///
    /// Temps share the same slot pool as named locals; callers must
    /// release the slot via `release_temp_slot` once the temporary value is
    /// no longer needed so it can be reused.
    pub fn allocate_temp_slot(&mut self) -> usize {
        if let Some(slot) = self.free_slots.pop() {
            slot
        } else {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        }
    }

    /// Allocate a set of temporary slots that occupy consecutive positions.
    ///
    /// This is required for operations that treat locals as an array (e.g.
    /// building the pointer list for `_string_concat_n`). The allocator reuses
    /// contiguous runs from `free_slots` when possible and falls back to carving
    /// out a fresh block.
    pub fn allocate_contiguous_temp_slots(&mut self, count: usize) -> Vec<usize> {
        if count == 0 {
            return Vec::new();
        }

        if count == 1 {
            return vec![self.allocate_temp_slot()];
        }

        if self.free_slots.len() >= count {
            let mut popped = Vec::with_capacity(count);
            for _ in 0..count {
                if let Some(slot) = self.free_slots.pop() {
                    popped.push(slot);
                } else {
                    break;
                }
            }

            if popped.len() == count {
                let mut sorted = popped.clone();
                sorted.sort_unstable();
                let contiguous = sorted.windows(2).all(|window| window[1] == window[0] + 1);
                if contiguous {
                    return sorted;
                }

                // Not contiguous – restore the slots for future reuse.
                for slot in popped.into_iter().rev() {
                    self.free_slots.push(slot);
                }
            } else {
                // Ran out of free slots; restore any we popped.
                for slot in popped.into_iter().rev() {
                    self.free_slots.push(slot);
                }
            }
        }

        let start = self.next_slot;
        self.next_slot += count;
        (start..start + count).collect()
    }

    /// Return a temporary slot to the pool for reuse.
    ///
    /// DISABLED: Immediately releasing temp slots causes use-after-free bugs
    /// where a slot is reused while its value is still needed by later code.
    /// Instead, we let temp slots accumulate and they're reclaimed when the
    /// function scope ends. This uses more stack space but prevents corruption.
    pub fn release_temp_slot(&mut self, _slot: usize) {
        // Intentionally do nothing - don't add to free_slots
    }

    /// Mark a variable as holding a heap-allocated pointer
    pub fn mark_heap_allocated(&mut self, name: &str, kind: ValueKind) {
        self.heap_allocated_vars.insert(name.to_string(), true);
        if matches!(kind, ValueKind::String | ValueKind::Vector | ValueKind::Map | ValueKind::Set) {
            if self.variables.contains_key(name) {
                self.variable_types.insert(name.to_string(), kind);
            } else if self.parameters.contains_key(name) {
                self.parameter_types.insert(name.to_string(), kind);
            }
        }
    }

    /// Check if a variable holds a heap-allocated pointer
    pub fn is_heap_allocated(&self, name: &str) -> bool {
        self.heap_allocated_vars.get(name).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_scope_resets_local_state() {
        let mut context = CompileContext::new();

        context.add_variable("x".to_string());
        context.mark_heap_allocated("x", ValueKind::String);
        context.add_parameter("y".to_string(), 0);
        context
            .add_function(
                "foo".to_string(),
                FunctionInfo {
                    name: "foo".to_string(),
                    param_count: 1,
                    start_address: 0,
                    local_count: 0,
                },
            )
            .unwrap();
        context.next_slot = 5;
        context.free_slots.push(3);

        let function_context = context.new_function_scope();

        assert!(function_context.in_function);
        assert!(function_context.variables.is_empty());
        assert!(function_context.parameters.is_empty());
        assert!(function_context.heap_allocated_vars.is_empty());
        assert!(function_context.variable_types.is_empty());
        assert!(function_context.parameter_types.is_empty());
        assert!(function_context.free_slots.is_empty());
        assert_eq!(function_context.next_slot, 0);
        assert_eq!(function_context.functions, context.functions);
    }

    #[test]
    fn contiguous_temp_slots_reuse_and_extend() {
        let mut context = CompileContext::new();

        // Fresh allocation should yield consecutive slots starting at zero.
        let first = context.allocate_contiguous_temp_slots(3);
        assert_eq!(first, vec![0, 1, 2]);

        for slot in first.iter().rev() {
            context.release_temp_slot(*slot);
        }

        // Reusing should pick the freed run instead of extending the frame.
        let second = context.allocate_contiguous_temp_slots(3);
        assert_eq!(second, vec![0, 1, 2]);

        for slot in second.iter().rev() {
            context.release_temp_slot(*slot);
        }

        // Simulate fragmented free slots; expect a fresh contiguous block.
        context.free_slots.clear();
        context.free_slots.extend([0, 2, 4]);
        context.next_slot = 6;

        let third = context.allocate_contiguous_temp_slots(2);
        assert_eq!(third, vec![6, 7]);
    }
}
