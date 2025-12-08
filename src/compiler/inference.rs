use std::collections::HashMap;

use crate::ast::{Node, Primitive};

use super::{MapKeyLiteral, MapValueTypes, CompileError, HeapOwnership, ValueKind};

/// Execute the type inference scaffolding over a list of AST expressions.
///
/// The current implementation focuses on building the reusable data model that
/// later inference stages can extend with real constraints. It walks the AST,
/// registers bindings for parameters/locals/returns, and spins a fixpoint loop
/// that constraint implementations can hook into. Future 6.5.x work will plug
/// concrete constraints into the `constraints` vector so the solver can drive
/// `ValueKind` assignments away from `Any`.
pub fn run_type_inference(expressions: &[Node]) -> Result<TypeInferenceSummary, CompileError> {
    let mut builder = GraphBuilder::new();
    builder.build(expressions);
    let graph = builder.finish();
    let mut engine = TypeInferenceEngine::new(graph);
    engine.solve();
    Ok(engine.into_summary())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FunctionKey {
    Program,
    Named(String),
}

impl FunctionKey {
    fn program() -> Self {
        FunctionKey::Program
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AstId(Vec<usize>);

impl AstId {
    fn root() -> Self {
        AstId(Vec::new())
    }

    fn push(&mut self, index: usize) {
        self.0.push(index);
    }

    fn pop(&mut self) {
        self.0.pop();
    }

    #[allow(dead_code)]
    pub fn as_slice(&self) -> &[usize] {
        &self.0
    }
}

impl Default for AstId {
    fn default() -> Self {
        AstId::root()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingId(usize);

impl BindingId {
    fn to_index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug)]
pub enum BindingOwner {
    #[allow(dead_code)]
    Parameter { function: FunctionKey, name: String, position: usize }, // TODO(6.5.3): surface parameter names/positions in diagnostics
    Local { function: FunctionKey, name: String, _depth: usize },
    Return { function: FunctionKey },
}

#[derive(Clone, Debug)]
pub struct BindingInfo {
    #[allow(dead_code)]
    pub id: BindingId,
    pub owner: BindingOwner,
    #[allow(dead_code)]
    pub ast_id: AstId,
    pub value_kind: ValueKind,
    pub heap_ownership: HeapOwnership,
    pub map_value_types: Option<MapValueTypes>,
    pub set_element_kind: Option<ValueKind>,
    pub vector_element_kind: Option<ValueKind>,
}

#[derive(Clone, Debug, Default)]
pub struct FunctionAnalysis {
    pub parameter_bindings: Vec<BindingId>,
    pub local_bindings: Vec<BindingId>,
    pub return_binding: Option<BindingId>,
}

#[derive(Clone, Debug)]
pub struct TypeInferenceSummary {
    bindings: Vec<BindingInfo>,
    functions: HashMap<FunctionKey, FunctionAnalysis>,
}

impl TypeInferenceSummary {
    pub fn binding(&self, id: BindingId) -> Option<&BindingInfo> {
        self.bindings.get(id.to_index())
    }

    pub fn function(&self, key: &FunctionKey) -> Option<&FunctionAnalysis> {
        self.functions.get(key)
    }

    pub fn binding_kind(&self, id: BindingId) -> Option<ValueKind> {
        self.binding(id).map(|info| info.value_kind)
    }

    pub fn binding_ownership(&self, id: BindingId) -> Option<HeapOwnership> {
        self.binding(id).map(|info| info.heap_ownership)
    }

    pub fn binding_map_value_types(&self, id: BindingId) -> Option<&MapValueTypes> {
        self.binding(id).and_then(|info| info.map_value_types.as_ref())
    }

    #[allow(dead_code)]
    pub fn binding_set_element_kind(&self, id: BindingId) -> Option<ValueKind> {
        self.binding(id).and_then(|info| info.set_element_kind)
    }

    #[allow(dead_code)]
    pub fn binding_vector_element_kind(&self, id: BindingId) -> Option<ValueKind> {
        self.binding(id).and_then(|info| info.vector_element_kind)
    }

    pub fn iter_named_functions(&self) -> impl Iterator<Item = (&str, &FunctionAnalysis)> {
        self.functions.iter().filter_map(|(key, analysis)| match key {
            FunctionKey::Named(name) => Some((name.as_str(), analysis)),
            FunctionKey::Program => None,
        })
    }
}

struct BindingNode {
    id: BindingId,
    owner: BindingOwner,
    ast_id: AstId,
    value_kind: ValueKind,
    heap_ownership: HeapOwnership,
    pub map_value_types: Option<MapValueTypes>,
    pub set_element_kind: Option<ValueKind>,
    pub vector_element_kind: Option<ValueKind>,
}

struct BindingGraph {
    nodes: Vec<BindingNode>,
    functions: HashMap<FunctionKey, FunctionAnalysis>,
    constraints: Vec<Box<dyn Constraint>>,
}

struct GraphBuilder {
    nodes: Vec<BindingNode>,
    functions: HashMap<FunctionKey, FunctionAnalysis>,
    constraints: Vec<Box<dyn Constraint>>,
    env: Vec<HashMap<String, BindingId>>,
    function_stack: Vec<FunctionKey>,
    binding_map_metadata: HashMap<BindingId, MapValueTypes>,
    binding_set_metadata: HashMap<BindingId, ValueKind>,
    binding_vector_metadata: HashMap<BindingId, ValueKind>,
}

impl GraphBuilder {
    fn new() -> Self {
        let mut functions = HashMap::new();
        functions.entry(FunctionKey::program()).or_default();
        GraphBuilder {
            nodes: Vec::new(),
            functions,
            constraints: Vec::new(),
            env: vec![HashMap::new()],
            function_stack: vec![FunctionKey::program()],
            binding_map_metadata: HashMap::new(),
            binding_set_metadata: HashMap::new(),
            binding_vector_metadata: HashMap::new(),
        }
    }

    fn build(&mut self, expressions: &[Node]) {
        self.register_function_signatures(expressions);
        let mut path = AstId::root();
        for (idx, expr) in expressions.iter().enumerate() {
            path.push(idx);
            self.visit_node(expr, &mut path);
            path.pop();
        }
    }

    fn finish(self) -> BindingGraph {
        BindingGraph {
            nodes: self.nodes,
            functions: self.functions,
            constraints: self.constraints,
        }
    }

    fn current_function(&self) -> FunctionKey {
        self.function_stack
            .last()
            .cloned()
            .unwrap_or_else(FunctionKey::program)
    }

    fn push_env(&mut self) {
        self.env.push(HashMap::new());
    }

    fn pop_env(&mut self) {
        self.env.pop();
    }

    fn register_binding_name(&mut self, name: &str, id: BindingId) {
        if let Some(frame) = self.env.last_mut() {
            frame.insert(name.to_string(), id);
        }
    }

    fn lookup_symbol(&self, name: &str) -> Option<BindingId> {
        for frame in self.env.iter().rev() {
            if let Some(id) = frame.get(name) {
                return Some(*id);
            }
        }
        None
    }

    fn register_function_signatures(&mut self, expressions: &[Node]) {
        let mut path = AstId::root();
        for (idx, expr) in expressions.iter().enumerate() {
            path.push(idx);
            self.register_function(expr, &mut path);
            path.pop();
        }
    }

    fn binding_map_value_types_clone(&self, binding: BindingId) -> Option<MapValueTypes> {
        if let Some(metadata) = self.binding_map_metadata.get(&binding) {
            return Some(metadata.clone());
        }
        self.nodes.iter().find(|node| node.id == binding).and_then(|node| node.map_value_types.clone())
    }

    fn binding_set_element_kind(&self, binding: BindingId) -> Option<ValueKind> {
        if let Some(kind) = self.binding_set_metadata.get(&binding) {
            return Some(*kind);
        }
        self.nodes.iter().find(|node| node.id == binding).and_then(|node| node.set_element_kind)
    }

    fn binding_vector_element_kind(&self, binding: BindingId) -> Option<ValueKind> {
        if let Some(kind) = self.binding_vector_metadata.get(&binding) {
            return Some(*kind);
        }
        self.nodes.iter().find(|node| node.id == binding).and_then(|node| node.vector_element_kind)
    }

    fn extract_map_metadata(&self, node: &Node) -> Option<MapValueTypes> {
        match node {
            Node::Symbol { value } => self.lookup_symbol(value).and_then(|binding| self.binding_map_value_types_clone(binding)),
            Node::Map { entries } => infer_map_literal_metadata(entries),
            _ => None,
        }
    }

    fn extract_set_element_kind(&self, node: &Node) -> Option<ValueKind> {
        extract_set_element_kind(self, node)
    }

    fn extract_vector_element_kind(&self, node: &Node) -> Option<ValueKind> {
        extract_vector_element_kind(self, node)
    }

    fn prime_binding_map_metadata(&mut self, binding: BindingId, metadata: &MapValueTypes) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == binding) {
            merge_map_value_types(&mut node.map_value_types, metadata);
        }
    }

    fn prime_binding_set_metadata(&mut self, binding: BindingId, kind: ValueKind) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == binding) {
            merge_element_kind(&mut node.set_element_kind, kind);
        }
    }

    fn prime_binding_vector_metadata(&mut self, binding: BindingId, kind: ValueKind) {
        if let Some(node) = self.nodes.iter_mut().find(|node| node.id == binding) {
            merge_element_kind(&mut node.vector_element_kind, kind);
        }
    }

    fn register_function(&mut self, node: &Node, path: &mut AstId) {
        let Node::List { root } = node else { return; };
        if root.len() != 4 {
            return;
        }
        let Node::Symbol { value } = &root[0] else {
            return;
        };
        if value != "defn" {
            return;
        }
        let func_name = match &root[1] {
            Node::Symbol { value } => value.clone(),
            _ => return,
        };
        let func_key = FunctionKey::Named(func_name.clone());
        if let Node::Vector { root: params } = &root[2] {
            path.push(2);
            for (idx, param) in params.iter().enumerate() {
                if let Node::Symbol { value } = param {
                    path.push(idx);
                    self.add_binding(
                        BindingOwner::Parameter {
                            function: func_key.clone(),
                            name: value.clone(),
                            position: idx,
                        },
                        path.clone(),
                    );
                    path.pop();
                }
            }
            path.pop();
        }
        path.push(3);
        self.add_binding(
            BindingOwner::Return {
                function: func_key,
            },
            path.clone(),
        );
        path.pop();
    }

    fn visit_node(&mut self, node: &Node, path: &mut AstId) {
        match node {
            Node::List { root } => self.visit_list(root, path),
            Node::Vector { root } | Node::Set { root } => {
                for (idx, child) in root.iter().enumerate() {
                    path.push(idx);
                    self.visit_node(child, path);
                    path.pop();
                }
            }
            Node::Map { entries } => {
        for (idx, (key, value)) in entries.iter().enumerate() {
            path.push(idx * 2);
            self.visit_node(key, path);
            path.pop();
            path.push(idx * 2 + 1);
            self.visit_node(value, path);
            path.pop();
        }
    }
            Node::Primitive { .. } | Node::Symbol { .. } => {}
        }
    }

    fn visit_list(&mut self, nodes: &[Node], path: &mut AstId) {
        if nodes.is_empty() {
            return;
        }

        if let Node::Symbol { value } = &nodes[0] {
            match value.as_str() {
                "defn" => {
                    self.visit_defn(nodes, path);
                    return;
                }
                "let" => {
                    self.visit_let(nodes, path);
                    return;
                }
                _ => {}
            }
        }

        for (idx, child) in nodes.iter().enumerate() {
            path.push(idx);
            self.visit_node(child, path);
            path.pop();
        }
    }

    fn visit_defn(&mut self, nodes: &[Node], path: &mut AstId) {
        if nodes.len() != 4 {
            self.visit_children(nodes, path);
            return;
        }

        let func_name = match &nodes[1] {
            Node::Symbol { value } => value.clone(),
            _ => {
                self.visit_children(nodes, path);
                return;
            }
        };

        let func_key = FunctionKey::Named(func_name.clone());
        self.function_stack.push(func_key.clone());
        self.push_env();

        path.push(2);
        if let Node::Vector { root } = &nodes[2] {
            for (idx, param) in root.iter().enumerate() {
                if let Node::Symbol { value } = param {
                    path.push(idx);
                    if let Some(binding_id) = self.get_parameter_binding(&func_key, idx) {
                        self.register_binding_name(value, binding_id);
                    }
                    path.pop();
                }
            }
        }
        path.pop();

        let return_binding = self.get_return_binding(&func_key).expect("return binding missing");
        path.push(3);
        self.visit_node(&nodes[3], path);
        self.plan_assignment(return_binding, &nodes[3]);
        path.pop();

        self.pop_env();
        self.function_stack.pop();
    }

    fn visit_let(&mut self, nodes: &[Node], path: &mut AstId) {
        if nodes.len() != 3 {
            self.visit_children(nodes, path);
            return;
        }

        let bindings_node = &nodes[1];
        let body_node = &nodes[2];

        let Node::Vector { root } = bindings_node else {
            self.visit_children(nodes, path);
            return;
        };

        if root.len() % 2 != 0 {
            self.visit_children(nodes, path);
            return;
        }

        self.push_env();
        path.push(1);
        for pair_index in 0..(root.len() / 2) {
            let name_idx = pair_index * 2;
            let value_idx = name_idx + 1;
            let mut binding_id = None;
            if let Node::Symbol { value } = &root[name_idx] {
                path.push(name_idx);
                let owner = BindingOwner::Local {
                    function: self.current_function(),
                    name: value.clone(),
                    _depth: self.env.len(),
                };
                binding_id = Some(self.add_binding(owner, path.clone()));
                path.pop();
            }

            path.push(value_idx);
            self.visit_node(&root[value_idx], path);
            if let Some(id) = binding_id {
                self.plan_assignment(id, &root[value_idx]);
                if let Node::Symbol { value } = &root[name_idx] {
                    self.register_binding_name(value, id);
                }
            }
            path.pop();
        }
        path.pop();

        path.push(2);
        self.visit_node(body_node, path);
        path.pop();
        self.pop_env();
    }

    fn visit_children(&mut self, nodes: &[Node], path: &mut AstId) {
        for (idx, child) in nodes.iter().enumerate() {
            path.push(idx);
            self.visit_node(child, path);
            path.pop();
        }
    }

    fn add_binding(&mut self, owner: BindingOwner, ast_id: AstId) -> BindingId {
        let id = BindingId(self.nodes.len());
        self.nodes.push(BindingNode {
            id,
            owner: owner.clone(),
            ast_id,
            value_kind: ValueKind::Any,
            heap_ownership: HeapOwnership::None,
            map_value_types: None,
            set_element_kind: None,
            vector_element_kind: None,
        });

        match &owner {
            BindingOwner::Parameter { function, .. } => {
                self.functions.entry(function.clone()).or_default().parameter_bindings.push(id);
            }
            BindingOwner::Local { function, .. } => {
                self.functions.entry(function.clone()).or_default().local_bindings.push(id);
            }
            BindingOwner::Return { function } => {
                self.functions.entry(function.clone()).or_default().return_binding = Some(id);
            }
        }

        id
    }

    fn get_parameter_binding(&self, function: &FunctionKey, index: usize) -> Option<BindingId> {
        self.functions
            .get(function)
            .and_then(|analysis| analysis.parameter_bindings.get(index).copied())
    }

    fn get_return_binding(&self, function: &FunctionKey) -> Option<BindingId> {
        self.functions.get(function).and_then(|analysis| analysis.return_binding)
    }

    fn plan_assignment(&mut self, binding: BindingId, node: &Node) {
        match node {
            Node::Primitive { value } => match value {
                Primitive::Number(_) => self.add_literal_constraint(binding, ValueKind::Number, HeapOwnership::None, None),
                Primitive::Boolean(_) => self.add_literal_constraint(binding, ValueKind::Boolean, HeapOwnership::None, None),
                Primitive::String(_) => self.add_literal_constraint(binding, ValueKind::String, HeapOwnership::Owned, None),
                Primitive::Keyword(_) => self.add_literal_constraint(binding, ValueKind::Keyword, HeapOwnership::None, None),
            },
            Node::Symbol { value } => {
                if value == "nil" {
                    self.add_literal_constraint(binding, ValueKind::Nil, HeapOwnership::None, None);
                } else if let Some(source) = self.lookup_symbol(value) {
                    self.constraints.push(Box::new(CopyConstraint::new(binding, source)));
                }
            }
            Node::Vector { root } => {
                let element_kind = infer_vector_literal_kind(root);
                self.add_literal_constraint_with_metadata(binding, ValueKind::Vector, HeapOwnership::Owned, None, None, element_kind);
            }
            Node::Map { entries } => {
                let metadata = infer_map_literal_metadata(entries);
                self.add_literal_constraint_with_metadata(binding, ValueKind::Map, HeapOwnership::Owned, metadata, None, None);
            }
            Node::Set { root } => {
                let element_kind = infer_set_literal_kind(root);
                self.add_literal_constraint_with_metadata(binding, ValueKind::Set, HeapOwnership::Owned, None, element_kind, None);
            }
            Node::List { root } => self.plan_list_assignment(binding, root),
        }
    }

    fn plan_list_assignment(&mut self, binding: BindingId, nodes: &[Node]) {
        if nodes.is_empty() {
            self.add_literal_constraint(binding, ValueKind::Nil, HeapOwnership::None, None);
            return;
        }

        let Node::Symbol { value } = &nodes[0] else {
            return;
        };

        match value.as_str() {
            "+" | "-" | "*" | "/" => {
                self.plan_builtin_arguments(nodes);
                self.add_literal_constraint(binding, ValueKind::Number, HeapOwnership::None, None);
            }
            "=" | "<" | ">" | "<=" | ">=" | "and" | "or" | "not" => {
                self.plan_builtin_arguments(nodes);
                self.add_literal_constraint(binding, ValueKind::Boolean, HeapOwnership::None, None);
            }
            "str" | "subs" => {
                self.plan_builtin_arguments(nodes);
                self.add_literal_constraint(binding, ValueKind::String, HeapOwnership::Owned, None);
            }
            "vec" => {
                self.plan_builtin_arguments(nodes);
                let element_kind = infer_element_kind(nodes.iter().skip(1));
                self.add_literal_constraint_with_metadata(binding, ValueKind::Vector, HeapOwnership::Owned, None, None, element_kind);
            }
            "set" => {
                self.plan_builtin_arguments(nodes);
                self.plan_set_metadata(binding, nodes);
            }
            "hash-map" => {
                self.plan_builtin_arguments(nodes);
                self.add_literal_constraint(binding, ValueKind::Map, HeapOwnership::Owned, None);
            }
            "count" => {
                self.plan_builtin_arguments(nodes);
                self.add_literal_constraint(binding, ValueKind::Number, HeapOwnership::None, None);
            }
            "assoc" => {
                self.plan_builtin_arguments(nodes);
                self.plan_assoc_metadata(binding, nodes);
            }
            "dissoc" => {
                self.plan_builtin_arguments(nodes);
                self.plan_dissoc_metadata(binding, nodes);
            }
            "disj" => {
                self.plan_builtin_arguments(nodes);
                self.plan_disj_metadata(binding, nodes);
            }
            "contains?" => {
                self.plan_builtin_arguments(nodes);
                self.add_literal_constraint(binding, ValueKind::Boolean, HeapOwnership::None, None);
            }
            "get" => {
                // `get` can return any element type; leave as Any but plan argument propagation.
                self.plan_builtin_arguments(nodes);
                self.plan_get_metadata(binding, nodes);
            }
            other => self.plan_function_call(binding, other, nodes),
        }
    }

    fn add_literal_constraint(&mut self, binding: BindingId, kind: ValueKind, ownership: HeapOwnership, map_value_types: Option<MapValueTypes>) {
        self.add_literal_constraint_with_metadata(binding, kind, ownership, map_value_types, None, None);
    }

    fn add_literal_constraint_with_metadata(
        &mut self,
        binding: BindingId,
        kind: ValueKind,
        ownership: HeapOwnership,
        map_value_types: Option<MapValueTypes>,
        set_element_kind: Option<ValueKind>,
        vector_element_kind: Option<ValueKind>,
    ) {
        if let Some(ref metadata) = map_value_types {
            self.binding_map_metadata.insert(binding, metadata.clone());
            self.prime_binding_map_metadata(binding, metadata);
        }
        if let Some(kind) = set_element_kind {
            self.binding_set_metadata.insert(binding, kind);
            self.prime_binding_set_metadata(binding, kind);
        }
        if let Some(kind) = vector_element_kind {
            self.binding_vector_metadata.insert(binding, kind);
            self.prime_binding_vector_metadata(binding, kind);
        }
        self.constraints.push(Box::new(LiteralConstraint::new(
            binding,
            kind,
            ownership,
            map_value_types,
            set_element_kind,
            vector_element_kind,
        )));
    }

    fn plan_function_call(&mut self, binding: BindingId, name: &str, nodes: &[Node]) {
        self.plan_builtin_arguments(nodes);
        let func_key = FunctionKey::Named(name.to_string());
        if let Some(return_binding) = self.get_return_binding(&func_key) {
            self.constraints.push(Box::new(CopyConstraint::new(binding, return_binding)));
        }
        if let Some(params) = self
            .functions
            .get(&func_key)
            .map(|analysis| analysis.parameter_bindings.clone())
        {
            for (idx, arg) in nodes[1..].iter().enumerate() {
                if let Some(param_binding) = params.get(idx) {
                    self.plan_assignment(*param_binding, arg);
                }
            }
        } else {
            for arg in &nodes[1..] {
                self.plan_assignment_for_node(arg);
            }
        }
    }

    fn plan_assoc_metadata(&mut self, binding: BindingId, nodes: &[Node]) {
        if nodes.len() < 3 {
            self.add_literal_constraint(binding, ValueKind::Map, HeapOwnership::Owned, None);
            return;
        }

        let base_metadata = nodes.get(1).and_then(|expr| self.extract_map_metadata(expr));
        let mut metadata = base_metadata.unwrap_or_else(MapValueTypes::new);

        for chunk in nodes[2..].chunks(2) {
            if chunk.len() < 2 {
                break;
            }
            if let Some(key_literal) = map_key_literal_from_node(&chunk[0]) {
                if let Some(kind) = node_literal_kind(&chunk[1]) {
                    metadata.insert(key_literal, kind);
                }
            }
        }

        let metadata_opt = if metadata.is_empty() { None } else { Some(metadata) };
        self.add_literal_constraint(binding, ValueKind::Map, HeapOwnership::Owned, metadata_opt);
    }

    fn plan_dissoc_metadata(&mut self, binding: BindingId, nodes: &[Node]) {
        if nodes.len() < 2 {
            self.add_literal_constraint(binding, ValueKind::Map, HeapOwnership::Owned, None);
            return;
        }
        let mut metadata = nodes.get(1).and_then(|expr| self.extract_map_metadata(expr)).unwrap_or_else(MapValueTypes::new);
        for key_expr in nodes.iter().skip(2) {
            if let Some(key_literal) = map_key_literal_from_node(key_expr) {
                metadata.remove(&key_literal);
            }
        }
        let metadata_opt = if metadata.is_empty() { None } else { Some(metadata) };
        self.add_literal_constraint(binding, ValueKind::Map, HeapOwnership::Owned, metadata_opt);
    }

    fn plan_get_metadata(&mut self, binding: BindingId, nodes: &[Node]) {
        if nodes.len() < 3 {
            return;
        }

        let key_expr = &nodes[2];
        if let Some(metadata) = self.extract_map_metadata(&nodes[1]) {
            if let Some(key_literal) = map_key_literal_from_node(key_expr) {
                if let Some(kind) = metadata.get(&key_literal) {
                    let ownership = if kind.is_heap_kind() { HeapOwnership::Borrowed } else { HeapOwnership::None };
                    self.add_literal_constraint(binding, *kind, ownership, None);
                    return;
                }
            }
        }

        if let Node::Symbol { value } = &nodes[1] {
            if let Some(key_literal) = map_key_literal_from_node(key_expr) {
                if let Some(map_binding) = self.lookup_symbol(value) {
                    self.constraints.push(Box::new(GetConstraint::new(binding, map_binding, key_literal)));
                } else {
                    debug_assert!(false, "unresolved map symbol {}", value);
                }
            }
        }

        if let Some(element_kind) = self.extract_vector_element_kind(&nodes[1]) {
            let ownership = if element_kind.is_heap_kind() { HeapOwnership::Borrowed } else { HeapOwnership::None };
            self.add_literal_constraint_with_metadata(binding, element_kind, ownership, None, None, None);
        } else if let Node::Symbol { value } = &nodes[1] {
            if let Some(vector_binding) = self.lookup_symbol(value) {
                self.constraints.push(Box::new(VectorElementConstraint::new(binding, vector_binding)));
            }
        }
    }

    fn plan_set_metadata(&mut self, binding: BindingId, nodes: &[Node]) {
        let element_kind = infer_element_kind(nodes.iter().skip(1));
        self.add_literal_constraint_with_metadata(binding, ValueKind::Set, HeapOwnership::Owned, None, element_kind, None);
    }

    fn plan_disj_metadata(&mut self, binding: BindingId, nodes: &[Node]) {
        let element_kind = nodes.get(1).and_then(|expr| self.extract_set_element_kind(expr));
        self.add_literal_constraint_with_metadata(binding, ValueKind::Set, HeapOwnership::Owned, None, element_kind, None);
    }

    fn plan_builtin_arguments(&mut self, nodes: &[Node]) {
        if nodes.is_empty() {
            return;
        }

        let Node::Symbol { value } = &nodes[0] else {
            return;
        };

        let arg_kinds: Option<Vec<ValueKind>> = match value.as_str() {
            "+" | "-" | "*" | "/" | "=" | "<" | ">" | "<=" | ">=" => {
                let count = nodes.len().saturating_sub(1);
                Some(vec![ValueKind::Number; count])
            }
            "and" | "or" => {
                let count = nodes.len().saturating_sub(1);
                Some(vec![ValueKind::Boolean; count])
            }
            "not" => Some(vec![ValueKind::Boolean]),
            "str" => Some(vec![ValueKind::Any; nodes.len() - 1]),
            "subs" => Some(vec![ValueKind::String, ValueKind::Number, ValueKind::Number]),
            "vec" => Some(vec![ValueKind::Any; nodes.len() - 1]),
            "set" => Some(vec![ValueKind::Any; nodes.len() - 1]),
            "hash-map" => Some(vec![ValueKind::Any; nodes.len() - 1]),
            "count" => Some(vec![ValueKind::Any]),
            "get" => Some(vec![ValueKind::Any, ValueKind::Any]),
            "assoc" => {
                if nodes.len() <= 1 {
                    Some(Vec::new())
                } else {
                    let mut kinds = Vec::with_capacity(nodes.len() - 1);
                    kinds.push(ValueKind::Map);
                    kinds.extend(std::iter::repeat(ValueKind::Any).take(nodes.len() - 2));
                    Some(kinds)
                }
            }
            "dissoc" => {
                if nodes.len() <= 1 {
                    Some(Vec::new())
                } else {
                    let mut kinds = Vec::with_capacity(nodes.len() - 1);
                    kinds.push(ValueKind::Map);
                    kinds.extend(std::iter::repeat(ValueKind::Any).take(nodes.len() - 2));
                    Some(kinds)
                }
            }
            "disj" => {
                if nodes.len() <= 1 {
                    Some(Vec::new())
                } else {
                    let mut kinds = Vec::with_capacity(nodes.len() - 1);
                    kinds.push(ValueKind::Set);
                    kinds.extend(std::iter::repeat(ValueKind::Any).take(nodes.len() - 2));
                    Some(kinds)
                }
            }
            "contains?" => Some(vec![ValueKind::Any, ValueKind::Any]),
            _ => None,
        };

        if let Some(expected) = arg_kinds {
            for (idx, arg) in nodes[1..].iter().enumerate() {
                if let Some(kind) = expected.get(idx) {
                    self.plan_argument_with_kind(arg, *kind);
                } else {
                    self.plan_assignment_for_node(arg);
                }
            }
        } else {
            // Fallback: still traverse arguments so nested forms get constraints.
            for arg in &nodes[1..] {
                self.plan_assignment_for_node(arg);
            }
        }
    }

    fn plan_argument_with_kind(&mut self, node: &Node, expected: ValueKind) {
        match (expected, node) {
            (ValueKind::Number, Node::Primitive { value: Primitive::Number(_) }) => {}
            (ValueKind::Boolean, Node::Primitive { value: Primitive::Boolean(_) }) => {}
            (ValueKind::String, Node::Primitive { value: Primitive::String(_) }) => {}
            (ValueKind::Vector, Node::Vector { .. }) => {}
            (ValueKind::Map, Node::Map { .. }) => {}
            (ValueKind::Set, Node::Set { .. }) => {}
            (_, Node::Symbol { value }) if expected != ValueKind::Any => {
                if let Some(binding) = self.lookup_symbol(value) {
                    self.add_literal_constraint(binding, expected, HeapOwnership::None, None);
                }
            }
            (ValueKind::Any, _) => self.plan_assignment_for_node(node),
            _ => self.plan_assignment_for_node(node),
        }
    }

    fn plan_assignment_for_node(&mut self, node: &Node) {
        match node {
            Node::Primitive { .. } | Node::Symbol { .. } | Node::Vector { .. } | Node::Map { .. } | Node::Set { .. } => {}
            Node::List { root } => {
                if !root.is_empty() {
                    if let Node::Symbol { value } = &root[0] {
                        if let Some(params) = self
                            .functions
                            .get(&FunctionKey::Named(value.clone()))
                            .map(|analysis| analysis.parameter_bindings.clone())
                        {
                            for (idx, arg) in root[1..].iter().enumerate() {
                                if let Some(binding_id) = params.get(idx) {
                                    self.plan_assignment(*binding_id, arg);
                                }
                            }
                        }
                    }
                }
                for child in root {
                    self.plan_assignment_for_node(child);
                }
            }
        }
    }
}

