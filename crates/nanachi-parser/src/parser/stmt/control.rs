use nanachi_ast::stmt::{Stmt, StmtKind};
use nanachi_lexer::{Span, Token};
use winnow::prelude::*;
use crate::parser::expr::{block_parser, expr_parser};
use super::super::common::token;
use super::super::ParserInput;

/// `while condition { body }`
pub fn while_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let while_tok = token(Token::While).parse_next(input)?;
    let condition = super::super::expr::expr_no_struct(input)?;
    let body = super::super::expr::block_parser(input)?;

    Ok(Stmt {
        span: Span {
            start: while_tok.span.start,
            end: body.span.end,
        },
        kind: StmtKind::While { condition, body },
    })
}

/// `for pattern [: type] in iter { body }`
pub fn for_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let for_tok = token(Token::For).parse_next(input)?;
    let pattern = super::super::pattern::pattern_parser(input)?;

    let ty = if token(Token::Colon).parse_next(input).is_ok() {
        Some(super::super::types::type_expr_parser(input)?)
    } else {
        None
    };

    token(Token::In).parse_next(input)?;
    let iter = super::super::expr::expr_no_struct(input)?;
    let body = super::super::expr::block_parser(input)?;

    Ok(Stmt {
        span: Span {
            start: for_tok.span.start,
            end: body.span.end,
        },
        kind: StmtKind::For {
            pattern,
            ty,
            iter,
            body,
        },
    })
}

/// `loop { body }`
pub fn loop_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let loop_tok = token(Token::Loop).parse_next(input)?;
    let body = block_parser(input)?;

    Ok(Stmt {
        span: Span {
            start: loop_tok.span.start,
            end: body.span.end,
        },
        kind: StmtKind::Loop { body },
    })
}

/// `break [expr];`
pub fn break_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let break_tok = token(Token::Break).parse_next(input)?;
    let value = if !matches!(
        input.input.first().map(|t| &t.token),
        Some(Token::Semi | Token::RBrace)
    ) {
        Some(expr_parser(input)?)
    } else {
        None
    };
    let semi = token(Token::Semi).parse_next(input)?;

    Ok(Stmt {
        span: Span {
            start: break_tok.span.start,
            end: semi.span.end,
        },
        kind: StmtKind::Break(value),
    })
}

/// `continue;`
pub fn continue_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let continue_tok = token(Token::Continue).parse_next(input)?;
    let semi = token(Token::Semi).parse_next(input)?;

    Ok(Stmt {
        span: Span {
            start: continue_tok.span.start,
            end: semi.span.end,
        },
        kind: StmtKind::Continue,
    })
}
