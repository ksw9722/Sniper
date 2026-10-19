//! The expression language of Nuclei templates (`dsl:` matchers and `{{…}}`
//! markers). Nuclei evaluates it with govaluate plus about seventy helper
//! functions; Sniper implements the subset its templates are measured to use.
//!
//! Only the tokenizer exists so far: the classifier uses it to see which
//! functions and variables a template needs before anything can run.

/// A lexical token. Numbers and strings keep their source spelling until the
/// parser needs their value.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Ident(String),
    Number(String),
    Str(String),
    Op(&'static str),
}

/// Operators longest first, so `==` is not read as `=` `=` and `!~` not as `!`.
const OPERATORS: &[&str] = &[
    "**", "==", "!=", "<=", ">=", "&&", "||", "=~", "!~", "<<", ">>", "<", ">", "!", "+", "-", "*",
    "/", "%", "?", ":", "(", ")", ",", "[", "]", "&", "|", "^", "~",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub offset: usize,
    pub reason: &'static str,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            let (value, next) = read_string(source, index)?;
            tokens.push(Token::Str(value));
            index = next;
            continue;
        }
        if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'.')
            {
                index += 1;
            }
            tokens.push(Token::Number(source[start..index].to_string()));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token::Ident(source[start..index].to_string()));
            continue;
        }
        match OPERATORS
            .iter()
            .find(|operator| source[index..].starts_with(**operator))
        {
            Some(operator) => {
                tokens.push(Token::Op(operator));
                index += operator.len();
            }
            None => {
                return Err(LexError {
                    offset: index,
                    reason: "unexpected_character",
                })
            }
        }
    }
    Ok(tokens)
}

/// Reads a quoted string starting at `start`, returning its unescaped value and
/// the offset after the closing quote. A backslash escapes the next character,
/// as govaluate does; `\n`, `\r` and `\t` mean the control characters.
fn read_string(source: &str, start: usize) -> Result<(String, usize), LexError> {
    let quote = source.as_bytes()[start] as char;
    let mut value = String::new();
    let mut chars = source[start + 1..].char_indices();
    while let Some((offset, character)) = chars.next() {
        match character {
            '\\' => match chars.next() {
                Some((_, 'n')) => value.push('\n'),
                Some((_, 'r')) => value.push('\r'),
                Some((_, 't')) => value.push('\t'),
                Some((_, escaped)) => value.push(escaped),
                None => break,
            },
            c if c == quote => return Ok((value, start + 1 + offset + 1)),
            c => value.push(c),
        }
    }
    Err(LexError {
        offset: start,
        reason: "unterminated_string",
    })
}

/// The functions an expression calls and the bare names it reads, in source
/// order. `true`, `false` and `nil` are literals, `in` an operator.
pub fn names(tokens: &[Token]) -> (Vec<String>, Vec<String>) {
    let mut functions = Vec::new();
    let mut variables = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let Token::Ident(name) = token else { continue };
        if matches!(name.as_str(), "true" | "false" | "nil" | "in") {
            continue;
        }
        if tokens.get(index + 1) == Some(&Token::Op("(")) {
            functions.push(name.clone());
        } else {
            variables.push(name.clone());
        }
    }
    (functions, variables)
}

/// The outermost `{{…}}` markers in a template string, without their braces.
/// Markers nest — `{{url_encode("{{command}}")}}` fills the inner one first —
/// so braces are matched by depth. An unterminated `{{` is not a marker, as
/// in Nuclei.
pub fn markers(text: &str) -> Vec<&str> {
    marker_spans(text)
        .into_iter()
        .map(|(start, end)| text[start + 2..end - 2].trim())
        .collect()
}

/// The text with every marker removed, for reading an expression whose inner
/// markers are filled before it is evaluated.
pub fn without_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (start, end) in marker_spans(text) {
        out.push_str(&text[copied..start]);
        copied = end;
    }
    out.push_str(&text[copied..]);
    out
}

/// Byte ranges of the outermost complete markers, braces included.
fn marker_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut index = 0;
    while index + 1 < bytes.len() {
        match &bytes[index..index + 2] {
            b"{{" => {
                if depth == 0 {
                    start = index;
                }
                depth += 1;
                index += 2;
            }
            b"}}" if depth > 0 => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    spans.push((start, index));
                }
            }
            _ => index += 1,
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_operators_strings_and_calls() {
        let tokens =
            tokenize(r#"status_code==200 && !contains(tolower(body), '<html') || x !~ "a\"b""#)
                .unwrap();
        assert_eq!(tokens[0], Token::Ident("status_code".into()));
        assert_eq!(tokens[1], Token::Op("=="));
        assert_eq!(tokens[2], Token::Number("200".into()));
        assert_eq!(tokens[3], Token::Op("&&"));
        assert_eq!(tokens[4], Token::Op("!"));
        assert!(tokens.contains(&Token::Str("<html".into())));
        assert!(tokens.contains(&Token::Op("!~")));
        assert_eq!(tokens.last(), Some(&Token::Str("a\"b".into())));
        let (functions, variables) = names(&tokens);
        assert_eq!(functions, ["contains", "tolower"]);
        assert_eq!(variables, ["status_code", "body", "x"]);
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert_eq!(tokenize("'open").unwrap_err().reason, "unterminated_string");
        assert_eq!(
            tokenize("a @ b").unwrap_err().reason,
            "unexpected_character"
        );
    }

    #[test]
    fn finds_outer_markers_and_ignores_unterminated_ones() {
        assert_eq!(
            markers("{{BaseURL}}/a?x={{ url_encode(\"{{command}}\") }}&z={{open"),
            ["BaseURL", "url_encode(\"{{command}}\")"]
        );
        assert_eq!(
            without_markers("url_encode(\"{{command}}\")"),
            "url_encode(\"\")"
        );
        assert_eq!(without_markers("a{{b}}c{{d}}e"), "ace");
    }
}
