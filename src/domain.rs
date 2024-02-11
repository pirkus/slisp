#[derive(PartialEq)]
#[derive(Debug)]
pub enum Errors {
    InvalidSyntax
}

pub struct AstNode {
    pub value: String,
    pub node_type: AstNodeType,
    pub nodes: Vec<AstNode>,
}

pub enum AstNodeType {
    Num,
    Str,
    Fn,
    Kw,
}