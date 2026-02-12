use nanachi_ast::item::{ImplItem, TraitItem, TraitMethod, Visibility};
use nanachi_lexer::{Span, Token};
use winnow::combinator::peek;
use winnow::prelude::*;

use super::super::common::{ident, token};
use super::super::{ParserInput, expr, types};
use super::common::{generic_params, type_expr_to_path};
use super::function::{fn_param, function_item};

/// `trait Name[<T>] { methods }`
pub fn trait_item(input: &mut ParserInput<'_>, vis: Visibility) -> winnow::Result<TraitItem> {
    let trait_tok = token(Token::Trait).parse_next(input)?;
    let name_tok = ident(input)?;
    let generics = generic_params(input)?;

    token(Token::LBrace).parse_next(input)?;
    let mut methods = Vec::new();
    while peek(token(Token::RBrace)).parse_next(input).is_err() {
        methods.push(trait_method(input)?);
    }
    let close = token(Token::RBrace).parse_next(input)?;

    Ok(TraitItem {
        visibility: vis,
        name: name_tok.text.clone(),
        generics,
        methods,
        span: Span {
            start: trait_tok.span.start,
            end: close.span.end,
        },
    })
}

fn trait_method(input: &mut ParserInput<'_>) -> winnow::Result<TraitMethod> {
    let fn_tok = token(Token::Fn).parse_next(input)?;
    let name_tok = ident(input)?;
    let generics = generic_params(input)?;

    token(Token::LParen).parse_next(input)?;
    let params =
        winnow::combinator::separated(0.., fn_param, token(Token::Comma)).parse_next(input)?;
    let _ = token(Token::Comma).parse_next(input);
    token(Token::RParen).parse_next(input)?;

    let return_ty = if token(Token::Arrow).parse_next(input).is_ok() {
        Some(types::type_expr_parser(input)?)
    } else {
        None
    };

    let (default_body, end) = if peek(token(Token::LBrace)).parse_next(input).is_ok() {
        let body = expr::block_parser(input)?;
        let end = body.span.end;
        (Some(body), end)
    } else {
        let semi = token(Token::Semi).parse_next(input)?;
        (None, semi.span.end)
    };

    Ok(TraitMethod {
        name: name_tok.text.clone(),
        generics,
        params,
        return_ty,
        default_body,
        span: Span {
            start: fn_tok.span.start,
            end,
        },
    })
}

/// `impl [<T>] [Trait for] Type { methods }`
pub fn impl_item(input: &mut ParserInput<'_>) -> winnow::Result<ImplItem> {
    let impl_tok = token(Token::Impl).parse_next(input)?;
    let generics = generic_params(input)?;

    let first_ty = types::type_expr_parser(input)?;
    let (trait_name, target) = if token(Token::For).parse_next(input).is_ok() {
        let trait_path = type_expr_to_path(&first_ty);
        let target = types::type_expr_parser(input)?;
        (trait_path, target)
    } else {
        (None, first_ty)
    };

    token(Token::LBrace).parse_next(input)?;
    let mut methods = Vec::new();
    while peek(token(Token::RBrace)).parse_next(input).is_err() {
        let vis = super::common::visibility(input);
        methods.push(function_item(input, vis)?);
    }
    let close = token(Token::RBrace).parse_next(input)?;

    Ok(ImplItem {
        generics,
        trait_name,
        target,
        methods,
        span: Span {
            start: impl_tok.span.start,
            end: close.span.end,
        },
    })
}
