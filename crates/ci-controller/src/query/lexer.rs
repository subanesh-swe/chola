//! Logos-based ChQL lexer. Produces a flat stream of (token, span) pairs
//! consumed by the chumsky parser. Whitespace is skipped; the parser handles
//! implicit-AND adjacency via expression sequencing.
//!
//! Operator ordering matters: longer operators (`>=`, `<=`, `!=`) MUST appear
//! before their shorter prefixes so logos picks them first.
//!
//! Identifiers (bare values) intentionally include `.`, `/`, `+`, `-`, `*`
//! to support branch names like `feat/foo-1.2`, dates like `2026-04-01`, and
//! wildcard bares like `feat/*`. Keywords `AND`/`OR`/`NOT` are recognised
//! case-insensitively as distinct tokens.

use logos::Logos;

use super::error::ChqlError;

/// Result of tokenising a string literal — includes whether the string was
/// terminated. An unterminated string is reported as a parse error at the
/// opening quote position.
#[derive(Debug, Clone, PartialEq)]
pub struct StrLit {
    pub value: String,
    pub terminated: bool,
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
pub enum Token {
    // Two-character operators must come BEFORE single-char variants.
    #[token("!=")]
    BangEq,
    #[token(">=")]
    Gte,
    #[token("<=")]
    Lte,

    #[token("!")]
    Bang,
    #[token(">")]
    Gt,
    #[token("<")]
    Lt,

    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,

    // Keywords — case insensitive. Match before generic ident so they win.
    #[regex(r"(?i:and)", priority = 5)]
    And,
    #[regex(r"(?i:or)", priority = 5)]
    Or,
    #[regex(r"(?i:not)", priority = 5)]
    Not,

    /// Quoted string. Custom callback parses escapes and returns the raw
    /// payload + whether the closing quote was found. Span on the token
    /// always points at the opening quote so error positions are sane.
    #[regex(r#""([^"\\]|\\.)*"?"#, lex_string)]
    String(StrLit),

    /// Number literal. Optional sign + integer + optional fractional part.
    /// Excludes leading-`+` since `+` is a bare-ident char.
    #[regex(r"-?[0-9]+(\.[0-9]+)?", |lex| lex.slice().parse::<f64>().ok(), priority = 4)]
    Number(f64),

    /// Identifier / bare value. Accepts wide chars to cover branch names,
    /// dates, wildcards. First char restricted to alpha/_/digit/- to avoid
    /// swallowing operator chars by accident.
    #[regex(r"[A-Za-z0-9_\-][A-Za-z0-9_./+\-*]*", |lex| lex.slice().to_string(), priority = 2)]
    Ident(String),
}

/// Logos callback for string literals. Returns the *content* (after unescaping)
/// and whether the literal was terminated. Logos has already matched the
/// `"..."?` regex so the slice always starts with `"`.
fn lex_string(lex: &mut logos::Lexer<'_, Token>) -> StrLit {
    let slice = lex.slice();
    // slice always starts with '"' (logos matched it). It ends with '"' if
    // terminated, otherwise stops at EOI.
    let bytes = slice.as_bytes();
    let terminated = bytes.len() >= 2 && *bytes.last().unwrap() == b'"';
    // Strip the opening quote + (if present) the closing quote.
    let inner_end = if terminated {
        slice.len() - 1
    } else {
        slice.len()
    };
    let inner = &slice[1..inner_end];

    // Unescape: `\"` → `"`, `\\` → `\`, `\n` → newline, `\t` → tab.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    StrLit {
        value: out,
        terminated,
    }
}

/// Tokenise `input` into `(token, byte_span)` pairs. Returns an error if a
/// token slice fails to match anything (logos `Err`). Unterminated strings
/// are returned as successful `String` tokens with `terminated: false`; the
/// parser is responsible for raising the user-facing error.
pub fn tokenize(input: &str) -> Result<Vec<(Token, std::ops::Range<usize>)>, ChqlError> {
    let mut out = Vec::new();
    let mut lex = Token::lexer(input);
    while let Some(tok) = lex.next() {
        let span = lex.span();
        match tok {
            Ok(t) => out.push((t, span)),
            Err(_) => {
                // Logos couldn't match — report the offending byte.
                return Err(ChqlError::at(
                    format!("Unexpected character '{}'", &input[span.start..span.end]),
                    span.start,
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_field_term() {
        let toks = tokenize("state:failed").expect("lex");
        assert_eq!(toks.len(), 3);
        assert!(matches!(toks[0].0, Token::Ident(ref s) if s == "state"));
        assert!(matches!(toks[1].0, Token::Colon));
        assert!(matches!(toks[2].0, Token::Ident(ref s) if s == "failed"));
    }

    #[test]
    fn neq_legacy_alias() {
        let toks = tokenize("exit_code:!=0").expect("lex");
        // ident colon bang-eq number
        assert!(matches!(toks[2].0, Token::BangEq));
        assert!(matches!(toks[3].0, Token::Number(n) if (n - 0.0).abs() < f64::EPSILON));
    }

    #[test]
    fn neq_bang() {
        let toks = tokenize("exit_code:!0").expect("lex");
        assert!(matches!(toks[2].0, Token::Bang));
        assert!(matches!(toks[3].0, Token::Number(_)));
    }

    #[test]
    fn gte_before_gt() {
        let toks = tokenize("exit_code:>=1").expect("lex");
        assert!(matches!(toks[2].0, Token::Gte));
    }

    #[test]
    fn quoted_string_with_escape() {
        let toks = tokenize(r#"branch:"a\"b""#).expect("lex");
        match &toks[2].0 {
            Token::String(s) => {
                assert_eq!(s.value, "a\"b");
                assert!(s.terminated);
            }
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn unterminated_string_marker() {
        let toks = tokenize(r#"branch:"foo"#).expect("lex");
        match &toks[2].0 {
            Token::String(s) => {
                assert_eq!(s.value, "foo");
                assert!(!s.terminated);
            }
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn keywords_case_insensitive() {
        let toks = tokenize("a AND b or NOT c").expect("lex");
        let kinds: Vec<_> = toks.iter().map(|(t, _)| t).collect();
        assert!(matches!(kinds[1], Token::And));
        assert!(matches!(kinds[3], Token::Or));
        assert!(matches!(kinds[4], Token::Not));
    }

    #[test]
    fn bare_ident_with_dots_slashes() {
        let toks = tokenize("branch:feat/foo-1.2").expect("lex");
        assert!(matches!(&toks[2].0, Token::Ident(s) if s == "feat/foo-1.2"));
    }

    #[test]
    fn range_brackets() {
        let toks = tokenize("exit_code:[1,5]").expect("lex");
        assert!(matches!(toks[2].0, Token::LBracket));
        assert!(matches!(toks[3].0, Token::Number(_)));
        assert!(matches!(toks[4].0, Token::Comma));
        assert!(matches!(toks[5].0, Token::Number(_)));
        assert!(matches!(toks[6].0, Token::RBracket));
    }
}
