use crate::domain::{Node, Primitive};

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
    fn parse_sexp_internal(input: &[u8], offset: &mut usize, inside_container: bool) -> Node {
        Self::parse_container(input, offset, inside_container, false)
    }

    fn parse_container(
        input: &[u8],
        offset: &mut usize,
        inside_container: bool,
        is_vector: bool,
    ) -> Node {
        let mut buffer = String::new();
        let mut sexp = vec![];

        while *offset < input.len() {
            let c = input[*offset] as char;
            match c {
                '(' => {
                    if !buffer.is_empty() {
                        sexp.push(Self::parse_atom(&buffer));
                        buffer = String::new();
                    }
                    *offset += 1;
                    sexp.push(Self::parse_container(input, offset, true, false));
                }
                '[' => {
                    if !buffer.is_empty() {
                        sexp.push(Self::parse_atom(&buffer));
                        buffer = String::new();
                    }
                    *offset += 1;
                    sexp.push(Self::parse_container(input, offset, true, true));
                }
                '"' => {
                    if !buffer.is_empty() {
                        sexp.push(Self::parse_atom(&buffer));
                        buffer = String::new();
                    }
                    *offset += 1;
                    sexp.push(Self::parse_string_literal(input, offset));
                }
                ')' => {
                    if !inside_container || is_vector {
                        panic!("Unexpected closing parenthesis");
                    }
                    if !buffer.is_empty() {
                        sexp.push(Self::parse_atom(&buffer));
                    }
                    return Node::new_list_from_raw(sexp);
                }
                ']' => {
                    if !inside_container || !is_vector {
                        panic!("Unexpected closing bracket");
                    }
                    if !buffer.is_empty() {
                        sexp.push(Self::parse_atom(&buffer));
                    }
                    return Node::new_vector_from_raw(sexp);
                }
                c if c.is_whitespace() => {
                    if !buffer.is_empty() {
                        sexp.push(Self::parse_atom(&buffer));
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
        if inside_container {
            panic!("Unclosed container");
        }

        if !buffer.is_empty() {
            Self::parse_atom(&buffer)
        } else {
            sexp.first().unwrap().to_owned()
        }
    }

    fn parse_string_literal(input: &[u8], offset: &mut usize) -> Node {
        let mut buffer = String::new();
        let mut escape = false;

        while *offset < input.len() {
            let c = input[*offset] as char;

            if escape {
                match c {
                    'n' => buffer.push('\n'),
                    't' => buffer.push('\t'),
                    'r' => buffer.push('\r'),
                    '"' => buffer.push('"'),
                    '\\' => buffer.push('\\'),
                    _ => buffer.push(c), // Unknown escape sequence, just include the character
                }
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                // Found closing quote, offset is now at the closing quote
                // The main loop will increment it past the quote
                break;
            } else {
                buffer.push(c);
            }

            *offset += 1;
        }

        if *offset >= input.len() {
            panic!("Unterminated string literal");
        }

        // Don't increment offset here - let the main loop handle it

        Node::Primitive {
            value: Primitive::String(buffer),
        }
    }

    fn parse_atom(buffer: &str) -> Node {
        if let Ok(num) = buffer.parse::<usize>() {
            Node::new_number(num)
        } else {
            Node::Symbol {
                value: buffer.to_string(),
            }
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
    fn parse_single_symbol() {
        let parsed = AstParser::parse_sexp_new_domain(b"hello", &mut 0);
        assert_eq!(
            parsed,
            Node::Symbol {
                value: "hello".to_string()
            }
        );
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
    fn parse_symbol_in_list() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ abc 2)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: "+".to_string()
                },
                Node::Symbol {
                    value: "abc".to_string()
                },
                Node::new_number(2)
            ])
        );
    }

    #[test]
    fn parse_complex_symbol() {
        let parsed = AstParser::parse_sexp_new_domain(b"abc123def", &mut 0);
        assert_eq!(
            parsed,
            Node::Symbol {
                value: "abc123def".to_string()
            }
        );
    }

    #[test]
    #[should_panic]
    fn parse_empty_input() {
        AstParser::parse_sexp_new_domain(b"", &mut 0);
    }

    #[test]
    fn parse_symbol_with_dots() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ 1.2.3 4)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: "+".to_string()
                },
                Node::Symbol {
                    value: "1.2.3".to_string()
                },
                Node::new_number(4)
            ])
        );
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

    #[test]
    fn parse_vector() {
        let parsed = AstParser::parse_sexp_new_domain(b"[x 5 y 10]", &mut 0);
        assert_eq!(
            parsed,
            Node::new_vector_from_raw(vec![
                Node::Symbol {
                    value: "x".to_string()
                },
                Node::new_number(5),
                Node::Symbol {
                    value: "y".to_string()
                },
                Node::new_number(10)
            ])
        );
    }

    #[test]
    fn parse_let_expression() {
        let parsed = AstParser::parse_sexp_new_domain(b"(let [x 5] x)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: "let".to_string()
                },
                Node::new_vector_from_raw(vec![
                    Node::Symbol {
                        value: "x".to_string()
                    },
                    Node::new_number(5)
                ]),
                Node::Symbol {
                    value: "x".to_string()
                }
            ])
        );
    }

    #[test]
    fn parse_string_literal() {
        let parsed = AstParser::parse_sexp_new_domain(b"\"hello world\"", &mut 0);
        assert_eq!(
            parsed,
            Node::Primitive {
                value: Primitive::String("hello world".to_string())
            }
        );
    }

    #[test]
    fn parse_string_with_escapes() {
        let parsed = AstParser::parse_sexp_new_domain(b"\"hello\\nworld\\t!\"", &mut 0);
        assert_eq!(
            parsed,
            Node::Primitive {
                value: Primitive::String("hello\nworld\t!".to_string())
            }
        );
    }

    #[test]
    fn parse_string_with_quotes() {
        let parsed = AstParser::parse_sexp_new_domain(b"\"say \\\"hello\\\"\"", &mut 0);
        assert_eq!(
            parsed,
            Node::Primitive {
                value: Primitive::String("say \"hello\"".to_string())
            }
        );
    }

    #[test]
    fn parse_string_with_backslash() {
        let parsed = AstParser::parse_sexp_new_domain(b"\"path\\\\to\\\\file\"", &mut 0);
        assert_eq!(
            parsed,
            Node::Primitive {
                value: Primitive::String("path\\to\\file".to_string())
            }
        );
    }

    #[test]
    fn parse_string_in_list() {
        let parsed = AstParser::parse_sexp_new_domain(b"(print \"hello\")", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol {
                    value: "print".to_string()
                },
                Node::Primitive {
                    value: Primitive::String("hello".to_string())
                }
            ])
        );
    }

    #[test]
    fn parse_empty_string() {
        let parsed = AstParser::parse_sexp_new_domain(b"\"\"", &mut 0);
        assert_eq!(
            parsed,
            Node::Primitive {
                value: Primitive::String("".to_string())
            }
        );
    }

    #[test]
    #[should_panic]
    fn parse_unterminated_string() {
        AstParser::parse_sexp_new_domain(b"\"hello", &mut 0);
    }
}
