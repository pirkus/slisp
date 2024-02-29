use std::ops::Add;
use crate::domain::AstNode;
use crate::domain::AstNodeType;
use crate::domain::Errors;

struct AstParser;

trait AstParserTrt {
    fn parse_sexp<'a>(input: &[u8], offset: usize, buffer: String, end_nodes: &'a mut Vec<AstNode>) -> &'a mut Vec<AstNode>;
    fn is_valid(str: String) -> Result<i32, Errors>;
}

impl AstParserTrt for AstParser {
    fn parse_sexp<'a>(input: &[u8], offset: usize, buffer: String, end_nodes: &'a mut Vec<AstNode>) -> &'a mut Vec<AstNode> {
        if offset >= input.len() {
            return end_nodes;
        }
        match input[offset] as char {
            '(' => {
                let nodes = &mut vec![];
                let result = AstParser::parse_sexp(input, offset + 1, String::new(), nodes);
                end_nodes.push(AstNode::new("$cons$".to_string(), AstNodeType::List, result.clone()));
                end_nodes
            }
            ')' => {
                if !buffer.is_empty() {
                    end_nodes.push(AstNode::new_end_node(buffer.clone(), if end_nodes.is_empty() { AstNodeType::Symbol } else { AstNodeType::Int }));
                    end_nodes
                } else {
                    end_nodes
                }
            }
            ' ' => {
                if !buffer.is_empty() {
                    end_nodes.push(AstNode::new_end_node(buffer.clone(), if end_nodes.is_empty() { AstNodeType::Symbol } else { AstNodeType::Int }));
                    AstParser::parse_sexp(input, offset + 1, String::new(), end_nodes)
                } else {
                    AstParser::parse_sexp(input, offset + 1, String::new(), end_nodes)
                }
            }
            _ => AstParser::parse_sexp(input, offset + 1, buffer.add(&(input[offset] as char).to_string()), end_nodes)
        }
    }

    // fn next_cons(&mut self) -> String {
    //     let mut list_name = String::from("$cons-");
    //     list_name.push(char::from_digit(self.list_count, 10).unwrap());
    //     self.list_count += 1;
    //
    //     list_name
    // }

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
        let nodes = &mut vec![];
        let parsed = AstParser::parse_sexp("(+ 2 (* 3 4))".as_bytes(), 0, String::new(), nodes);
        assert_eq!(
            parsed,
            &vec![
                AstNode::new("$cons$".to_string(), AstNodeType::List,
                             vec![
                                 AstNode::new_end_node("+".to_string(), AstNodeType::Symbol),
                                 AstNode::new_end_node("2".to_string(), AstNodeType::Int),
                                 AstNode::new("$cons$".to_string(), AstNodeType::List,
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