fn infer_element_kind<'a, I>(nodes: I) -> Option<ValueKind>
where
    I: IntoIterator<Item = &'a Node>,
{
    let mut element_kind: Option<ValueKind> = None;
    for node in nodes {
        if let Some(kind) = node_literal_kind(node) {
            merge_element_kind(&mut element_kind, kind);
        }
    }
    element_kind
}

fn infer_set_literal_kind(root: &[Node]) -> Option<ValueKind> {
    infer_element_kind(root.iter())
}

fn infer_vector_literal_kind(root: &[Node]) -> Option<ValueKind> {
    infer_element_kind(root.iter())
}

fn extract_set_element_kind(builder: &GraphBuilder, node: &Node) -> Option<ValueKind> {
    match node {
        Node::Set { root } => infer_set_literal_kind(root),
        Node::Symbol { value } => builder.lookup_symbol(value).and_then(|binding| builder.binding_set_element_kind(binding)),
        Node::List { root } => {
            if let Some(Node::Symbol { value }) = root.first() {
                match value.as_str() {
                    "set" => infer_element_kind(root.iter().skip(1)),
                    "disj" => root.get(1).and_then(|expr| extract_set_element_kind(builder, expr)),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_vector_element_kind(builder: &GraphBuilder, node: &Node) -> Option<ValueKind> {
    match node {
        Node::Vector { root } => infer_vector_literal_kind(root),
        Node::Symbol { value } => builder
            .lookup_symbol(value)
            .and_then(|binding| builder.binding_vector_element_kind(binding)),
        Node::List { root } => {
            if let Some(Node::Symbol { value }) = root.first() {
                match value.as_str() {
                    "vec" => infer_element_kind(root.iter().skip(1)),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn infer_map_literal_metadata(entries: &[(Node, Node)]) -> Option<MapValueTypes> {
    let mut metadata = MapValueTypes::new();
    for (key_node, value_node) in entries {
        if let Some(map_key) = map_key_literal_from_node(key_node) {
            if let Some(kind) = node_literal_kind(value_node) {
                metadata.insert(map_key, kind);
            }
        }
    }
    if metadata.is_empty() {
        None
    } else {
        Some(metadata)
    }
}

fn map_key_literal_from_node(node: &Node) -> Option<MapKeyLiteral> {
    match node {
        Node::Primitive { value } => match value {
            Primitive::String(s) => Some(MapKeyLiteral::String(s.clone())),
            Primitive::Keyword(s) => Some(MapKeyLiteral::Keyword(s.clone())),
            Primitive::Number(n) => Some(MapKeyLiteral::Number(*n as i64)),
            Primitive::Boolean(b) => Some(MapKeyLiteral::Boolean(*b)),
        },
        Node::Symbol { value } if value == "nil" => Some(MapKeyLiteral::Nil),
        _ => None,
    }
}

fn node_literal_kind(node: &Node) -> Option<ValueKind> {
    match node {
        Node::Primitive { value } => match value {
            Primitive::Number(_) => Some(ValueKind::Number),
            Primitive::Boolean(_) => Some(ValueKind::Boolean),
            Primitive::String(_) => Some(ValueKind::String),
            Primitive::Keyword(_) => Some(ValueKind::Keyword),
        },
        Node::Symbol { value } if value == "nil" => Some(ValueKind::Nil),
        Node::Vector { .. } => Some(ValueKind::Vector),
        Node::Map { .. } => Some(ValueKind::Map),
        Node::Set { .. } => Some(ValueKind::Set),
        _ => None,
    }
}

struct TypeInferenceEngine {
    nodes: Vec<BindingNode>,
    functions: HashMap<FunctionKey, FunctionAnalysis>,
    constraints: Vec<Box<dyn Constraint>>,
}

impl TypeInferenceEngine {
    fn new(graph: BindingGraph) -> Self {
        TypeInferenceEngine {
            nodes: graph.nodes,
            functions: graph.functions,
            constraints: graph.constraints,
        }
    }

    fn solve(&mut self) {
        if self.constraints.is_empty() {
            return;
        }

        loop {
            let mut progress = false;
            for constraint in &mut self.constraints {
                let mut context = ConstraintContext::new(&mut self.nodes);
                if constraint.apply(&mut context) == ConstraintState::Progress {
                    progress = true;
                }
            }

            if !progress {
                break;
            }
        }
    }

    fn into_summary(self) -> TypeInferenceSummary {
        let bindings = self
            .nodes
            .into_iter()
            .map(|node| BindingInfo {
                id: node.id,
                owner: node.owner,
                ast_id: node.ast_id,
                value_kind: node.value_kind,
                heap_ownership: node.heap_ownership,
                map_value_types: node.map_value_types,
                set_element_kind: node.set_element_kind,
                vector_element_kind: node.vector_element_kind,
            })
            .collect();

        TypeInferenceSummary {
            bindings,
            functions: self.functions,
        }
    }
}

struct ConstraintContext<'a> {
    nodes: &'a mut [BindingNode],
}

impl<'a> ConstraintContext<'a> {
    fn new(nodes: &'a mut [BindingNode]) -> Self {
        ConstraintContext { nodes }
    }

    fn update_binding_kind(&mut self, id: BindingId, kind: ValueKind) -> bool {
        let node = self.nodes.get_mut(id.to_index()).expect("invalid binding id");
        let merged = merge_kinds(node.value_kind, kind);
        if merged != node.value_kind {
            node.value_kind = merged;
            true
        } else {
            false
        }
    }

    fn update_binding_ownership(&mut self, id: BindingId, ownership: HeapOwnership) -> bool {
        if ownership == HeapOwnership::None {
            return false;
        }
        let node = self.nodes.get_mut(id.to_index()).expect("invalid binding id");
        let merged = merge_ownership(node.heap_ownership, ownership);
        if merged != node.heap_ownership {
            node.heap_ownership = merged;
            true
        } else {
            false
        }
    }

    fn binding_kind(&self, id: BindingId) -> ValueKind {
        self.nodes[id.to_index()].value_kind
    }

    fn binding_ownership(&self, id: BindingId) -> HeapOwnership {
        self.nodes[id.to_index()].heap_ownership
    }

    fn binding_map_value_types(&self, id: BindingId) -> Option<&MapValueTypes> {
        self.nodes[id.to_index()].map_value_types.as_ref()
    }

    fn update_map_value_types(&mut self, id: BindingId, map: Option<&MapValueTypes>) -> bool {
        let Some(incoming) = map else { return false; };
        let node = self.nodes.get_mut(id.to_index()).expect("invalid binding id");
        merge_map_value_types(&mut node.map_value_types, incoming)
    }

    fn binding_set_element_kind(&self, id: BindingId) -> Option<ValueKind> {
        self.nodes[id.to_index()].set_element_kind
    }

    fn update_set_element_kind(&mut self, id: BindingId, kind: Option<ValueKind>) -> bool {
        let Some(kind) = kind else { return false; };
        let node = self.nodes.get_mut(id.to_index()).expect("invalid binding id");
        merge_element_kind(&mut node.set_element_kind, kind)
    }

    fn binding_vector_element_kind(&self, id: BindingId) -> Option<ValueKind> {
        self.nodes[id.to_index()].vector_element_kind
    }

    fn update_vector_element_kind(&mut self, id: BindingId, kind: Option<ValueKind>) -> bool {
        let Some(kind) = kind else { return false; };
        let node = self.nodes.get_mut(id.to_index()).expect("invalid binding id");
        merge_element_kind(&mut node.vector_element_kind, kind)
    }
}

fn merge_kinds(current: ValueKind, next: ValueKind) -> ValueKind {
    if current == ValueKind::Any {
        return next;
    }

    if next == ValueKind::Any {
        return current;
    }

    if current == next {
        current
    } else {
        ValueKind::Any
    }
}

fn merge_ownership(current: HeapOwnership, next: HeapOwnership) -> HeapOwnership {
    use HeapOwnership::*;
    match (current, next) {
        (Owned, _) | (_, Owned) => Owned,
        (Borrowed, _) | (_, Borrowed) => Borrowed,
        (None, None) => None,
    }
}

fn merge_map_value_types(target: &mut Option<MapValueTypes>, incoming: &MapValueTypes) -> bool {
    match target {
        Some(existing) => {
            let mut changed = false;
            for (key, value) in incoming {
                match existing.get(key) {
                    Some(existing_kind) => {
                        let merged = merge_kinds(*existing_kind, *value);
                        if merged != *existing_kind {
                            existing.insert(key.clone(), merged);
                            changed = true;
                        }
                    }
                    None => {
                        existing.insert(key.clone(), *value);
                        changed = true;
                    }
                }
            }
            changed
        }
        None => {
            if incoming.is_empty() {
                false
            } else {
                *target = Some(incoming.clone());
                true
            }
        }
    }
}

fn merge_element_kind(target: &mut Option<ValueKind>, incoming: ValueKind) -> bool {
    match target {
        Some(existing) => {
            let merged = merge_kinds(*existing, incoming);
            if merged != *existing {
                *existing = merged;
                true
            } else {
                false
            }
        }
        None => {
            *target = Some(incoming);
            true
        }
    }
}

#[derive(PartialEq, Eq)]
enum ConstraintState {
    Stable,
    Progress,
}

trait Constraint {
    fn apply(&mut self, context: &mut ConstraintContext<'_>) -> ConstraintState;
}

struct LiteralConstraint {
    target: BindingId,
    kind: ValueKind,
    ownership: HeapOwnership,
    pub map_value_types: Option<MapValueTypes>,
    pub set_element_kind: Option<ValueKind>,
    pub vector_element_kind: Option<ValueKind>,
}

impl LiteralConstraint {
    fn new(
        target: BindingId,
        kind: ValueKind,
        ownership: HeapOwnership,
        map_value_types: Option<MapValueTypes>,
        set_element_kind: Option<ValueKind>,
        vector_element_kind: Option<ValueKind>,
    ) -> Self {
        LiteralConstraint {
            target,
            kind,
            ownership,
            map_value_types,
            set_element_kind,
            vector_element_kind,
        }
    }
}

impl Constraint for LiteralConstraint {
    fn apply(&mut self, context: &mut ConstraintContext<'_>) -> ConstraintState {
        let mut progress = false;
        if context.update_binding_kind(self.target, self.kind) {
            progress = true;
        }
        if context.update_binding_ownership(self.target, self.ownership) {
            progress = true;
        }
        if context.update_map_value_types(self.target, self.map_value_types.as_ref()) {
            progress = true;
        }
        if context.update_set_element_kind(self.target, self.set_element_kind) {
            progress = true;
        }
        if context.update_vector_element_kind(self.target, self.vector_element_kind) {
            progress = true;
        }
        if progress {
            ConstraintState::Progress
        } else {
            ConstraintState::Stable
        }
    }
}

struct CopyConstraint {
    target: BindingId,
    source: BindingId,
}

impl CopyConstraint {
    fn new(target: BindingId, source: BindingId) -> Self {
        CopyConstraint { target, source }
    }
}

impl Constraint for CopyConstraint {
    fn apply(&mut self, context: &mut ConstraintContext<'_>) -> ConstraintState {
        let source_kind = context.binding_kind(self.source);
        let source_ownership = context.binding_ownership(self.source);
        let source_map_types = context.binding_map_value_types(self.source).cloned();
        let source_set_element = context.binding_set_element_kind(self.source);
        let source_vector_element = context.binding_vector_element_kind(self.source);
        let mut progress = false;
        if source_kind == ValueKind::Any {
            return ConstraintState::Stable;
        }
        if context.update_binding_kind(self.target, source_kind) {
            progress = true;
        }
        if context.update_binding_ownership(self.target, source_ownership) {
            progress = true;
        }
        if context.update_map_value_types(self.target, source_map_types.as_ref()) {
            progress = true;
        }
        if context.update_set_element_kind(self.target, source_set_element) {
            progress = true;
        }
        if context.update_vector_element_kind(self.target, source_vector_element) {
            progress = true;
        }
        if progress {
            ConstraintState::Progress
        } else {
            ConstraintState::Stable
        }
    }
}

struct GetConstraint {
    target: BindingId,
    map_binding: BindingId,
    key: MapKeyLiteral,
}

impl GetConstraint {
    fn new(target: BindingId, map_binding: BindingId, key: MapKeyLiteral) -> Self {
        GetConstraint { target, map_binding, key }
    }
}

impl Constraint for GetConstraint {
    fn apply(&mut self, context: &mut ConstraintContext<'_>) -> ConstraintState {
        let Some(metadata) = context.binding_map_value_types(self.map_binding) else {
            return ConstraintState::Stable;
        };

        let Some(kind) = metadata.get(&self.key).copied() else {
            return ConstraintState::Stable;
        };

        let ownership = if kind.is_heap_kind() { HeapOwnership::Borrowed } else { HeapOwnership::None };
        let mut progress = false;
        if context.update_binding_kind(self.target, kind) {
            progress = true;
        }
        if context.update_binding_ownership(self.target, ownership) {
            progress = true;
        }
        if progress {
            ConstraintState::Progress
        } else {
            ConstraintState::Stable
        }
    }
}

struct VectorElementConstraint {
    target: BindingId,
    vector_binding: BindingId,
}

impl VectorElementConstraint {
    fn new(target: BindingId, vector_binding: BindingId) -> Self {
        VectorElementConstraint { target, vector_binding }
    }
}

impl Constraint for VectorElementConstraint {
    fn apply(&mut self, context: &mut ConstraintContext<'_>) -> ConstraintState {
        let Some(element_kind) = context.binding_vector_element_kind(self.vector_binding) else {
            return ConstraintState::Stable;
        };

        let ownership = if element_kind.is_heap_kind() { HeapOwnership::Borrowed } else { HeapOwnership::None };
        let mut progress = false;
        if context.update_binding_kind(self.target, element_kind) {
            progress = true;
        }
        if context.update_binding_ownership(self.target, ownership) {
            progress = true;
        }
        if progress {
            ConstraintState::Progress
        } else {
            ConstraintState::Stable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AstParser, AstParserTrt};

    fn parse_expr(input: &str) -> Node {
        let mut offset = 0;
        AstParser::parse_sexp_new_domain(input.as_bytes(), &mut offset)
    }

    #[test]
    fn collects_function_parameters_and_returns() {
        let mut domain = 0;
        let expr = AstParser::parse_sexp_new_domain("(defn add [x y] (+ x y))".as_bytes(), &mut domain);
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("add".to_string());
        let analysis = summary.function(&key).unwrap();
        assert_eq!(analysis.parameter_bindings.len(), 2);
        assert!(analysis.return_binding.is_some());

        let first_param = summary.binding(analysis.parameter_bindings[0]).unwrap();
        match &first_param.owner {
            BindingOwner::Parameter { name, position, .. } => {
                assert_eq!(name, "x");
                assert_eq!(*position, 0);
            }
            other => panic!("unexpected owner: {:?}", other),
        }

        let return_binding = summary.binding(analysis.return_binding.unwrap()).unwrap();
        match &return_binding.owner {
            BindingOwner::Return { .. } => {}
            other => panic!("expected return binding, got {:?}", other),
        }
    }

    #[test]
    fn collects_top_level_let_locals() {
        let mut domain = 0;
        let expr = AstParser::parse_sexp_new_domain("(let [x 1 y (str \"a\" \"b\")] y)".as_bytes(), &mut domain);
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let analysis = summary.function(&FunctionKey::Program).unwrap();
        assert_eq!(analysis.local_bindings.len(), 2);
        let first = summary.binding(analysis.local_bindings[0]).unwrap();
        match &first.owner {
            BindingOwner::Local { name, .. } => assert_eq!(name, "x"),
            other => panic!("expected local binding, got {:?}", other),
        }
    }

    #[test]
    fn propagates_literal_and_symbol_constraints_in_let() {
        let mut domain = 0;
        let expr = AstParser::parse_sexp_new_domain("(let [x 1 y x] y)".as_bytes(), &mut domain);
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let analysis = summary.function(&FunctionKey::Program).unwrap();
        assert_eq!(analysis.local_bindings.len(), 2);
        let x_binding = summary.binding(analysis.local_bindings[0]).unwrap();
        assert_eq!(x_binding.value_kind, ValueKind::Number);
        let y_binding = summary.binding(analysis.local_bindings[1]).unwrap();
        assert_eq!(y_binding.value_kind, ValueKind::Number);
    }

    #[test]
    fn annotates_function_returns_from_literals() {
        let mut domain = 0;
        let expr = AstParser::parse_sexp_new_domain("(defn constant [] 42)".as_bytes(), &mut domain);
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("constant".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_id = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_id).unwrap();
        assert_eq!(binding.value_kind, ValueKind::Number);
    }

    #[test]
    fn propagates_function_call_results() {
        let make = parse_expr("(defn make [] (str \"x\"))");
        let use_fn = parse_expr("(defn use [] (make))");
        let program = vec![make, use_fn];
        let summary = run_type_inference(&program).unwrap();
        let use_key = FunctionKey::Named("use".to_string());
        let use_analysis = summary.function(&use_key).unwrap();
        let return_binding = use_analysis.return_binding.expect("missing return binding");
        assert_eq!(summary.binding(return_binding).unwrap().value_kind, ValueKind::String);
    }

    #[test]
    fn propagates_argument_kind_into_parameters() {
        let id = parse_expr("(defn id [x] x)");
        let caller = parse_expr("(defn caller [] (id 1))");
        let program = vec![id, caller];
        let summary = run_type_inference(&program).unwrap();
        let key = FunctionKey::Named("id".to_string());
        let analysis = summary.function(&key).unwrap();
        let param_binding = analysis.parameter_bindings[0];
        assert_eq!(summary.binding(param_binding).unwrap().value_kind, ValueKind::Number);
    }

    #[test]
    fn builtin_constraints_enforce_argument_kinds() {
        let expr = parse_expr("(defn add1 [x] (+ x 1))");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("add1".to_string());
        let analysis = summary.function(&key).unwrap();
        let param_binding = analysis.parameter_bindings[0];
        assert_eq!(summary.binding(param_binding).unwrap().value_kind, ValueKind::Number);
    }

    #[test]
    fn map_literal_metadata_is_recorded() {
        let expr = parse_expr("(defn make [] {\"a\" 1 :b true})");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("make".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_binding = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_binding).unwrap();
        let metadata = binding.map_value_types.as_ref().expect("missing metadata");
        assert_eq!(metadata.get(&MapKeyLiteral::String("a".to_string())), Some(&ValueKind::Number));
        assert_eq!(metadata.get(&MapKeyLiteral::Keyword("b".to_string())), Some(&ValueKind::Boolean));
    }

    #[test]
    fn get_infers_value_kind_from_map_metadata() {
        let expr = parse_expr("(defn use-get [] (get {\"a\" \"x\"} \"a\"))");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("use-get".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_binding = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_binding).unwrap();
        assert_eq!(binding.value_kind, ValueKind::String);
        assert_eq!(binding.heap_ownership, HeapOwnership::Borrowed);
    }

    #[test]
    fn set_literal_metadata_is_recorded() {
        let expr = parse_expr("(defn make [] #{1 2 3})");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("make".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_binding = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_binding).unwrap();
        assert_eq!(binding.value_kind, ValueKind::Set);
        assert_eq!(binding.set_element_kind, Some(ValueKind::Number));
    }

    #[test]
    fn disj_preserves_set_metadata() {
        let expr = parse_expr("(defn trim [] (let [s #{1 2} t (disj s 2)] t))");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("trim".to_string());
        let analysis = summary.function(&key).unwrap();
        let locals = &analysis.local_bindings;
        assert_eq!(locals.len(), 2);
        let trimmed_binding = summary.binding(locals[1]).unwrap();
        assert_eq!(trimmed_binding.value_kind, ValueKind::Set);
        assert_eq!(trimmed_binding.set_element_kind, Some(ValueKind::Number));
    }

    #[test]
    fn vector_literal_metadata_is_recorded() {
        let expr = parse_expr("(defn make [] [1 2 3])");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("make".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_binding = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_binding).unwrap();
        assert_eq!(binding.value_kind, ValueKind::Vector);
        assert_eq!(binding.vector_element_kind, Some(ValueKind::Number));
    }

    #[test]
    fn get_infers_value_kind_from_vector_metadata() {
        let expr = parse_expr("(defn first-val [] (get [\"a\" \"b\"] 0))");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("first-val".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_binding = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_binding).unwrap();
        assert_eq!(binding.value_kind, ValueKind::String);
        assert_eq!(binding.heap_ownership, HeapOwnership::Borrowed);
    }

    #[test]
    fn heap_ownership_propagates_through_string_returns() {
        let expr = parse_expr("(defn make [] (str \"x\"))");
        let summary = run_type_inference(std::slice::from_ref(&expr)).unwrap();
        let key = FunctionKey::Named("make".to_string());
        let analysis = summary.function(&key).unwrap();
        let return_binding = analysis.return_binding.expect("missing return binding");
        let binding = summary.binding(return_binding).unwrap();
        assert_eq!(binding.value_kind, ValueKind::String);
        assert_eq!(binding.heap_ownership, HeapOwnership::Owned);
    }

    #[test]
    fn solver_applies_constraints_until_stable() {
        let mut functions = HashMap::new();
        functions.entry(FunctionKey::program()).or_default();
        let id = BindingId(0);
        let nodes = vec![BindingNode {
            id,
            owner: BindingOwner::Return {
                function: FunctionKey::program(),
            },
            ast_id: AstId::root(),
            value_kind: ValueKind::Any,
            heap_ownership: HeapOwnership::None,
            map_value_types: None,
            set_element_kind: None,
            vector_element_kind: None,
        }];
        let constraints: Vec<Box<dyn Constraint>> = vec![Box::new(TestConstraint { id, fired: false })];
        let graph = BindingGraph {
            nodes,
            functions,
            constraints,
        };
        let mut engine = TypeInferenceEngine::new(graph);
        engine.solve();
        let summary = engine.into_summary();
        let binding = summary.binding(id).unwrap();
        assert_eq!(binding.value_kind, ValueKind::Number);
    }

    struct TestConstraint {
        id: BindingId,
        fired: bool,
    }

    impl Constraint for TestConstraint {
        fn apply(&mut self, context: &mut ConstraintContext<'_>) -> ConstraintState {
            if self.fired {
                return ConstraintState::Stable;
            }
            self.fired = true;
            if context.update_binding_kind(self.id, ValueKind::Number) {
                ConstraintState::Progress
            } else {
                ConstraintState::Stable
            }
        }
    }
}
