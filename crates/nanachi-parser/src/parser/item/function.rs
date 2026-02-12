use nanachi_ast::item::{FnParam, FnParamKind, FunctionItem, Visibility};
use nanachi_lexer::{Span, Token};
use winnow::combinator::separated;
use winnow::prelude::*;

use super::common::{generic_params, where_clause};
use super::super::common::{ident, token};
use super::super::{expr, types, ParserInput};

/// `[async] fn name[<T>](params) [-> RetTy] [where ...] { body }`
pub fn function_item(
    input: &mut ParserInput<'_>,
    vis: Visibility,
) -> winnow::Result<FunctionItem> {
    let start = input.input.first().map(|t| t.span.start).unwrap_or(0);

    let is_async = token(Token::Async).parse_next(input).is_ok();
    token(Token::Fn).parse_next(input)?;
    let name_tok = ident(input)?;

    let generics = generic_params(input)?;

    token(Token::LParen).parse_next(input)?;
    let params: Vec<FnParam> = separated(0.., fn_param, token(Token::Comma)).parse_next(input)?;
    let _ = token(Token::Comma).parse_next(input);
    token(Token::RParen).parse_next(input)?;

    let return_ty = if token(Token::Arrow).parse_next(input).is_ok() {
        Some(types::type_expr_parser(input)?)
    } else {
        None
    };

    let where_clause = where_clause(input)?;
    let body = expr::block_parser(input)?;
    let end = body.span.end;

    Ok(FunctionItem {
        visibility: vis,
        is_async,
        name: name_tok.text.clone(),
        generics,
        params,
        return_ty,
        where_clause,
        body,
        span: Span { start, end },
    })
}

/// Parse a function parameter: `self` or `name: Type`.
pub fn fn_param(input: &mut ParserInput<'_>) -> winnow::Result<FnParam> {
    if let Ok(t) = token(Token::SelfLower).parse_next(input) {
        return Ok(FnParam {
            span: t.span,
            kind: FnParamKind::SelfParam,
        });
    }

    let name_tok = ident(input)?;
    token(Token::Colon).parse_next(input)?;
    let ty = types::type_expr_parser(input)?;

    Ok(FnParam {
        span: Span {
            start: name_tok.span.start,
            end: ty.span.end,
        },
        kind: FnParamKind::Typed {
            name: name_tok.text.clone(),
            ty,
        },
    })
}
