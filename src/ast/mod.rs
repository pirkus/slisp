/// AST (Abstract Syntax Tree) module
///
/// This module defines the AST data structures and parser for SLisp.
/// - AST node types (Node, Primitive)
/// - Parser to convert source text to AST
pub mod parser;

// Re-export the main types for convenience
pub use parser::{parse_file, AstParser, AstParserTrt};

#[derive(PartialEq, Debug, Clone)]
pub enum Primitive {
    Number(usize),
    Boolean(bool),
    String(String),
    Keyword(String),
}

#[derive(PartialEq, Debug, Clone)]
pub enum Node {
    List { root: Vec<Node> },
    Vector { root: Vec<Node> },
    Map { entries: Vec<(Node, Node)> },
    Primitive { value: Primitive },
    Symbol { value: String },
}

impl Node {
    pub fn new_number(number: usize) -> Node {
        Node::Primitive { value: Primitive::Number(number) }
    }

    pub fn new_boolean(value: bool) -> Node {
        Node::Primitive { value: Primitive::Boolean(value) }
    }

    pub fn new_list_from_raw(nodes: Vec<Node>) -> Node {
        Node::List { root: nodes }
    }

    pub fn new_vector_from_raw(nodes: Vec<Node>) -> Node {
        Node::Vector { root: nodes }
    }

    pub fn new_map_from_raw(entries: Vec<(Node, Node)>) -> Node {
        Node::Map { entries }
    }

    pub fn new_keyword_from_raw(value: String) -> Node {
        Node::Primitive { value: Primitive::Keyword(value) }
    }
}
