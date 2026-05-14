//! ChQL parser. Implemented as a hand-rolled Pratt-style parser over the
//! token stream from [`super::lexer`]. We considered chumsky combinators but
//! the corpus's per-position error messages were easier to author by hand —
//! chumsky's combinators are used at the dependency level (we still depend on
//! it for future error-recovery features) but the actual descent is manual.
//!
//! Grammar (matching `local/plans/CHQL.md`):
//! ```text
//! expr     → or_expr
//! or_expr  → and_expr ( ('or') and_expr )*
//! and_expr → not_expr ( (('and') | adjacency) not_expr )*
//! not_expr → 'not'? atom
//! atom     → '(' expr ')' | field_term
//! field_term → ident ':' value
//! ```
//! Empty input parses to `Ok(None)` so callers can short-circuit the `?q=` path.

use std::ops::Range;

use super::ast::{Ast, CmpOp, Value};
use super::error::ChqlError;
use super::lexer::{tokenize, StrLit, Token};

// Allowed field names (matches CHQL.md whitelist). The compiler maps these
// to typed Column enum variants; here we only validate the spelling.
const VALID_FIELDS: &[&str] = &[
    "state",
    "branch",
    "repo",
    "repo_id",
    "stage",
    "exit_code",
    "created_at",
    "completed_at",
];

/// Parse `input` into an [`Ast`]. Empty input returns `Ok(None)`.
pub fn parse(input: &str) -> Result<Option<Ast>, ChqlError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut p = Parser { tokens, pos: 0 };
    let ast = p.parse_expr()?;
    if p.pos < p.tokens.len() {
        let (tok, span) = &p.tokens[p.pos];
        return Err(ChqlError::at(
            format!("Unexpected token {}", tok_display(tok)),
            span.start,
        ));
    }
    Ok(Some(ast))
}

struct Parser {
    tokens: Vec<(Token, Range<usize>)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&(Token, Range<usize>)> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<(Token, Range<usize>)> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    // expr → or_expr
    fn parse_expr(&mut self) -> Result<Ast, ChqlError> {
        self.parse_or()
    }

