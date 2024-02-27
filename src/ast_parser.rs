
use crate::domain::AstNode;
use crate::domain::AstNodeType;
use crate::domain::Errors;

struct AstParser {
    list_count: u32
}
trait AstParserTrt {
    fn new() -> AstParser { AstParser { list_count: 0 } }
    fn parse_sexp(&mut self, input: &[u8], offset: usize) -> (usize, Vec<AstNode>);
    fn is_valid(str: String) -> Result<i32, Errors>;
}

impl AstParserTrt for AstParser {
    fn parse_sexp(&mut self, input: &[u8], offset: usize) -> (usize, Vec<AstNode>) {
        let mut result: Vec<AstNode> = Vec::new();
        let mut buffer = String::new();
        for i in offset..input.len() {
            match input[i] as char {
                '(' => return {
                    let (new_index, children) = self.parse_sexp(input, i + 1);
                    let mut list_name = String::from("$cons-");
                    list_name.push(char::from_digit(self.list_count, 10).unwrap());
                    self.list_count += 1;
                    result.push(AstNode::new(list_name, AstNodeType::List, children));
                    (new_index+1, result)
                },
                ')' => return {
                    result.push(AstNode::new_end_node(buffer.clone(), if result.is_empty() {AstNodeType::Symbol} else {AstNodeType::Int}));
                    (i, result)
                },
                ' ' => {
                    if !buffer.is_empty() { 
                        result.push(AstNode::new_end_node(buffer.clone(), if result.is_empty() {AstNodeType::Symbol} else {AstNodeType::Int}))
                    }
                    buffer = String::new();
                },
                _ => buffer.push(input[i] as char)
            }
        }

        panic!("Invalid syntax.")
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
                    },
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
        let parsed = binding.parse_sexp("(+ 2 (* 3 4))".as_bytes(), 0).1;
        assert_eq!(
            parsed, 
            vec![
                AstNode::new("$cons-1".to_string(), AstNodeType::List,
                    vec![
                        AstNode::new_end_node("+".to_string(), AstNodeType::Symbol),
                        AstNode::new_end_node("2".to_string(), AstNodeType::Int),
                        AstNode::new("$cons-0".to_string(), AstNodeType::List, 
                            vec![
                                AstNode::new_end_node("*".to_string(), AstNodeType::Symbol),
                                AstNode::new_end_node("3".to_string(), AstNodeType::Int),
                                AstNode::new_end_node("4".to_string(), AstNodeType::Int)
                            ]
                        )
                    ]
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