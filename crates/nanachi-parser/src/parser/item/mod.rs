use nanachi_ast::item::*;
use nanachi_lexer::Token;

use super::common::backtrack;
use super::ParserInput;

mod common;
mod data;
mod function;
mod trait_impl;
mod use_item;

/// Parse a top-level item.
pub fn item_parser(input: &mut ParserInput<'_>) -> winnow::Result<Item> {
    let vis = common::visibility(input);

    let next = input.input.first().ok_or(backtrack())?;
    match next.token {
        Token::Fn | Token::Async => {
            let func = function::function_item(input, vis)?;
            Ok(Item {
                span: func.span,
                kind: ItemKind::Function(func),
            })
        }
        Token::Struct => {
            let s = data::struct_item(input, vis)?;
            Ok(Item {
                span: s.span,
                kind: ItemKind::Struct(s),
            })
        }
        Token::Enum => {
            let e = data::enum_item(input, vis)?;
            Ok(Item {
                span: e.span,
                kind: ItemKind::Enum(e),
            })
        }
        Token::Trait => {
            let t = trait_impl::trait_item(input, vis)?;
            Ok(Item {
                span: t.span,
                kind: ItemKind::Trait(t),
            })
        }
        Token::Impl => {
            let i = trait_impl::impl_item(input)?;
            Ok(Item {
                span: i.span,
                kind: ItemKind::Impl(i),
            })
        }
        Token::Use => {
            let u = use_item::use_item(input, vis)?;
            Ok(Item {
                span: u.span,
                kind: ItemKind::Use(u),
            })
        }
        Token::Rust => {
            let r = use_item::rust_block(input)?;
            Ok(Item {
                span: r.span,
                kind: ItemKind::RustBlock(r),
            })
        }
        _ => Err(backtrack()),
    }
}

/// Parse a full program: sequence of items.
pub fn program_parser(input: &mut ParserInput<'_>) -> winnow::Result<Program> {
    let mut items = Vec::new();
    while !input.input.is_empty() {
        items.push(item_parser(input)?);
    }
    Ok(Program { items })
}
