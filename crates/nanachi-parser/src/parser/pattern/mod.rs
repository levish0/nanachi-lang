use nanachi_ast::expr::Literal;
use nanachi_ast::pattern::{Pattern, PatternKind};
use nanachi_lexer::{Span, Token};
use winnow::combinator::separated;
use winnow::prelude::*;
use winnow::token::any;

use super::common::{backtrack, token};
use super::ParserInput;

mod path;

/// Parse a pattern: `_`, `x`, `42`, `(a, b)`, `Some(x)`, `Point { x, y }`.
pub fn pattern_parser(input: &mut ParserInput<'_>) -> winnow::Result<Pattern> {
    let next = input.input.first().ok_or(backtrack())?;

    match next.token {
        Token::Ident if next.text == "_" => {
            let t = any.parse_next(input)?;
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Wildcard,
            })
        }
        Token::DotDot => {
            let t = any.parse_next(input)?;
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Rest,
            })
        }
        Token::IntLiteral => {
            let t = any.parse_next(input)?;
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Literal(Literal::Int(t.text.clone())),
            })
        }
        Token::FloatLiteral => {
            let t = any.parse_next(input)?;
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Literal(Literal::Float(t.text.clone())),
            })
        }
        Token::StringLiteral => {
            let t = any.parse_next(input)?;
            let content = t.text[1..t.text.len() - 1].to_string();
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Literal(Literal::String(content)),
            })
        }
        Token::CharLiteral => {
            let t = any.parse_next(input)?;
            let ch = t.text.chars().nth(1).unwrap_or('\0');
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Literal(Literal::Char(ch)),
            })
        }
        Token::True => {
            let t = any.parse_next(input)?;
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Literal(Literal::Bool(true)),
            })
        }
        Token::False => {
            let t = any.parse_next(input)?;
            Ok(Pattern {
                span: t.span,
                kind: PatternKind::Literal(Literal::Bool(false)),
            })
        }
        Token::LParen => {
            let open = any.parse_next(input)?;
            let fields: Vec<Pattern> =
                separated(0.., pattern_parser, token(Token::Comma)).parse_next(input)?;
            let _ = token(Token::Comma).parse_next(input);
            let close = token(Token::RParen).parse_next(input)?;
            Ok(Pattern {
                span: Span {
                    start: open.span.start,
                    end: close.span.end,
                },
                kind: PatternKind::Tuple(fields),
            })
        }
        Token::Ident | Token::SelfUpper => path::ident_or_path_pattern(input),
        _ => Err(backtrack()),
    }
}
