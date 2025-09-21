#[derive(PartialEq, Debug, Clone)]
pub enum Primitive {
    Number(usize),
    _Str(String),
}

#[derive(PartialEq, Debug, Clone)]
pub enum Node {
    List { root: Vec<Box<Node>> },
    Vector { root: Vec<Box<Node>> },
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
        Node::List {
            root: nodes.into_iter().map(Box::new).collect(),
        }
    }

    pub fn new_vector_from_raw(nodes: Vec<Node>) -> Node {
        Node::Vector {
            root: nodes.into_iter().map(Box::new).collect(),
        }
    }
}
