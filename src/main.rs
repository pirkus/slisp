mod ast_parser;
mod domain;

use crate::domain::AstNode;
use crate::domain::AstNodeType;

fn main() {
    // let mut buffer = String::new();
    // let stdin = io::stdin();
    let mut ast = AstNode {
        value: String::from("3"),
        node_type: AstNodeType::Num,
        nodes: Box::new(Vec::new()),
    };
    let ast2 = AstNode {
        value: String::from("1"),
        node_type: AstNodeType::Num,
        nodes: Box::new(Vec::new()),
    };
    ast.nodes.push(ast2);
    print(&ast);
    print(&ast.nodes[0]);
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

fn print(node: &AstNode) {
    println!("{0}", node.value)
}
