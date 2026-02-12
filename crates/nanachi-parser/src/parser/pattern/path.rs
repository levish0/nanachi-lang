use nanachi_ast::pattern::{FieldPattern, Pattern, PatternKind};
use nanachi_ast::types::Path;
use nanachi_lexer::{Span, Token};
use winnow::combinator::{peek, separated};
use winnow::prelude::*;
use winnow::token::any;

use super::super::ParserInput;
use super::super::common::{ident, token};
use super::pattern_parser;

/// Parse identifier-based patterns:
/// - Simple binding: `x`
/// - Path: `None`, `Color::Red`
/// - Tuple struct: `Some(x)`, `Color::Rgb(r, g, b)`
/// - Struct: `Point { x, y }`
pub fn ident_or_path_pattern(input: &mut ParserInput<'_>) -> winnow::Result<Pattern> {
    let first = any.parse_next(input)?;
    let mut segments = vec![first.text.clone()];
    let mut end = first.span.end;

    while token(Token::ColonColon).parse_next(input).is_ok() {
        let seg = ident(input)?;
        end = seg.span.end;
        segments.push(seg.text.clone());
    }

    let path = Path {
        segments: segments.clone(),
        span: Span {
            start: first.span.start,
            end,
        },
    };

    if peek(token(Token::LParen)).parse_next(input).is_ok() {
        token(Token::LParen).parse_next(input)?;
        let fields: Vec<Pattern> =
            separated(0.., pattern_parser, token(Token::Comma)).parse_next(input)?;
        let _ = token(Token::Comma).parse_next(input);
        let close = token(Token::RParen).parse_next(input)?;
        return Ok(Pattern {
            span: Span {
                start: first.span.start,
                end: close.span.end,
            },
            kind: PatternKind::TupleStruct { path, fields },
        });
    }

    if peek(token(Token::LBrace)).parse_next(input).is_ok() {
        token(Token::LBrace).parse_next(input)?;
        let fields: Vec<FieldPattern> =
            separated(0.., field_pattern, token(Token::Comma)).parse_next(input)?;
        let _ = token(Token::Comma).parse_next(input);
        let close = token(Token::RBrace).parse_next(input)?;
        return Ok(Pattern {
            span: Span {
                start: first.span.start,
                end: close.span.end,
            },
            kind: PatternKind::Struct { path, fields },
        });
    }

    if segments.len() == 1 {
        Ok(Pattern {
            span: first.span,
            kind: PatternKind::Ident(first.text.clone()),
        })
    } else {
        Ok(Pattern {
            span: path.span,
            kind: PatternKind::Path(path),
        })
    }
}

/// Parse a field pattern: `name: pattern` or shorthand `name`.
fn field_pattern(input: &mut ParserInput<'_>) -> winnow::Result<FieldPattern> {
    if peek(token(Token::DotDot)).parse_next(input).is_ok() {
        let t = token(Token::DotDot).parse_next(input)?;
        return Ok(FieldPattern {
            name: "..".to_string(),
            pattern: None,
            span: t.span,
        });
    }

    let name_tok = ident(input)?;

    if token(Token::Colon).parse_next(input).is_ok() {
        let pat = pattern_parser(input)?;
        let end = pat.span.end;
        return Ok(FieldPattern {
            name: name_tok.text.clone(),
            pattern: Some(pat),
            span: Span {
                start: name_tok.span.start,
                end,
            },
        });
    }

    Ok(FieldPattern {
        name: name_tok.text.clone(),
        pattern: None,
        span: name_tok.span,
    })
}
