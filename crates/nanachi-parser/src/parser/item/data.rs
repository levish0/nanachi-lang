use nanachi_ast::item::{
    EnumItem, EnumVariant, StructField, StructItem, VariantFields, Visibility,
};
use nanachi_ast::types::TypeExpr;
use nanachi_lexer::{Span, Token};
use winnow::combinator::{peek, separated};
use winnow::prelude::*;

use super::common::{generic_params, visibility};
use super::super::common::{ident, token};
use super::super::{types, ParserInput};

/// `struct Name[<T>] { fields }`
pub fn struct_item(
    input: &mut ParserInput<'_>,
    vis: Visibility,
) -> winnow::Result<StructItem> {
    let struct_tok = token(Token::Struct).parse_next(input)?;
    let name_tok = ident(input)?;
    let generics = generic_params(input)?;

    token(Token::LBrace).parse_next(input)?;
    let fields: Vec<StructField> =
        separated(0.., struct_field, token(Token::Comma)).parse_next(input)?;
    let _ = token(Token::Comma).parse_next(input);
    let close = token(Token::RBrace).parse_next(input)?;

    Ok(StructItem {
        visibility: vis,
        name: name_tok.text.clone(),
        generics,
        fields,
        span: Span {
            start: struct_tok.span.start,
            end: close.span.end,
        },
    })
}

pub fn struct_field(input: &mut ParserInput<'_>) -> winnow::Result<StructField> {
    let vis = visibility(input);
    let name_tok = ident(input)?;
    token(Token::Colon).parse_next(input)?;
    let ty = types::type_expr_parser(input)?;
    let end = ty.span.end;

    Ok(StructField {
        visibility: vis,
        name: name_tok.text.clone(),
        ty,
        span: Span {
            start: name_tok.span.start,
            end,
        },
    })
}

/// `enum Name[<T>] { variants }`
pub fn enum_item(input: &mut ParserInput<'_>, vis: Visibility) -> winnow::Result<EnumItem> {
    let enum_tok = token(Token::Enum).parse_next(input)?;
    let name_tok = ident(input)?;
    let generics = generic_params(input)?;

    token(Token::LBrace).parse_next(input)?;
    let variants: Vec<EnumVariant> =
        separated(0.., enum_variant, token(Token::Comma)).parse_next(input)?;
    let _ = token(Token::Comma).parse_next(input);
    let close = token(Token::RBrace).parse_next(input)?;

    Ok(EnumItem {
        visibility: vis,
        name: name_tok.text.clone(),
        generics,
        variants,
        span: Span {
            start: enum_tok.span.start,
            end: close.span.end,
        },
    })
}

fn enum_variant(input: &mut ParserInput<'_>) -> winnow::Result<EnumVariant> {
    let name_tok = ident(input)?;

    let fields = if peek(token(Token::LParen)).parse_next(input).is_ok() {
        token(Token::LParen).parse_next(input)?;
        let types: Vec<TypeExpr> =
            separated(0.., types::type_expr_parser, token(Token::Comma)).parse_next(input)?;
        let _ = token(Token::Comma).parse_next(input);
        token(Token::RParen).parse_next(input)?;
        VariantFields::Tuple(types)
    } else if peek(token(Token::LBrace)).parse_next(input).is_ok() {
        token(Token::LBrace).parse_next(input)?;
        let fields: Vec<StructField> =
            separated(0.., struct_field, token(Token::Comma)).parse_next(input)?;
        let _ = token(Token::Comma).parse_next(input);
        token(Token::RBrace).parse_next(input)?;
        VariantFields::Struct(fields)
    } else {
        VariantFields::Unit
    };

    let end = input
        .input
        .first()
        .map(|t| t.span.start)
        .unwrap_or(name_tok.span.end);

    Ok(EnumVariant {
        name: name_tok.text.clone(),
        fields,
        span: Span {
            start: name_tok.span.start,
            end,
        },
    })
}
