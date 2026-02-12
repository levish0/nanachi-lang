use nanachi_ast::stmt::Stmt;
use nanachi_lexer::Token;

use super::ParserInput;
use super::common::backtrack;

mod control;
mod expr_item;
mod local;

/// Parse a statement.
pub fn stmt_parser(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let next = input.input.first().ok_or(backtrack())?;

    match next.token {
        Token::Let => local::let_stmt(input),
        Token::While => control::while_stmt(input),
        Token::For => control::for_stmt(input),
        Token::Loop => control::loop_stmt(input),
        Token::Break => control::break_stmt(input),
        Token::Continue => control::continue_stmt(input),
        Token::Fn
        | Token::Struct
        | Token::Enum
        | Token::Trait
        | Token::Impl
        | Token::Use
        | Token::Rust
        | Token::Pub
        | Token::Async => expr_item::item_stmt(input),
        _ => expr_item::expr_stmt(input),
    }
}
