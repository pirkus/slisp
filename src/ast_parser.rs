use crate::domain::AstNode;
use crate::domain::AstNodeType;
use std::collections::VecDeque;
use crate::domain::Errors;

struct AstParser;
impl AstParser {
    pub fn parse(&self) -> AstNode {
        return AstNode {
            value: String::from("3"),
            node_type: AstNodeType::Num,
            nodes: Vec::new(),
        };
    }

    pub fn is_valid(str: String) -> Result<i32, Errors> {
        let mut z: VecDeque<char> = str.chars().collect();
        let mut par_count: i32 = 0;
        while(!z.is_empty()) {
            match  z.pop_front().unwrap() {
                '(' => par_count += 1,
                ')' => {
                    par_count -= 1; 
                    if par_count == 0 && !z.is_empty() {
                        return Err(Errors::InvalidSyntax)
                    }
                },
                _ => (),
            }
        }

        if par_count != 0 {
            return Err(Errors::InvalidSyntax)
        } else {
            return Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_valid_return_true_or_false() {
        assert_eq!(Ok(0), AstParser::is_valid(String::from("()")));
        assert_eq!(Err(Errors::InvalidSyntax), AstParser::is_valid(String::from(")")));
        assert_eq!(Err(Errors::InvalidSyntax), AstParser::is_valid(String::from("()()")));
        assert_eq!(Ok(0), AstParser::is_valid(String::from("(let (n 2) n*2)")));
    }
}