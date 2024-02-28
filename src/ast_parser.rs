use std::ops::Add;
use crate::domain::AstNode;
use crate::domain::AstNodeType;
use crate::domain::Errors;

struct AstParser {
    list_count: u32,
}

trait AstParserTrt {
    fn new() -> AstParser { AstParser { list_count: 0 } }
    fn parse_sexp(&mut self, input: &[u8], offset: usize, buffer: String, end_nodes: Vec<AstNode>) -> Vec<AstNode>;
    fn is_valid(str: String) -> Result<i32, Errors>;
}

impl AstParserTrt for AstParser {
    fn parse_sexp(&mut self, input: &[u8], offset: usize, buffer: String, end_nodes: Vec<AstNode>) -> Vec<AstNode> {
        if offset >= input.len() {
            return end_nodes;
        }
        match input[offset] as char {
            '(' => return {
                let mut list_name = String::from("$cons-");
                list_name.push(char::from_digit(self.list_count, 10).unwrap());
                self.list_count += 1;

                let mut new_end_nodes = end_nodes.clone();

                let result = self.parse_sexp(input, offset + 1, String::new(), vec![]);

                new_end_nodes.push(AstNode::new(list_name, AstNodeType::List, result));

                new_end_nodes
            },
            ')' => return {
                if !buffer.is_empty() {
                    let mut new_end_nodes = end_nodes.clone();
                    new_end_nodes.push(AstNode::new_end_node(buffer.clone(), if end_nodes.is_empty() { AstNodeType::Symbol } else { AstNodeType::Int }));

                    new_end_nodes
                } else {
                    end_nodes
                }
            },
            ' ' => return {
                if !buffer.is_empty() {
                    let mut new_end_nodes = end_nodes.clone();
                    new_end_nodes.push(AstNode::new_end_node(buffer.clone(), if end_nodes.is_empty() { AstNodeType::Symbol } else { AstNodeType::Int }));
                    self.parse_sexp(input, offset + 1, String::new(), new_end_nodes)
                } else {
                    self.parse_sexp(input, offset + 1, String::new(), end_nodes)
                }
            },
            _ => self.parse_sexp(input, offset + 1, buffer.add(&(input[offset] as char).to_string()), end_nodes)
        }
    }

    fn is_valid(str: String) -> Result<i32, Errors> {
        str
            .chars()
            .collect::<Vec<char>>()
            .iter()
            .map(|x| {
                match x {
                    '(' => Ok(1),
                    ')' => Ok(-1),
                    _ => Ok(0)
                }
            }).into_iter()
            .enumerate()
            .try_fold(0, |acc, (i, res)| {
                match res {
                    Ok(num) => {
                        if (acc + num == 0) && str.len() > i + 1 {
                            Err(Errors::InvalidSyntax)
                        } else {
                            Ok(acc + num)
                        }
                    }
                    Err(_) => res
                }
            }).and_then(|x| {
            if x == 0 {
                Ok(0)
            } else {
                Err(Errors::InvalidSyntax)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sexp() {
        // add a test for: "(+ (+ (* 1 2) (* 3   4)))"
        let mut binding = AstParser::new();
        let parsed = binding.parse_sexp("(+ 2 (* 3 4))".as_bytes(), 0, String::new(), vec![]);
        assert_eq!(
            parsed,
            vec![
                AstNode::new("$cons-0".to_string(), AstNodeType::List,
                             vec![
                                 AstNode::new_end_node("+".to_string(), AstNodeType::Symbol),
                                 AstNode::new_end_node("2".to_string(), AstNodeType::Int),
                                 AstNode::new("$cons-1".to_string(), AstNodeType::List,
                                              vec![
                                                  AstNode::new_end_node("*".to_string(), AstNodeType::Symbol),
                                                  AstNode::new_end_node("3".to_string(), AstNodeType::Int),
                                                  AstNode::new_end_node("4".to_string(), AstNodeType::Int),
                                              ],
                                 ),
                             ],
                )
            ]
        );
        print!("{:#?}", parsed)
    }

    #[test]
    fn is_valid() {
        assert_eq!(Ok(0), AstParser::is_valid(String::from("()")));
        assert_eq!(Ok(0), AstParser::is_valid(String::from("(()())")));
        assert_eq!(Err(Errors::InvalidSyntax), AstParser::is_valid(String::from(")")));
        assert_eq!(Err(Errors::InvalidSyntax), AstParser::is_valid(String::from("()()")));
        assert_eq!(Ok(0), AstParser::is_valid(String::from("(+ (* 1 2) (* 3 4))")));
    }
}