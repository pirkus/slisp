/// CLI commands for file compilation
use crate::ast_parser::{AstParser, AstParserTrt};
use crate::codegen::{compile_to_executable, detect_host_target};
use crate::compiler::compile_program;
use crate::domain::Node;
use crate::repl::format_compile_error;
use std::fs;
use std::process::Command;

/// Compile a .slisp file to an executable
pub fn compile_file_to_executable(input_file: &str, output_file: &str) -> Result<(), String> {
    let file_content = fs::read_to_string(input_file)
        .map_err(|e| format!("Failed to read file '{}': {}", input_file, e))?;

    let expressions = parse_all_expressions(&file_content)?;
    let ir_program = compile_program(&expressions).map_err(|e| format_compile_error(&e))?;

    let target = detect_host_target();
    let (machine_code, heap_init_offset) = compile_to_executable(&ir_program, target);

    target
        .generate_executable(&machine_code, output_file, heap_init_offset)
        .map_err(|e| format!("Failed to write executable: {}", e))?;

    Command::new("chmod")
        .args(["+x", output_file])
        .output()
        .map_err(|e| format!("Failed to chmod executable: {}", e))?;

    Ok(())
}

/// Parse all top-level expressions from file content
fn parse_all_expressions(file_content: &str) -> Result<Vec<Node>, String> {
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
                offset = skip_string_literal(bytes, offset)?;
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

/// Skip a string literal, handling escape sequences properly
fn skip_string_literal(bytes: &[u8], mut offset: usize) -> Result<usize, String> {
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
mod tests {
    use super::*;

    #[test]
    fn test_skip_whitespace() {
        assert_eq!(skip_whitespace(b"   hello", 0), 3);
        assert_eq!(skip_whitespace(b"hello", 0), 0);
        assert_eq!(skip_whitespace(b"\n\t  hello", 0), 4);
    }

    #[test]
    fn test_find_atom_end() {
        assert_eq!(find_atom_end(b"hello world", 0).unwrap(), 5);
        assert_eq!(find_atom_end(b"123)", 0).unwrap(), 3);
        assert_eq!(find_atom_end(b"symbol;comment", 0).unwrap(), 6);
    }

    #[test]
    fn test_skip_string_literal() {
        assert_eq!(skip_string_literal(br#""hello""#, 0).unwrap(), 7);
        assert_eq!(skip_string_literal(br#""hello \"world\"""#, 0).unwrap(), 17);
        assert!(skip_string_literal(br#""unterminated"#, 0).is_err());
    }

    #[test]
    fn test_find_list_end() {
        assert_eq!(find_list_end(b"(+ 1 2)", 0).unwrap(), 7);
        assert_eq!(find_list_end(b"(+ (- 3 1) 2)", 0).unwrap(), 13);
        assert!(find_list_end(b"(unclosed", 0).is_err());
        // Extra closing paren after balanced expression stops at first balanced point
        assert_eq!(find_list_end(b"(too many))", 0).unwrap(), 10);
    }

    #[test]
    fn test_find_list_end_with_strings() {
        assert_eq!(
            find_list_end(br#"(str "hello (not a paren)" "world")"#, 0).unwrap(),
            35
        );
        assert_eq!(
            find_list_end(br#"(str "escaped \" quote")"#, 0).unwrap(),
            24
        );
    }

    #[test]
    fn test_parse_multiple_expressions() {
        let input = "(defn add [x y] (+ x y))\n(defn -main [] (add 3 4))";
        let result = parse_all_expressions(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_with_comments() {
        let input = "; This is a comment\n(+ 1 2)\n; Another comment\n(* 3 4)";
        let result = parse_all_expressions(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }
}
