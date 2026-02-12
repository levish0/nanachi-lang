use nanachi_ast::item::{RustBlockItem, UseItem, UseTree, Visibility};
use nanachi_lexer::{Span, Token};
use winnow::combinator::{peek, separated};
use winnow::prelude::*;
use winnow::token::any;

use super::super::common::{backtrack, ident, token};
use super::ParserInput;

/// `use path::to::item;`
pub fn use_item(input: &mut ParserInput<'_>, vis: Visibility) -> winnow::Result<UseItem> {
    let use_tok = token(Token::Use).parse_next(input)?;
    let tree = use_tree(input)?;
    let semi = token(Token::Semi).parse_next(input)?;

    Ok(UseItem {
        visibility: vis,
        tree,
        span: Span {
            start: use_tok.span.start,
            end: semi.span.end,
        },
    })
}

fn use_tree(input: &mut ParserInput<'_>) -> winnow::Result<UseTree> {
    let first = ident(input)?;
    let mut segments = vec![first.text.clone()];
    let mut end = first.span.end;

    while token(Token::ColonColon).parse_next(input).is_ok() {
        if token(Token::Star).parse_next(input).is_ok() {
            let path = nanachi_ast::types::Path {
                segments,
                span: Span {
                    start: first.span.start,
                    end,
                },
            };
            return Ok(UseTree::Glob { path });
        }
        if peek(token(Token::LBrace)).parse_next(input).is_ok() {
            token(Token::LBrace).parse_next(input)?;
            let items: Vec<UseTree> =
                separated(1.., use_tree, token(Token::Comma)).parse_next(input)?;
            let _ = token(Token::Comma).parse_next(input);
            token(Token::RBrace).parse_next(input)?;
            let path = nanachi_ast::types::Path {
                segments,
                span: Span {
                    start: first.span.start,
                    end,
                },
            };
            return Ok(UseTree::Nested { path, items });
        }
        let seg = ident(input)?;
        end = seg.span.end;
        segments.push(seg.text.clone());
    }

    let path = nanachi_ast::types::Path {
        segments,
        span: Span {
            start: first.span.start,
            end,
        },
    };

    let alias = if token(Token::As).parse_next(input).is_ok() {
        let alias_tok = ident(input)?;
        Some(alias_tok.text.clone())
    } else {
        None
    };

    Ok(UseTree::Simple { path, alias })
}

/// `rust { raw_code }`
pub fn rust_block(input: &mut ParserInput<'_>) -> winnow::Result<RustBlockItem> {
    let rust_tok = token(Token::Rust).parse_next(input)?;
    token(Token::LBrace).parse_next(input)?;

    let mut depth = 1u32;
    let mut code = String::new();

    loop {
        if input.input.is_empty() {
            return Err(backtrack());
        }
        let t = any.parse_next(input)?;
        if t.token == Token::LBrace {
            depth += 1;
        } else if t.token == Token::RBrace {
            depth -= 1;
            if depth == 0 {
                return Ok(RustBlockItem {
                    code,
                    span: Span {
                        start: rust_tok.span.start,
                        end: t.span.end,
                    },
                });
            }
        }
        if !code.is_empty() {
            code.push(' ');
        }
        code.push_str(&t.text);
    }
}
