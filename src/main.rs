mod ast_parser;
mod domain;

use crate::domain::AstNode;
use crate::domain::AstNodeType;

fn main() {
    // let mut buffer = String::new();
    // let stdin = io::stdin();
    let ast = AstNode::new(String::from("3"), AstNodeType::Int,  vec![]);
    let ast2 = AstNode::new(String::from("1"), AstNodeType::Int, vec![ast]);
    println!("Display: {:#?}", ast2);
    // while stdin.read_line(&mut buffer).is_ok() {
    //     // Trim end.
    //     let trimmed = buffer.trim_end();
    //     if trimmed == "exit" {
    //         exit(1)
    //     }
    //     println!("You typed: [{trimmed}]");
    //     buffer.clear();
    // }
}
