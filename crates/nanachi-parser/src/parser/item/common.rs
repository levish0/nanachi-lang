use nanachi_ast::item::{GenericParam, Visibility, WherePredicate};
use nanachi_ast::types::TypeExpr;
use nanachi_lexer::{Span, Token};
use winnow::combinator::separated;
use winnow::prelude::*;

use super::super::common::{ident, token};
use super::super::types;
use super::ParserInput;

/// Parse optional `pub` visibility.
pub fn visibility(input: &mut ParserInput<'_>) -> Visibility {
    if token(Token::Pub).parse_next(input).is_ok() {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

/// Parse `<T, U: Display>` or nothing.
pub fn generic_params(input: &mut ParserInput<'_>) -> winnow::Result<Vec<GenericParam>> {
    if token(Token::Lt).parse_next(input).is_err() {
        return Ok(vec![]);
    }

    let params: Vec<GenericParam> =
        separated(1.., generic_param, token(Token::Comma)).parse_next(input)?;
    let _ = token(Token::Comma).parse_next(input);
    types::gt_token(input)?;

    Ok(params)
}

fn generic_param(input: &mut ParserInput<'_>) -> winnow::Result<GenericParam> {
    let name_tok = ident(input)?;

    let bounds = if token(Token::Colon).parse_next(input).is_ok() {
        separated(1.., types::type_expr_parser, token(Token::Plus)).parse_next(input)?
    } else {
        vec![]
    };

    let end = bounds
        .last()
        .map(|b: &TypeExpr| b.span.end)
        .unwrap_or(name_tok.span.end);

    Ok(GenericParam {
        name: name_tok.text.clone(),
        bounds,
        span: Span {
            start: name_tok.span.start,
            end,
        },
    })
}

/// Parse `where T: Display, U: Debug` or nothing.
pub fn where_clause(input: &mut ParserInput<'_>) -> winnow::Result<Vec<WherePredicate>> {
    if token(Token::Where).parse_next(input).is_err() {
        return Ok(vec![]);
    }

    let predicates: Vec<WherePredicate> =
        separated(1.., where_predicate, token(Token::Comma)).parse_next(input)?;
    let _ = token(Token::Comma).parse_next(input);

    Ok(predicates)
}

fn where_predicate(input: &mut ParserInput<'_>) -> winnow::Result<WherePredicate> {
    let ty = types::type_expr_parser(input)?;
    token(Token::Colon).parse_next(input)?;
    let bounds: Vec<TypeExpr> =
        separated(1.., types::type_expr_parser, token(Token::Plus)).parse_next(input)?;

    let end = bounds.last().map(|b| b.span.end).unwrap_or(ty.span.end);

    Ok(WherePredicate {
        span: Span {
            start: ty.span.start,
            end,
        },
        ty,
        bounds,
    })
}

/// Extract path from a TypeExpr::Named for trait name in `impl Trait for Type`.
pub fn type_expr_to_path(ty: &TypeExpr) -> Option<nanachi_ast::types::Path> {
    match &ty.kind {
        nanachi_ast::types::TypeKind::Named { path, .. } => Some(path.clone()),
        _ => None,
    }
}
