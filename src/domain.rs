#[derive(PartialEq)]
#[derive(Debug)]
pub enum Errors {
    InvalidSyntax
}

#[derive(PartialEq)]
#[derive(Debug)]
#[derive(Clone)]
pub struct AstNode {
    pub value: String,
    pub node_type: AstNodeType,
    pub nodes: Option<Vec<Box<AstNode>>>,
}

impl AstNode {
    pub fn new(value: String, node_type: AstNodeType, nodes: Vec<AstNode>) -> AstNode {
        AstNode { value, node_type, nodes: Some(nodes.iter().map(|x| Box::new(x.clone())).collect::<Vec<Box<AstNode>>>()) }
    }

    pub fn new_end_node(value: String, node_type: AstNodeType) -> AstNode {
        AstNode { value, node_type: node_type, nodes: None }
    }
}

#[derive(PartialEq)]
#[derive(Debug)]
#[derive(Clone)]
pub enum AstNodeType {
    Int,
    Symbol,
    List
    // Kw,
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn ast_is_constructable() {
        let s = String::from("root_node");
        let nested_node = AstNode { value: String::from("nested"), node_type: AstNodeType::Symbol, nodes: None };
        assert_eq!(AstNode { value: s.clone(), node_type: AstNodeType::Symbol, nodes: Some(vec![Box::new(nested_node.clone())]) }, AstNode::new(s.clone(), AstNodeType::Symbol, vec![nested_node.clone()]))
    }
}