use super::{Node, Primitive};

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

    fn parse_container(input: &[u8], offset: &mut usize, inside_container: bool, is_vector: bool) -> Node {
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

        Node::Primitive { value: Primitive::String(buffer) }
    }

    fn parse_atom(buffer: &str) -> Node {
        if let Ok(num) = buffer.parse::<usize>() {
            Node::new_number(num)
        } else {
            Node::Symbol { value: buffer.to_string() }
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
                Node::Symbol { value: String::from("+") },
                Node::new_number(2),
                Node::new_list_from_raw(vec![Node::Symbol { value: String::from("*") }, Node::new_number(3), Node::new_number(4)])
            ])
        );
    }

    #[test]
    fn parse_nested_expression() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ (+ (* 1 2) (* 3 4)))", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![
                Node::Symbol { value: String::from("+") },
                Node::new_list_from_raw(vec![
                    Node::Symbol { value: String::from("+") },
                    Node::new_list_from_raw(vec![Node::Symbol { value: String::from("*") }, Node::new_number(1), Node::new_number(2)]),
                    Node::new_list_from_raw(vec![Node::Symbol { value: String::from("*") }, Node::new_number(3), Node::new_number(4)])
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
        assert_eq!(parsed, Node::Symbol { value: "hello".to_string() });
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
            Node::new_list_from_raw(vec![Node::Symbol { value: String::from("+") }, Node::new_number(2), Node::new_number(3)])
        );
    }

    #[test]
    fn parse_multiple_expressions() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ 1 2) (- 5 3)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![Node::Symbol { value: String::from("+") }, Node::new_number(1), Node::new_number(2)])
        );
    }

    #[test]
    fn parse_symbol_in_list() {
        let parsed = AstParser::parse_sexp_new_domain(b"(+ abc 2)", &mut 0);
        assert_eq!(
            parsed,
            Node::new_list_from_raw(vec![Node::Symbol { value: "+".to_string() }, Node::Symbol { value: "abc".to_string() }, Node::new_number(2)])
        );
    }

    #[test]
    fn parse_complex_symbol() {
        let parsed = AstParser::parse_sexp_new_domain(b"abc123def", &mut 0);
        assert_eq!(parsed, Node::Symbol { value: "abc123def".to_string() });
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
            Node::new_list_from_raw(vec![Node::Symbol { value: "+".to_string() }, Node::Symbol { value: "1.2.3".to_string() }, Node::new_number(4)])
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
                Node::Symbol { value: "x".to_string() },
                Node::new_number(5),
                Node::Symbol { value: "y".to_string() },
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
                Node::Symbol { value: "let".to_string() },
                Node::new_vector_from_raw(vec![Node::Symbol { value: "x".to_string() }, Node::new_number(5)]),
                Node::Symbol { value: "x".to_string() }
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
                Node::Symbol { value: "print".to_string() },
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

// ============================================================================
// Multi-expression file parsing
// ============================================================================

/// Parse all top-level expressions from file content
///
/// This handles:
/// - Multiple top-level expressions
/// - Comments (lines starting with ';')
/// - Whitespace between expressions
/// - Proper nesting of parentheses
/// - String literals with escapes
pub fn parse_file(file_content: &str) -> Result<Vec<Node>, String> {
    let bytes = file_content.as_bytes();
    let mut expressions = Vec::new();
    let mut offset = 0;

    while offset < bytes.len() {
        offset = skip_whitespace(bytes, offset);

        if offset >= bytes.len() {
            break;
        }

        let start = offset;
        offset = find_expression_end(bytes, offset)?;

        let expression_text = &file_content[start..offset];
        let mut parse_offset = 0;
        let ast = AstParser::parse_sexp_new_domain(expression_text.as_bytes(), &mut parse_offset);
        expressions.push(ast);
    }

    if expressions.is_empty() {
        Err("No expressions found in file".to_string())
    } else {
        Ok(expressions)
    }
}

/// Skip whitespace characters and return the next non-whitespace position
fn skip_whitespace(bytes: &[u8], mut offset: usize) -> usize {
    while offset < bytes.len() && is_whitespace(bytes[offset]) {
        offset += 1;
    }
    offset
}

/// Find the end of a single top-level expression
fn find_expression_end(bytes: &[u8], offset: usize) -> Result<usize, String> {
    match bytes[offset] {
        b'(' => find_list_end(bytes, offset),
        b';' => {
            // Skip comments at top level
            let next_offset = skip_comment(bytes, offset);
            // Recursively find the next expression after the comment
            if next_offset < bytes.len() {
                let next_offset = skip_whitespace(bytes, next_offset);
                if next_offset < bytes.len() {
                    return find_expression_end(bytes, next_offset);
                }
            }
            Ok(next_offset)
        }
        _ => find_atom_end(bytes, offset),
    }
}

/// Find the end of a list expression (parenthesized form)
fn find_list_end(bytes: &[u8], mut offset: usize) -> Result<usize, String> {
    let mut depth = 0;

    while offset < bytes.len() {
        match bytes[offset] {
            b'(' => {
                depth += 1;
                offset += 1;
            }
            b')' => {
                depth -= 1;
                offset += 1;
                if depth == 0 {
                    return Ok(offset);
                }
                if depth < 0 {
                    return Err(format!("Unmatched closing parenthesis at byte {}", offset));
                }
            }
            b'"' => {
                offset = skip_string_literal_boundary(bytes, offset)?;
            }
            b';' => {
                offset = skip_comment(bytes, offset);
            }
            _ => {
                offset += 1;
            }
        }
    }

    if depth > 0 {
        Err(format!("Unclosed parenthesis: expected {} more ')'", depth))
    } else {
        Ok(offset)
    }
}

/// Find the end of an atom (number, symbol, or other non-list token)
fn find_atom_end(bytes: &[u8], mut offset: usize) -> Result<usize, String> {
    while offset < bytes.len() {
        let b = bytes[offset];
        if is_whitespace(b) || b == b'(' || b == b')' || b == b';' {
            break;
        }
        offset += 1;
    }
    Ok(offset)
}

/// Skip a string literal boundary (for finding expression ends)
/// This is different from parse_string_literal - it just skips past the string
fn skip_string_literal_boundary(bytes: &[u8], mut offset: usize) -> Result<usize, String> {
    // Skip opening quote
    offset += 1;

    while offset < bytes.len() {
        match bytes[offset] {
            b'\\' => {
                // Skip escape sequence (backslash + next character)
                offset += 2;
                if offset > bytes.len() {
                    return Err("Unterminated string: escape sequence at end of input".to_string());
                }
            }
            b'"' => {
                // Found closing quote
                offset += 1;
                return Ok(offset);
            }
            _ => {
                offset += 1;
            }
        }
    }

    Err("Unterminated string literal".to_string())
}

/// Skip a comment (from ';' to end of line)
fn skip_comment(bytes: &[u8], mut offset: usize) -> usize {
    while offset < bytes.len() && bytes[offset] != b'\n' {
        offset += 1;
    }
    offset
}

/// Check if a byte is ASCII whitespace
#[inline]
fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

#[cfg(test)]
mod file_parser_tests {
    use super::*;

    #[test]
    fn test_parse_multiple_expressions() {
        let input = "(defn add [x y] (+ x y))\n(defn -main [] (add 3 4))";
        let result = parse_file(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_with_comments() {
        let input = "; This is a comment\n(+ 1 2)\n; Another comment\n(* 3 4)";
        let result = parse_file(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_empty_file() {
        let input = "";
        let result = parse_file(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "No expressions found in file");
    }

    #[test]
    fn test_parse_only_whitespace() {
        let input = "   \n\t  \n  ";
        let result = parse_file(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_inline_comments() {
        let input = "(+ 1 2) ; inline comment\n(* 3 4)";
        let result = parse_file(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}