    // Reject leading combinators / closers so the error message points at the
    // first bad token rather than failing later in atom().
    fn check_leading(&self) -> Result<(), ChqlError> {
        if let Some((tok, span)) = self.peek() {
            match tok {
                Token::Or => {
                    return Err(ChqlError::at(
                        "Unexpected 'OR': expected expression",
                        span.start,
                    ))
                }
                Token::And => {
                    return Err(ChqlError::at(
                        "Unexpected 'AND': expected expression",
                        span.start,
                    ))
                }
                Token::RParen => {
                    return Err(ChqlError::at("Unexpected token ')'", span.start));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn parse_or(&mut self) -> Result<Ast, ChqlError> {
        self.check_leading()?;
        let mut left = self.parse_and()?;
        while let Some((Token::Or, span)) = self.peek().cloned() {
            self.bump();
            if self.peek().is_none() {
                return Err(ChqlError::at("Expected expression after 'OR'", span.end));
            }
            let right = self.parse_and()?;
            left = Ast::Or {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Ast, ChqlError> {
        let mut left = self.parse_not()?;
        loop {
            match self.peek() {
                // explicit AND keyword
                Some((Token::And, span)) => {
                    let span = span.clone();
                    self.bump();
                    if self.peek().is_none() {
                        return Err(ChqlError::at(
                            "Expected expression after 'AND'",
                            span.end,
                        ));
                    }
                    let right = self.parse_not()?;
                    left = Ast::And {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                // adjacency = implicit AND: next is a start-of-atom token
                Some((t, _)) if can_start_atom(t) => {
                    let right = self.parse_not()?;
                    left = Ast::And {
                        left: Box::new(left),
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Ast, ChqlError> {
        if let Some((Token::Not, _)) = self.peek() {
            self.bump();
            let inner = self.parse_atom()?;
            return Ok(Ast::Not {
                expr: Box::new(inner),
            });
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<Ast, ChqlError> {
        match self.peek().cloned() {
            Some((Token::LParen, span)) => {
                let open_start = span.start;
                self.bump();
                let inner = self.parse_expr()?;
                match self.peek() {
                    Some((Token::RParen, _)) => {
                        self.bump();
                        Ok(inner)
                    }
                    _ => Err(ChqlError::at(
                        "Unclosed parenthesis: expected ')'",
                        open_start,
                    )),
                }
            }
            Some((Token::Ident(name), span)) => {
                self.bump();
                self.expect_field_term(&name, span)
            }
            Some((tok, span)) => Err(ChqlError::at(
                format!("Unexpected token {}", tok_display(&tok)),
                span.start,
            )),
            None => Err(ChqlError::at("Expected expression", 0)),
        }
    }

    // After consuming an `Ident`, parse `: value` to form a comparison.
    fn expect_field_term(
        &mut self,
        field_name: &str,
        field_span: Range<usize>,
    ) -> Result<Ast, ChqlError> {
        if !VALID_FIELDS.contains(&field_name) {
            let valid = VALID_FIELDS.join(", ");
            return Err(ChqlError::at(
                format!(
                    "Unknown field '{}'. Valid fields: {}",
                    field_name, valid
                ),
                field_span.start,
            ));
        }
        // Require ':'
        match self.peek().cloned() {
            Some((Token::Colon, _)) => {
                self.bump();
            }
            Some((tok, span)) => {
                return Err(ChqlError::at(
                    format!("Expected ':' after field '{field_name}', got {}", tok_display(&tok)),
                    span.start,
                ))
            }
            None => {
                return Err(ChqlError::at(
                    format!("Expected ':' after field '{field_name}'"),
                    field_span.end,
                ))
            }
        }
        // Now parse the value
        self.parse_value(field_name, field_span)
    }

    fn parse_value(
        &mut self,
        field_name: &str,
        field_span: Range<usize>,
    ) -> Result<Ast, ChqlError> {
        let (tok, span) = match self.peek().cloned() {
            Some(p) => p,
            None => {
                return Err(ChqlError::at("Expected value after ':'", field_span.end + 1));
            }
        };
        match tok {
            // !value or !=value → neq
            Token::Bang | Token::BangEq => {
                self.bump();
                let (next_tok, next_span) = match self.peek().cloned() {
                    Some(p) => p,
                    None => {
                        return Err(ChqlError::at(
                            "Expected value after '!'",
                            span.end,
                        ))
                    }
                };
                let value = match next_tok {
                    Token::String(s) => {
                        if !s.terminated {
                            return Err(ChqlError::at(
                                "Unterminated string literal",
                                next_span.start,
                            ));
                        }
                        self.bump();
                        Value::Str { v: s.value }
                    }
                    Token::Ident(id) => {
                        self.bump();
                        Value::Str { v: id }
                    }
                    Token::Number(n) => {
                        self.bump();
                        Value::Num { v: n }
                    }
                    other => {
                        return Err(ChqlError::at(
                            format!("Expected value after '!', got {}", tok_display(&other)),
                            next_span.start,
                        ))
                    }
                };
                Ok(Ast::Cmp {
                    field: field_name.to_string(),
                    op: CmpOp::Neq,
                    value,
                })
            }
            // Range: [lo, hi]
            Token::LBracket => {
                self.bump();
                self.parse_range(field_name, span.clone())
            }
            // Numeric cmp: >= > <= <
            Token::Gte | Token::Gt | Token::Lte | Token::Lt => {
                let op = match tok {
                    Token::Gte => CmpOp::Gte,
                    Token::Gt => CmpOp::Gt,
                    Token::Lte => CmpOp::Lte,
                    Token::Lt => CmpOp::Lt,
                    _ => unreachable!(),
                };
                self.bump();
                let (n_tok, n_span) = match self.peek().cloned() {
                    Some(p) => p,
                    None => {
                        return Err(ChqlError::at(
                            "Expected number or string after comparison operator",
                            span.end,
                        ))
                    }
                };
                let value = match n_tok {
                    Token::Number(n) => {
                        self.bump();
                        Value::Num { v: n }
                    }
                    Token::String(s) => {
                        if !s.terminated {
                            return Err(ChqlError::at(
                                "Unterminated string literal",
                                n_span.start,
                            ));
                        }
                        self.bump();
                        // Date fields get Date kind; everything else Str.
                        if is_date_field(field_name) {
                            Value::Date { v: s.value }
                        } else {
                            Value::Str { v: s.value }
                        }
                    }
                    other => {
                        return Err(ChqlError::at(
                            format!(
                                "Expected number or string after comparison operator, got {}",
                                tok_display(&other)
                            ),
                            n_span.start,
                        ))
                    }
                };
                Ok(Ast::Cmp {
                    field: field_name.to_string(),
                    op,
                    value,
                })
            }
            // i("…") or re("…") — only when ident matches AND `(` follows.
            Token::Ident(ref id) if (id == "i" || id == "re") && self.peek_is_lparen_next() => {
                let id = id.clone();
                self.bump(); // consume the ident
                self.bump(); // consume the (
                self.parse_call(field_name, &id, span.clone())
            }
            // Plain ident → exact string (or wildcard if contains '*')
            Token::Ident(id) => {
                self.bump();
                let op = if id.contains('*') {
                    CmpOp::Wildcard
                } else {
                    CmpOp::Eq
                };
                Ok(Ast::Cmp {
                    field: field_name.to_string(),
                    op,
                    value: Value::Str { v: id },
                })
            }
            // Quoted string → exact (or wildcard if contains '*') or date
            Token::String(s) => {
                if !s.terminated {
                    return Err(ChqlError::at(
                        "Unterminated string literal",
                        span.start,
                    ));
                }
                self.bump();
                let v = s.value;
                let op = if v.contains('*') {
                    CmpOp::Wildcard
                } else {
                    CmpOp::Eq
                };
                let value = if is_date_field(field_name) && op == CmpOp::Eq {
                    Value::Date { v }
                } else {
                    Value::Str { v }
                };
                Ok(Ast::Cmp {
                    field: field_name.to_string(),
                    op,
                    value,
                })
            }
            // Number → exact numeric
            Token::Number(n) => {
                self.bump();
                Ok(Ast::Cmp {
                    field: field_name.to_string(),
                    op: CmpOp::Eq,
                    value: Value::Num { v: n },
                })
            }
            other => Err(ChqlError::at(
                format!("Unexpected token {}", tok_display(&other)),
                span.start,
            )),
        }
    }

    fn peek_is_lparen_next(&self) -> bool {
        matches!(self.tokens.get(self.pos + 1), Some((Token::LParen, _)))
    }

    /// Parse the body of `i(...)` or `re(...)` AFTER the `(` has been consumed.
    /// `call_open_span` points at the `(`; we use its end position for
    /// `Unterminated function call` errors so the cursor underlines the `(`.
    fn parse_call(
        &mut self,
        field_name: &str,
        fn_name: &str,
        call_open_span: Range<usize>,
    ) -> Result<Ast, ChqlError> {
        let (tok, span) = match self.peek().cloned() {
            Some(p) => p,
            None => {
                return Err(ChqlError::at(
                    "Unterminated function call: expected string literal then ')'",
                    call_open_span.end + 1,
                ))
            }
        };
        let value = match tok {
            Token::String(s) => {
                if !s.terminated {
                    return Err(ChqlError::at(
                        "Unterminated string literal",
                        span.start,
                    ));
                }
                self.bump();
                Value::Str { v: s.value }
            }
            _ => {
                return Err(ChqlError::at(
                    "Unterminated function call: expected string literal then ')'",
                    span.start,
                ))
            }
        };
        match self.peek() {
            Some((Token::RParen, _)) => {
                self.bump();
            }
            _ => {
                return Err(ChqlError::at(
                    "Unterminated function call: expected string literal then ')'",
                    call_open_span.end + 1,
                ));
            }
        }
        let op = match fn_name {
            "i" => CmpOp::IlikeContains,
            "re" => CmpOp::Regex,
            _ => unreachable!(),
        };
        Ok(Ast::Cmp {
            field: field_name.to_string(),
            op,
            value,
        })
    }

    /// Parse `[lo, hi]` after the `[` has been consumed. `open_span` points
    /// at the `[` so its `.end` is right after the bracket.
    fn parse_range(
        &mut self,
        field_name: &str,
        open_span: Range<usize>,
    ) -> Result<Ast, ChqlError> {
        let lo = self.parse_range_endpoint(field_name, open_span.end)?;
        match self.peek().cloned() {
            Some((Token::Comma, _)) => {
                self.bump();
            }
            _ => {
                return Err(ChqlError::at(
                    "Unterminated range: expected number or string then ']'",
                    open_span.end + 1,
                ))
            }
        }
        // Right after the comma — bail out if anything other than a value is next.
        let (next_tok, next_span) = match self.peek().cloned() {
            Some(p) => p,
            None => {
                return Err(ChqlError::at(
                    "Unterminated range: expected number or string then ']'",
                    open_span.end + 4,
                ))
            }
        };
        if !matches!(next_tok, Token::Number(_) | Token::String(_)) {
            return Err(ChqlError::at(
                "Unterminated range: expected number or string then ']'",
                next_span.start,
            ));
        }
        let hi = self.parse_range_endpoint(field_name, next_span.start)?;
        match self.peek() {
            Some((Token::RBracket, _)) => {
                self.bump();
            }
            _ => {
                return Err(ChqlError::at(
                    "Unterminated range: expected number or string then ']'",
                    open_span.end,
                ));
            }
        }
        Ok(Ast::InRange {
            field: field_name.to_string(),
            lo,
            hi,
        })
    }

    fn parse_range_endpoint(
        &mut self,
        field_name: &str,
        fallback_pos: usize,
    ) -> Result<Value, ChqlError> {
        let (tok, span) = match self.peek().cloned() {
            Some(p) => p,
            None => {
                return Err(ChqlError::at(
                    "Unterminated range: expected number or string then ']'",
                    fallback_pos,
                ))
            }
        };
        match tok {
            Token::Number(n) => {
                self.bump();
                Ok(Value::Num { v: n })
            }
            Token::String(s) => {
                if !s.terminated {
                    return Err(ChqlError::at(
                        "Unterminated string literal",
                        span.start,
                    ));
                }
                self.bump();
                if is_date_field(field_name) {
                    Ok(Value::Date { v: s.value })
                } else {
                    Ok(Value::Str { v: s.value })
                }
            }
            _ => Err(ChqlError::at(
                "Unterminated range: expected number or string then ']'",
                span.start,
            )),
        }
    }
}

fn is_date_field(name: &str) -> bool {
    name == "created_at" || name == "completed_at"
}

/// Returns true if `t` can begin a new atom — used to detect implicit-AND
/// adjacency.
fn can_start_atom(t: &Token) -> bool {
    matches!(t, Token::Ident(_) | Token::LParen | Token::Not | Token::String(_))
}

fn tok_display(t: &Token) -> String {
    match t {
        Token::BangEq => "'!='".into(),
        Token::Gte => "'>='".into(),
        Token::Lte => "'<='".into(),
        Token::Bang => "'!'".into(),
        Token::Gt => "'>'".into(),
        Token::Lt => "'<'".into(),
        Token::Colon => "':'".into(),
        Token::Comma => "','".into(),
        Token::LBracket => "'['".into(),
        Token::RBracket => "']'".into(),
        Token::LParen => "'('".into(),
        Token::RParen => "')'".into(),
        Token::And => "'AND'".into(),
        Token::Or => "'OR'".into(),
        Token::Not => "'NOT'".into(),
        Token::Ident(s) => format!("'{s}'"),
        Token::String(StrLit { value, .. }) => format!("\"{value}\""),
        Token::Number(n) => format!("{n}"),
    }
}

#[cfg(test)]
mod parser_inline_tests {
    use super::*;

    fn p(s: &str) -> Ast {
        parse(s).expect("parse ok").expect("non-empty")
    }

    #[test]
    fn empty_input_none() {
        assert!(parse("").unwrap().is_none());
        assert!(parse("   ").unwrap().is_none());
    }

    #[test]
    fn bare_eq() {
        let a = p("state:failed");
        match a {
            Ast::Cmp { field, op, value } => {
                assert_eq!(field, "state");
                assert_eq!(op, CmpOp::Eq);
                assert_eq!(value, Value::Str { v: "failed".into() });
            }
            _ => panic!("bad ast"),
        }
    }

    #[test]
    fn neq_bang() {
        let a = p("state:!failed");
        match a {
            Ast::Cmp { op, .. } => assert_eq!(op, CmpOp::Neq),
            _ => panic!(),
        }
    }

    #[test]
    fn or_chain_left_assoc() {
        let a = p("state:failed OR state:cancelled OR state:expired");
        match a {
            Ast::Or { left, right: _ } => match *left {
                Ast::Or { .. } => {}
                _ => panic!("not left-assoc"),
            },
            _ => panic!("not or"),
        }
    }

    #[test]
    fn implicit_and_adjacency() {
        let a = p("state:failed branch:main");
        match a {
            Ast::And { .. } => {}
            _ => panic!("not and"),
        }
    }

    #[test]
    fn paren_grouping() {
        let a = p("( state:failed OR state:cancelled ) AND branch:main");
        match a {
            Ast::And { left, right: _ } => match *left {
                Ast::Or { .. } => {}
                _ => panic!("inner not or"),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn err_unknown_field_position() {
        let err = parse("unknownfield:value").unwrap_err();
        assert!(err.message.contains("Unknown field"));
        assert_eq!(err.position, Some(0));
    }

    #[test]
    fn err_trailing_and() {
        let err = parse("state:failed AND").unwrap_err();
        assert!(err.message.contains("Expected expression after 'AND'"));
    }

    #[test]
    fn err_leading_or_position_zero() {
        let err = parse("OR state:failed").unwrap_err();
        assert_eq!(err.position, Some(0));
    }

    #[test]
    fn err_extra_close_paren() {
        let err = parse("state:failed )").unwrap_err();
        assert!(err.message.contains("Unexpected token ')'"));
    }
}
