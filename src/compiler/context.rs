/// Compilation context for tracking variables, parameters, and functions
use super::{
    inference::{BindingOwner, FunctionKey, TypeInferenceSummary},
    HeapOwnership, MapValueTypes, ValueKind,
};
use crate::ir::FunctionInfo;
use std::collections::HashMap;

/// Context maintained during compilation
/// Tracks variables, parameters, and function definitions
#[derive(Debug, Clone)]
pub struct CompileContext {
    pub variables: HashMap<String, usize>,                                                // variable name -> local slot index
    pub heap_allocated_vars: HashMap<String, bool>,                                       // tracks if variable holds heap pointer
    pub variable_types: HashMap<String, ValueKind>,                                       // tracks inferred variable types
    pub variable_map_value_types: HashMap<String, MapValueTypes>,                         // tracks map entry metadata for locals
    pub variable_set_element_kinds: HashMap<String, ValueKind>,                           // set element kinds for locals
    pub variable_vector_element_kinds: HashMap<String, ValueKind>,                        // vector element kinds for locals
    pub parameters: HashMap<String, usize>,                                               // parameter name -> param slot index
    pub parameter_types: HashMap<String, ValueKind>,                                      // inferred parameter types
    pub parameter_map_value_types: HashMap<String, MapValueTypes>,                        // map metadata for parameters
    pub parameter_set_element_kinds: HashMap<String, ValueKind>,                          // set element kinds for parameters
    pub parameter_vector_element_kinds: HashMap<String, ValueKind>,                       // vector element kinds for parameters
    pub functions: HashMap<String, FunctionInfo>,                                         // function name -> function info
    pub function_return_types: HashMap<String, ValueKind>,                                // function name -> return kind
    pub function_return_map_value_types: HashMap<String, MapValueTypes>,                  // function name -> map metadata
    pub function_return_set_element_kinds: HashMap<String, ValueKind>,                    // function name -> set element kind
    pub function_return_vector_element_kinds: HashMap<String, ValueKind>,                 // function name -> vector element kind
    pub function_parameter_types: HashMap<String, Vec<ValueKind>>,                        // function name -> parameter kinds
    pub function_parameter_map_value_types: HashMap<String, Vec<Option<MapValueTypes>>>,  // function name -> parameter map metadata
    pub function_parameter_set_element_kinds: HashMap<String, Vec<Option<ValueKind>>>,    // function name -> parameter set element kind
    pub function_parameter_vector_element_kinds: HashMap<String, Vec<Option<ValueKind>>>, // function name -> parameter vector element kind
    pub function_return_ownership: HashMap<String, HeapOwnership>,                        // function name -> heap ownership semantics
    pub type_inference: Option<TypeInferenceSummary>,                                     // cached inference summary for current compilation unit
    pub current_function: FunctionKey,
    pub local_binding_offsets: HashMap<FunctionKey, usize>,
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
            variable_map_value_types: HashMap::new(),
            variable_set_element_kinds: HashMap::new(),
            variable_vector_element_kinds: HashMap::new(),
            parameters: HashMap::new(),
            parameter_types: HashMap::new(),
            parameter_map_value_types: HashMap::new(),
            parameter_set_element_kinds: HashMap::new(),
            parameter_vector_element_kinds: HashMap::new(),
            functions: HashMap::new(),
            function_return_types: HashMap::new(),
            function_return_map_value_types: HashMap::new(),
            function_return_set_element_kinds: HashMap::new(),
            function_return_vector_element_kinds: HashMap::new(),
            function_parameter_types: HashMap::new(),
            function_parameter_map_value_types: HashMap::new(),
            function_parameter_set_element_kinds: HashMap::new(),
            function_parameter_vector_element_kinds: HashMap::new(),
            function_return_ownership: HashMap::new(),
            type_inference: None,
            current_function: FunctionKey::Program,
            local_binding_offsets: HashMap::new(),
            next_slot: 0,
            free_slots: Vec::new(),
            in_function: false,
        }
    }

    pub fn absorb_parameter_inference(&mut self, other: &CompileContext) {
        for (name, params) in &other.function_parameter_types {
            for (idx, kind) in params.iter().enumerate() {
                if *kind != ValueKind::Any {
                    self.record_function_parameter_type(name, idx, *kind);
                }
            }
        }
    }

    /// Create a fresh context for compiling a function body.
    ///
    /// The returned context shares the function table with the parent context
    /// but clears out any function-local state such as variables, parameters,
    /// heap-allocation markers, and slot tracking. This ensures that newly
    /// added fields to the compilation context receive the appropriate
    /// initialization for function scopes in one place.
    pub fn new_function_scope(&self, function_name: &str) -> Self {
        let key = FunctionKey::Named(function_name.to_string());
        let mut local_binding_offsets = self.local_binding_offsets.clone();
        local_binding_offsets.insert(key.clone(), 0);
        Self {
            variables: HashMap::new(),
            heap_allocated_vars: HashMap::new(),
            variable_types: HashMap::new(),
            variable_map_value_types: HashMap::new(),
            variable_set_element_kinds: HashMap::new(),
            variable_vector_element_kinds: HashMap::new(),
            parameters: HashMap::new(),
            parameter_types: HashMap::new(),
            parameter_map_value_types: HashMap::new(),
            parameter_set_element_kinds: HashMap::new(),
            parameter_vector_element_kinds: HashMap::new(),
            functions: self.functions.clone(),
            function_return_types: self.function_return_types.clone(),
            function_return_map_value_types: self.function_return_map_value_types.clone(),
            function_return_set_element_kinds: self.function_return_set_element_kinds.clone(),
            function_return_vector_element_kinds: self.function_return_vector_element_kinds.clone(),
            function_parameter_types: self.function_parameter_types.clone(),
            function_parameter_map_value_types: self.function_parameter_map_value_types.clone(),
            function_parameter_set_element_kinds: self.function_parameter_set_element_kinds.clone(),
            function_parameter_vector_element_kinds: self.function_parameter_vector_element_kinds.clone(),
            function_return_ownership: self.function_return_ownership.clone(),
            type_inference: self.type_inference.clone(),
            current_function: key,
            local_binding_offsets,
            next_slot: 0,
            free_slots: Vec::new(),
            in_function: true,
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

    /// Set the inferred type for a variable
    pub fn set_variable_type(&mut self, name: &str, kind: ValueKind) {
        self.variable_types.insert(name.to_string(), kind);
    }

    pub fn set_variable_map_value_types(&mut self, name: &str, types: Option<MapValueTypes>) {
        match types {
            Some(map) if !map.is_empty() => {
                self.variable_map_value_types.insert(name.to_string(), map);
            }
            _ => {
                self.variable_map_value_types.remove(name);
            }
        }
    }

    pub fn set_variable_set_element_kind(&mut self, name: &str, kind: Option<ValueKind>) {
        match kind {
            Some(kind) => {
                self.variable_set_element_kinds.insert(name.to_string(), kind);
            }
            None => {
                self.variable_set_element_kinds.remove(name);
            }
        }
    }

    pub fn set_variable_vector_element_kind(&mut self, name: &str, kind: Option<ValueKind>) {
        match kind {
            Some(kind) => {
                self.variable_vector_element_kinds.insert(name.to_string(), kind);
            }
            None => {
                self.variable_vector_element_kinds.remove(name);
            }
        }
    }

    pub fn get_variable_map_value_types(&self, name: &str) -> Option<&MapValueTypes> {
        self.variable_map_value_types.get(name)
    }

    pub fn get_variable_set_element_kind(&self, name: &str) -> Option<ValueKind> {
        self.variable_set_element_kinds.get(name).copied()
    }

    pub fn get_variable_vector_element_kind(&self, name: &str) -> Option<ValueKind> {
        self.variable_vector_element_kinds.get(name).copied()
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
        self.heap_allocated_vars.entry(name.clone()).or_insert(false);
        self.parameter_set_element_kinds.remove(&name);
        self.parameter_vector_element_kinds.remove(&name);
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

    pub fn set_parameter_map_value_types(&mut self, name: &str, types: Option<MapValueTypes>) {
        match types {
            Some(map) if !map.is_empty() => {
                self.parameter_map_value_types.insert(name.to_string(), map);
            }
            _ => {
                self.parameter_map_value_types.remove(name);
            }
        }
    }

    pub fn get_parameter_map_value_types(&self, name: &str) -> Option<&MapValueTypes> {
        self.parameter_map_value_types.get(name)
    }

    pub fn set_parameter_set_element_kind(&mut self, name: &str, kind: Option<ValueKind>) {
        match kind {
            Some(kind) if kind != ValueKind::Any => {
                self.parameter_set_element_kinds.insert(name.to_string(), kind);
            }
            _ => {
                self.parameter_set_element_kinds.remove(name);
            }
        }
    }

    pub fn set_parameter_vector_element_kind(&mut self, name: &str, kind: Option<ValueKind>) {
        match kind {
            Some(kind) if kind != ValueKind::Any => {
                self.parameter_vector_element_kinds.insert(name.to_string(), kind);
            }
            _ => {
                self.parameter_vector_element_kinds.remove(name);
            }
        }
    }

    pub fn get_parameter_set_element_kind(&self, name: &str) -> Option<ValueKind> {
        self.parameter_set_element_kinds.get(name).copied()
    }

    pub fn get_parameter_vector_element_kind(&self, name: &str) -> Option<ValueKind> {
        self.parameter_vector_element_kinds.get(name).copied()
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

    pub fn set_function_return_map_value_types(&mut self, name: &str, types: Option<MapValueTypes>) {
        match types {
            Some(map) if !map.is_empty() => {
                self.function_return_map_value_types.insert(name.to_string(), map);
            }
            _ => {
                self.function_return_map_value_types.remove(name);
            }
        }
    }

    pub fn set_function_return_set_element_kind(&mut self, name: &str, kind: Option<ValueKind>) {
        match kind {
            Some(kind) if kind != ValueKind::Any => {
                self.function_return_set_element_kinds.insert(name.to_string(), kind);
            }
            _ => {
                self.function_return_set_element_kinds.remove(name);
            }
        }
    }

    pub fn set_function_return_vector_element_kind(&mut self, name: &str, kind: Option<ValueKind>) {
        match kind {
            Some(kind) if kind != ValueKind::Any => {
                self.function_return_vector_element_kinds.insert(name.to_string(), kind);
            }
            _ => {
                self.function_return_vector_element_kinds.remove(name);
            }
        }
    }

    pub fn get_function_return_map_value_types(&self, name: &str) -> Option<&MapValueTypes> {
        self.function_return_map_value_types.get(name)
    }

    pub fn get_function_return_set_element_kind(&self, name: &str) -> Option<ValueKind> {
        self.function_return_set_element_kinds.get(name).copied()
    }

    pub fn get_function_return_vector_element_kind(&self, name: &str) -> Option<ValueKind> {
        self.function_return_vector_element_kinds.get(name).copied()
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

    /// Attach a precomputed type inference summary to the context.
    pub fn set_type_inference(&mut self, summary: TypeInferenceSummary) {
        self.type_inference = Some(summary);
        self.local_binding_offsets.clear();
    }

    /// Retrieve the shared type inference summary if one has been installed.
    #[allow(dead_code)]
    pub fn type_inference(&self) -> Option<&TypeInferenceSummary> {
        self.type_inference.as_ref()
    }

    /// Hydrate function metadata from the attached type inference summary.
    pub fn hydrate_from_inference(&mut self) {
        let Some(summary) = self.type_inference.clone() else {
            return;
        };

        for (name, analysis) in summary.iter_named_functions() {
            if let Some(return_binding) = analysis.return_binding {
                if let Some(kind) = summary.binding_kind(return_binding) {
                    self.set_function_return_type(name, kind);
                }
                if let Some(ownership) = summary.binding_ownership(return_binding) {
                    self.set_function_return_ownership(name, ownership);
                }
                if let Some(map_types) = summary.binding_map_value_types(return_binding).cloned() {
                    self.set_function_return_map_value_types(name, Some(map_types));
                }
                if let Some(set_element_kind) = summary.binding_set_element_kind(return_binding) {
                    self.set_function_return_set_element_kind(name, Some(set_element_kind));
                }
                if let Some(vector_element_kind) = summary.binding_vector_element_kind(return_binding) {
                    self.set_function_return_vector_element_kind(name, Some(vector_element_kind));
                }
            }

            for (idx, binding_id) in analysis.parameter_bindings.iter().enumerate() {
                if let Some(kind) = summary.binding_kind(*binding_id) {
                    self.record_function_parameter_type(name, idx, kind);
                }
                if let Some(map) = summary.binding_map_value_types(*binding_id).cloned() {
                    self.set_function_parameter_map_value_types(name, idx, Some(map));
                }
                if let Some(set_element_kind) = summary.binding_set_element_kind(*binding_id) {
                    self.set_function_parameter_set_element_kind(name, idx, Some(set_element_kind));
                }
                if let Some(vector_element_kind) = summary.binding_vector_element_kind(*binding_id) {
                    self.set_function_parameter_vector_element_kind(name, idx, Some(vector_element_kind));
                }
            }
        }
    }

    pub fn consume_local_binding_metadata(&mut self, var_name: &str) -> Option<(ValueKind, HeapOwnership, Option<MapValueTypes>, Option<ValueKind>, Option<ValueKind>)> {
        let summary = self.type_inference.as_ref()?;
        let analysis = summary.function(&self.current_function)?;
        let offset = self.local_binding_offsets.entry(self.current_function.clone()).or_insert(0);
        if *offset >= analysis.local_bindings.len() {
            return None;
        }
        while *offset < analysis.local_bindings.len() {
            let binding_id = analysis.local_bindings[*offset];
            *offset += 1;
            let binding = summary.binding(binding_id)?;
            if let BindingOwner::Local { name, .. } = &binding.owner {
                if name == var_name {
                    let map_value_types = summary.binding_map_value_types(binding_id).cloned();
                    let set_element_kind = summary.binding_set_element_kind(binding_id);
                    let vector_element_kind = summary.binding_vector_element_kind(binding_id);
                    return Some((binding.value_kind, binding.heap_ownership, map_value_types, set_element_kind, vector_element_kind));
                }
            }
        }
        None
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

    pub fn set_function_parameter_map_value_types(&mut self, name: &str, index: usize, types: Option<MapValueTypes>) {
        let entry = self.function_parameter_map_value_types.entry(name.to_string()).or_insert_with(Vec::new);
        if entry.len() <= index {
            entry.resize(index + 1, None);
        }
        match types {
            Some(map) if !map.is_empty() => entry[index] = Some(map),
            _ => entry[index] = None,
        }
    }

    pub fn get_function_parameter_map_value_types(&self, name: &str, index: usize) -> Option<&MapValueTypes> {
        self.function_parameter_map_value_types.get(name).and_then(|values| values.get(index)).and_then(|slot| slot.as_ref())
    }

    pub fn set_function_parameter_set_element_kind(&mut self, name: &str, index: usize, kind: Option<ValueKind>) {
        let entry = self.function_parameter_set_element_kinds.entry(name.to_string()).or_insert_with(Vec::new);
        if entry.len() <= index {
            entry.resize(index + 1, None);
        }
        match kind {
            Some(kind) if kind != ValueKind::Any => entry[index] = Some(kind),
            _ => entry[index] = None,
        }
    }

    pub fn get_function_parameter_set_element_kind(&self, name: &str, index: usize) -> Option<ValueKind> {
        self.function_parameter_set_element_kinds
            .get(name)
            .and_then(|values| values.get(index))
            .and_then(|slot| slot.as_ref())
            .copied()
    }

    pub fn set_function_parameter_vector_element_kind(&mut self, name: &str, index: usize, kind: Option<ValueKind>) {
        let entry = self.function_parameter_vector_element_kinds.entry(name.to_string()).or_insert_with(Vec::new);
        if entry.len() <= index {
            entry.resize(index + 1, None);
        }
        match kind {
            Some(kind) if kind != ValueKind::Any => entry[index] = Some(kind),
            _ => entry[index] = None,
        }
    }

    pub fn get_function_parameter_vector_element_kind(&self, name: &str, index: usize) -> Option<ValueKind> {
        self.function_parameter_vector_element_kinds
            .get(name)
            .and_then(|values| values.get(index))
            .and_then(|slot| slot.as_ref())
            .copied()
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
            self.variable_map_value_types.remove(name);
            self.variable_set_element_kinds.remove(name);
            self.variable_vector_element_kinds.remove(name);
            Some(slot)
        } else {
            None
        }
    }

    /// Remove multiple variables (for cleaning up let bindings)
    pub fn remove_variables(&mut self, names: &[String]) {
        names.iter().for_each(|name| {
            self.remove_variable(name);
        });
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
            let popped: Vec<usize> = (0..count).filter_map(|_| self.free_slots.pop()).collect();

            if popped.len() == count {
                let mut sorted = popped.clone();
                sorted.sort_unstable();
                let contiguous = sorted.windows(2).all(|window| window[1] == window[0] + 1);
                if contiguous {
                    return sorted;
                }

                // Not contiguous – restore the slots for future reuse.
                popped.into_iter().rev().for_each(|slot| self.free_slots.push(slot));
            } else {
                // Ran out of free slots; restore any we popped.
                popped.into_iter().rev().for_each(|slot| self.free_slots.push(slot));
            }
        }

        let start = self.next_slot;
        self.next_slot += count;
        (start..start + count).collect()
    }

    /// Return a temporary slot to the pool for reuse.
    pub fn release_temp_slot(&mut self, slot: usize) {
        self.free_slots.push(slot);
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
    use crate::ast::{AstParser, AstParserTrt};
    use crate::compiler::inference::run_type_inference;
    use crate::compiler::MapKeyLiteral;

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

        let function_context = context.new_function_scope("foo");

        assert!(function_context.in_function);
        assert!(function_context.variables.is_empty());
        assert!(function_context.parameters.is_empty());
        assert!(function_context.heap_allocated_vars.is_empty());
        assert!(function_context.variable_types.is_empty());
        assert!(function_context.parameter_types.is_empty());
        assert!(function_context.free_slots.is_empty());
        assert_eq!(function_context.next_slot, 0);
        assert_eq!(function_context.functions, context.functions);
        assert!(matches!(function_context.current_function, FunctionKey::Named(ref name) if name == "foo"));
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

    #[test]
    fn hydrate_from_inference_populates_function_metadata() {
        let foo = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn foo [x] x)".as_bytes(), &mut offset)
        };
        let caller = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn caller [] (foo 1))".as_bytes(), &mut offset)
        };
        let program = vec![foo, caller];
        let summary = run_type_inference(&program).unwrap();

        let mut context = CompileContext::new();
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

        context.set_type_inference(summary);
        context.hydrate_from_inference();

        assert_eq!(context.get_function_parameter_type("foo", 0), Some(ValueKind::Number));
        assert_eq!(context.get_function_return_type("foo"), Some(ValueKind::Number));
        assert_eq!(context.get_function_return_ownership("foo"), Some(HeapOwnership::None));
    }

    #[test]
    fn consume_local_binding_metadata_tracks_program_locals() {
        let mut offset = 0;
        let expr = AstParser::parse_sexp_new_domain("(let [m {\"a\" 1} y 1] m)".as_bytes(), &mut offset);
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();

        let mut context = CompileContext::new();
        context.set_type_inference(summary);
        context.hydrate_from_inference();

        let (map_kind, map_owner, map_metadata, set_kind, vector_kind) = context.consume_local_binding_metadata("m").unwrap();
        assert_eq!(map_kind, ValueKind::Map);
        assert_eq!(map_owner, HeapOwnership::Owned);
        let metadata = map_metadata.expect("expected map metadata");
        assert_eq!(metadata.get(&MapKeyLiteral::String("a".to_string())), Some(&ValueKind::Number));
        assert_eq!(set_kind, None);
        assert_eq!(vector_kind, None);

        let (number_kind, number_owner, _, _, _) = context.consume_local_binding_metadata("y").unwrap();
        assert_eq!(number_kind, ValueKind::Number);
        assert_eq!(number_owner, HeapOwnership::None);
    }

    #[test]
    fn hydrate_records_parameter_map_metadata() {
        let foo = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn foo [m] (get m \"a\"))".as_bytes(), &mut offset)
        };
        let caller = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn caller [] (foo {\"a\" \"x\"}))".as_bytes(), &mut offset)
        };
        let program = vec![foo, caller];
        let summary = run_type_inference(&program).unwrap();

        let mut context = CompileContext::new();
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

        context.set_type_inference(summary);
        context.hydrate_from_inference();

        let metadata = context.get_function_parameter_map_value_types("foo", 0).expect("expected map metadata");
        assert_eq!(metadata.get(&MapKeyLiteral::String("a".to_string())), Some(&ValueKind::String));
    }

    #[test]
    fn hydrate_records_set_and_vector_parameter_metadata() {
        let foo = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn foo [s v] (get v 0))".as_bytes(), &mut offset)
        };
        let caller = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn caller [] (foo #{1 2} [\"a\" \"b\"]))".as_bytes(), &mut offset)
        };
        let program = vec![foo, caller];
        let summary = run_type_inference(&program).unwrap();
        let foo_analysis = summary.function(&FunctionKey::Named("foo".to_string())).unwrap();
        let set_param = foo_analysis.parameter_bindings[0];
        let vec_param = foo_analysis.parameter_bindings[1];
        assert_eq!(summary.binding_set_element_kind(set_param), Some(ValueKind::Number));
        assert_eq!(summary.binding_vector_element_kind(vec_param), Some(ValueKind::String));

        let mut context = CompileContext::new();
        context
            .add_function(
                "foo".to_string(),
                FunctionInfo {
                    name: "foo".to_string(),
                    param_count: 2,
                    start_address: 0,
                    local_count: 0,
                },
            )
            .unwrap();

        context.set_type_inference(summary);
        context.hydrate_from_inference();

        assert_eq!(context.get_function_parameter_set_element_kind("foo", 0), Some(ValueKind::Number));
        assert_eq!(context.get_function_parameter_vector_element_kind("foo", 1), Some(ValueKind::String));
        assert_eq!(context.get_function_return_type("foo"), Some(ValueKind::String));
        assert_eq!(context.get_function_return_vector_element_kind("foo"), None);
    }

    #[test]
    fn hydrate_records_set_return_metadata() {
        let make = {
            let mut offset = 0;
            AstParser::parse_sexp_new_domain("(defn make [] #{:a})".as_bytes(), &mut offset)
        };
        let program = vec![make];
        let summary = run_type_inference(&program).unwrap();

        let mut context = CompileContext::new();
        context
            .add_function(
                "make".to_string(),
                FunctionInfo {
                    name: "make".to_string(),
                    param_count: 0,
                    start_address: 0,
                    local_count: 0,
                },
            )
            .unwrap();

        context.set_type_inference(summary);
        context.hydrate_from_inference();

        assert_eq!(context.get_function_return_set_element_kind("make"), Some(ValueKind::Keyword));
    }

    #[test]
    fn variable_element_kind_setters_round_trip() {
        let mut context = CompileContext::new();
        context.set_variable_set_element_kind("s", Some(ValueKind::Number));
        context.set_variable_vector_element_kind("v", Some(ValueKind::String));

        assert_eq!(context.get_variable_set_element_kind("s"), Some(ValueKind::Number));
        assert_eq!(context.get_variable_vector_element_kind("v"), Some(ValueKind::String));
    }
}
