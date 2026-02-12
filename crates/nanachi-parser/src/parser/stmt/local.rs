use nanachi_ast::stmt::{Stmt, StmtKind};
use nanachi_lexer::{Span, Token};
use winnow::prelude::*;

use super::super::ParserInput;
use super::super::common::token;

/// `let pattern [: type] [= value];`
pub fn let_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let let_tok = token(Token::Let).parse_next(input)?;
    let pattern = super::super::pattern::pattern_parser(input)?;

    let ty = if token(Token::Colon).parse_next(input).is_ok() {
        Some(super::super::types::type_expr_parser(input)?)
    } else {
        None
    };

    let value = if token(Token::Eq).parse_next(input).is_ok() {
        Some(super::super::expr::expr_parser(input)?)
    } else {
        None
    };

    let semi = token(Token::Semi).parse_next(input)?;

    Ok(Stmt {
        span: Span {
            start: let_tok.span.start,
            end: semi.span.end,
        },
        kind: StmtKind::Let { pattern, ty, value },
    })
}
