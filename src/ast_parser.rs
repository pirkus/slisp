use crate::domain::AstNode;
use crate::domain::AstNodeType;
use crate::domain::Errors;

struct AstParser;
trait AstParserTrt {
     fn parse(str: String) -> AstNode;

     fn is_valid(str: String) -> Result<i32, Errors>;
}

impl AstParserTrt for AstParser {
     fn parse(input: String) -> AstNode {
        return AstNode {
            value: String::from("3"),
            node_type: AstNodeType::Num,
            nodes: Vec::new(),
        };
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
    fn is_valid_return_true_or_false() {
        assert_eq!(Ok(0), AstParser::is_valid(String::from("()")));
        assert_eq!(Ok(0), AstParser::is_valid(String::from("(()())")));
        assert_eq!(Err(Errors::InvalidSyntax), AstParser::is_valid(String::from(")")));
        assert_eq!(Err(Errors::InvalidSyntax), AstParser::is_valid(String::from("()()")));
        assert_eq!(Ok(0), AstParser::is_valid(String::from("(let (n 2) n*2)")));
    }
}