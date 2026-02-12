use nanachi_ast::types::{Path, TypeExpr, TypeKind};
use nanachi_lexer::{Span, Token};
use winnow::combinator::separated;
use winnow::error::ContextError;
use winnow::prelude::*;

use super::common::{ident, token};
use super::ParserInput;

mod primitive;

/// Parse a type expression: `i32`, `Vec<String>`, `T?`, etc.
pub fn type_expr_parser(input: &mut ParserInput<'_>) -> winnow::Result<TypeExpr, ContextError> {
    let base = base_type_parser(input)?;

    if let Ok(q) = token(Token::Question).parse_next(input) {
        return Ok(TypeExpr {
            span: Span {
                start: base.span.start,
                end: q.span.end,
            },
            kind: TypeKind::Option(Box::new(base)),
        });
    }

    Ok(base)
}

/// Parse a base type (without `?` suffix).
fn base_type_parser(input: &mut ParserInput<'_>) -> winnow::Result<TypeExpr, ContextError> {
    if let Ok(open) = token(Token::LParen).parse_next(input) {
        if let Ok(close) = token(Token::RParen).parse_next(input) {
            return Ok(TypeExpr {
                span: Span {
                    start: open.span.start,
                    end: close.span.end,
                },
                kind: TypeKind::Unit,
            });
        }
        let first = type_expr_parser(input)?;
        token(Token::Comma).parse_next(input)?;
        let mut types = vec![first];
        let rest: Vec<TypeExpr> =
            separated(0.., type_expr_parser, token(Token::Comma)).parse_next(input)?;
        types.extend(rest);
        let close = token(Token::RParen).parse_next(input)?;
        return Ok(TypeExpr {
            span: Span {
                start: open.span.start,
                end: close.span.end,
            },
            kind: TypeKind::Tuple(types),
        });
    }

    let first = ident(input)?;

    if let Some(prim) = primitive::parse_primitive(&first.text) {
        return Ok(TypeExpr {
            span: first.span,
            kind: TypeKind::Primitive(prim),
        });
    }

    let mut segments = vec![first.text.clone()];
    let mut end = first.span.end;

    while token(Token::ColonColon).parse_next(input).is_ok() {
        let seg = ident(input)?;
        end = seg.span.end;
        segments.push(seg.text.clone());
    }

    let path = Path {
        segments,
        span: Span {
            start: first.span.start,
            end,
        },
    };

    let generics = if token(Token::Lt).parse_next(input).is_ok() {
        let args: Vec<TypeExpr> =
            separated(0.., type_expr_parser, token(Token::Comma)).parse_next(input)?;
        gt_token(input)?;
        args
    } else {
        vec![]
    };

    let span = Span {
        start: first.span.start,
        end: generics.last().map(|g| g.span.end + 1).unwrap_or(end),
    };

    Ok(TypeExpr {
        span,
        kind: TypeKind::Named { path, generics },
    })
}

/// Consume `>`, handling `>>` (Shr) splitting for generics.
pub fn gt_token(input: &mut ParserInput<'_>) -> winnow::Result<Span, ContextError> {
    if input.state.pending_gt {
        input.state.pending_gt = false;
        let span = input
            .input
            .first()
            .map(|t| t.span)
            .unwrap_or(Span { start: 0, end: 0 });
        return Ok(span);
    }

    if let Ok(t) = token(Token::Gt).parse_next(input) {
        return Ok(t.span);
    }

    let t = token(Token::Shr).parse_next(input)?;
    input.state.pending_gt = true;
    Ok(Span {
        start: t.span.start,
        end: t.span.start + 1,
    })
}
