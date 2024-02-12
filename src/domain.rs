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
    pub nodes: Box<Vec<AstNode>>,
}

impl AstNode {
    pub fn new(value: String, node_type: AstNodeType, nodes: Vec<AstNode>) -> AstNode {
        AstNode { value, node_type: node_type, nodes: Box::new(nodes) }
    }
}

#[derive(PartialEq)]
#[derive(Debug)]
#[derive(Clone)]
pub enum AstNodeType {
    Num,
    // Str,
    Fn,
    // Kw,
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn ast_is_constructable() {
        let s = String::from("root_node");
        let nested_node = AstNode { value: String::from("nested"), node_type: AstNodeType::Fn, nodes: Box::new(vec![]) };
        assert_eq!(AstNode { value: s.clone(), node_type: AstNodeType::Fn, nodes: Box::new(vec![nested_node.clone()]) }, AstNode::new(s.clone(), AstNodeType::Fn, vec![nested_node.clone()]))
    }
}