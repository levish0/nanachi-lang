use nanachi_ast::expr::{Block, Expr, ExprKind, UnOp};
use nanachi_lexer::{Span, Token};
use winnow::combinator::peek;
use winnow::prelude::*;
use winnow::token::any;

use super::atom::atom_parser;
use super::infix::{infix_bp, make_infix};
use super::postfix::postfix_op;
use super::super::common::{backtrack, token};
use super::super::ParserInput;

pub fn expr_parser(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    expr_bp(input, 0)
}

/// Parse an expression without struct literals (for conditions in if/while/for).
pub fn expr_no_struct(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let prev = input.state.no_struct_literal;
    input.state.no_struct_literal = true;
    let result = expr_bp(input, 0);
    input.state.no_struct_literal = prev;
    result
}

pub fn block_parser(input: &mut ParserInput<'_>) -> winnow::Result<Block> {
    let open = token(Token::LBrace).parse_next(input)?;
    let mut stmts = Vec::new();
    let mut tail_expr = None;

    while peek(token(Token::RBrace)).parse_next(input).is_err() {
        let checkpoint = input.input;
        let state_backup = input.state.clone();

        match super::super::stmt::stmt_parser(input) {
            Ok(stmt) => stmts.push(stmt),
            Err(_) => {
                input.input = checkpoint;
                input.state = state_backup;
                tail_expr = Some(Box::new(expr_parser(input)?));
                break;
            }
        }
    }

    let close = token(Token::RBrace).parse_next(input)?;

    Ok(Block {
        span: Span {
            start: open.span.start,
            end: close.span.end,
        },
        stmts,
        tail_expr,
    })
}

pub fn expr_bp(input: &mut ParserInput<'_>, min_bp: u8) -> winnow::Result<Expr> {
    let mut lhs = prefix_or_atom(input)?;

    loop {
        let Some(next) = input.input.first() else {
            break;
        };

        if matches!(
            next.token,
            Token::Dot | Token::QuestionDot | Token::LBracket | Token::LParen
        ) {
            lhs = postfix_op(input, lhs)?;
            continue;
        }

        let Some((l_bp, r_bp)) = infix_bp(&next.token) else {
            break;
        };
        if l_bp < min_bp {
            break;
        }

        let op_tok = any.parse_next(input)?;
        let rhs = expr_bp(input, r_bp)?;
        lhs = make_infix(lhs, &op_tok, rhs);
    }

    Ok(lhs)
}

fn prefix_or_atom(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let next = input.input.first().ok_or(backtrack())?;

    match next.token {
        Token::Minus => {
            let op = any.parse_next(input)?;
            let operand = expr_bp(input, 25)?;
            Ok(Expr {
                span: Span {
                    start: op.span.start,
                    end: operand.span.end,
                },
                kind: ExprKind::UnaryOp {
                    op: UnOp::Neg,
                    operand: Box::new(operand),
                },
            })
        }
        Token::Bang => {
            let op = any.parse_next(input)?;
            let operand = expr_bp(input, 25)?;
            Ok(Expr {
                span: Span {
                    start: op.span.start,
                    end: operand.span.end,
                },
                kind: ExprKind::UnaryOp {
                    op: UnOp::Not,
                    operand: Box::new(operand),
                },
            })
        }
        Token::Tilde => {
            let op = any.parse_next(input)?;
            let operand = expr_bp(input, 25)?;
            Ok(Expr {
                span: Span {
                    start: op.span.start,
                    end: operand.span.end,
                },
                kind: ExprKind::UnaryOp {
                    op: UnOp::BitNot,
                    operand: Box::new(operand),
                },
            })
        }
        _ => atom_parser(input),
    }
}
