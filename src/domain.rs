#[derive(PartialEq, Debug, Clone)]
pub enum Primitive {
    Number(usize),
    String(String),
}

#[derive(PartialEq, Debug, Clone)]
pub enum Node {
    List { root: Vec<Node> },
    Vector { root: Vec<Node> },
    Primitive { value: Primitive },
    Symbol { value: String },
}

impl Node {
    pub fn new_number(number: usize) -> Node {
        Node::Primitive {
            value: Primitive::Number(number),
        }
    }

    pub fn new_list_from_raw(nodes: Vec<Node>) -> Node {
        Node::List { root: nodes }
    }

    pub fn new_vector_from_raw(nodes: Vec<Node>) -> Node {
        Node::Vector { root: nodes }
    }
}
