use crate::domain::Node;

pub struct AstParser;

pub trait AstParserTrt {
    fn parse_sexp_new_domain(input: &[u8], offset: &mut usize) -> Node;
}

impl AstParserTrt for AstParser {
    fn parse_sexp_new_domain(input: &[u8], offset: &mut usize) -> Node {
        Self::parse_sexp_internal(input, offset, false)
    }
}

impl AstParser {
    fn parse_sexp_internal(input: &[u8], offset: &mut usize, inside_list: bool) -> Node {
        let mut buffer = String::new();
        let mut sexp = vec![];

        while *offset < input.len() {
            let c = input[*offset] as char;
            match c {
                '(' => {
                    *offset += 1;
                    sexp.push(Self::parse_sexp_internal(input, offset, true));
                }
                ')' => {
                    if !inside_list {
                        // Unopened parenthesis - panic
                        panic!("Unexpected closing parenthesis");
                    }
                    if !buffer.is_empty() {
                        sexp.push(Node::new_number(buffer.parse().unwrap()));
                    }
                    return Node::new_list_from_raw(sexp);
                }
                ' ' => {
                    if !buffer.is_empty() {
                        sexp.push(if sexp.is_empty() {
                            Node::Symbol { value: buffer }
                        } else {
                            Node::new_number(buffer.parse().unwrap())
                        });
                        buffer = String::new();
                    }
                }
                _ => {
                    buffer.push(c);
                }
            }
            *offset += 1;
        }

        // If we reach end of input
        if inside_list {
            // We're inside a list but never found closing parenthesis - panic
            panic!("Unclosed parenthesis");
        }

        if !buffer.is_empty() {
            Node::new_number(buffer.parse().unwrap())
        } else {
            sexp.first().unwrap().to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sexp_new_domain() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ 2 (* 3 4))", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: String::from("+")
                },
                Node::new_number(2),
                Node::new_list_from_raw(vec![
                    Node::Symbol {
                        value: String::from("*")
                    },
                    Node::new_number(3),
                    Node::new_number(4)
                ])
            ])
        );
    }

    #[test]
    fn parse_nested_expression() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ (+ (* 1 2) (* 3 4)))", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: String::from("+")
                },
                Node::new_list_from_raw(vec![
                    Node::Symbol {
                        value: String::from("+")
                    },
                    Node::new_list_from_raw(vec![
                        Node::Symbol {
                            value: String::from("*")
                        },
                        Node::new_number(1),
                        Node::new_number(2)
                    ]),
                    Node::new_list_from_raw(vec![
                        Node::Symbol {
                            value: String::from("*")
                        },
                        Node::new_number(3),
                        Node::new_number(4)
                    ])
                ])
            ])
        );
    }

    #[test]
    fn parse_single_number() {
        let parsed = AstParser::parse_sexp_new_domain(b"42", &mut 0);
        assert_eq!(parsed, Node::new_number(42));
    }

    #[test]
    #[should_panic]
    fn parse_single_symbol() {
        // This actually panics because the parser tries to parse "hello" as a number
        AstParser::parse_sexp_new_domain(b"hello", &mut 0);
    }

    #[test]
    fn parse_empty_list() {
        let parsed = AstParser::parse_sexp_new_domain(b"()", &mut 0);
        assert_eq!(parsed, Node::new_list_from_raw(vec![]));
    }

    #[test]
    fn parse_with_extra_spaces() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+   2    3)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: String::from("+")
                },
                Node::new_number(2),
                Node::new_number(3)
            ])
        );
    }

    #[test]
    fn parse_multiple_expressions() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ 1 2) (- 5 3)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: String::from("+")
                },
                Node::new_number(1),
                Node::new_number(2)
            ])
        );
    }

    #[test]
    #[should_panic]
    fn parse_invalid_number_in_list() {
        AstParser::parse_sexp_new_domain(b"(+ abc 2)", &mut 0);
    }

    #[test]
    #[should_panic]
    fn parse_invalid_standalone_number() {
        AstParser::parse_sexp_new_domain(b"abc123def", &mut 0);
    }

    #[test]
    #[should_panic]
    fn parse_empty_input() {
        AstParser::parse_sexp_new_domain(b"", &mut 0);
    }

    #[test]
    #[should_panic]
    fn parse_malformed_number() {
        AstParser::parse_sexp_new_domain(b"(+ 1.2.3 4)", &mut 0);
    }

    #[test]
    #[should_panic]
    fn parse_unmatched_parentheses() {
        // This panics because sexp is empty and we try to get first().unwrap()
        AstParser::parse_sexp_new_domain(b"( ", &mut 0);
    }

    #[test]
    #[should_panic]
    fn parse_unclosed_parenthesis_current_behavior() {
        // Currently this returns Node::new_number(1) instead of panicking
        AstParser::parse_sexp_new_domain(b"(+ 1 2", &mut 0);
    }

    #[test]
    #[should_panic]
    fn parse_unopened_parenthesis() {
        AstParser::parse_sexp_new_domain(b"2 )", &mut 0);
    }
}
