use crate::domain::Node;

struct AstParser;

trait AstParserTrt {
    fn parse_sexp_new_domain(input: &[u8], offset: &mut usize) -> Node;
}

impl AstParserTrt for AstParser {
    fn parse_sexp_new_domain(input: &[u8], offset: &mut usize) -> Node {
        let mut buffer = String::new();
        let mut sexp = vec![];
        while *offset < input.len() { 
            let c = input[*offset] as char;
            match c {
                '(' => {
                    *offset += 1;
                    sexp.push(AstParser::parse_sexp_new_domain(input, offset));
                },
                ')' => {
                    if !buffer.is_empty() {
                        sexp.push(Node::new_number(buffer.parse().unwrap()));
                    }
                    return Node::new_list_from_raw(sexp)
                },
                ' ' => {
                    if !buffer.is_empty() {
                        sexp.push(if sexp.is_empty() {Node::Symbol { value: buffer }} else { Node::new_number(buffer.parse().unwrap()) });
                        buffer = String::new();
                    }
                },
                _ => {
                    buffer.push(c);
                }
            }
            *offset += 1;
        }

        sexp.first().unwrap().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sexp_new_domain() {
        // add a test for: "(+ (+ (* 1 2) (* 3   4)))"
// tests for invalid strings ZA
        let parsed = AstParser::parse_sexp_new_domain(b"(+ 2 (* 3 4))", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol { value: String::from("+") },
                Node::new_number(2),
                Node::new_list_from_raw(vec![
                    Node::Symbol { value: String::from("*") },
                    Node::new_number(3),
                    Node::new_number(4)
                ])
            ])
        );
        println!("{:#?}", parsed)
    }
}