use nanachi_ast::expr::{Expr, ExprKind, OptionalAccess};
use nanachi_lexer::{Span, Token};
use winnow::combinator::{peek, separated};
use winnow::prelude::*;
use winnow::token::any;

use super::super::ParserInput;
use super::super::common::{backtrack, ident, token};
use super::core::expr_parser;

pub fn postfix_op(input: &mut ParserInput<'_>, receiver: Expr) -> winnow::Result<Expr> {
    let next = input.input.first().ok_or(backtrack())?;

    match next.token {
        Token::Dot => {
            any.parse_next(input)?;
            if let Ok(t) = token(Token::Await).parse_next(input) {
                return Ok(Expr {
                    span: Span {
                        start: receiver.span.start,
                        end: t.span.end,
                    },
                    kind: ExprKind::Await {
                        expr: Box::new(receiver),
                    },
                });
            }
            let field = ident(input)?;
            if peek(token(Token::LParen)).parse_next(input).is_ok() {
                token(Token::LParen).parse_next(input)?;
                let args: Vec<Expr> =
                    separated(0.., expr_parser, token(Token::Comma)).parse_next(input)?;
                let close = token(Token::RParen).parse_next(input)?;
                return Ok(Expr {
                    span: Span {
                        start: receiver.span.start,
                        end: close.span.end,
                    },
                    kind: ExprKind::MethodCall {
                        receiver: Box::new(receiver),
                        method: field.text.clone(),
                        args,
                    },
                });
            }
            Ok(Expr {
                span: Span {
                    start: receiver.span.start,
                    end: field.span.end,
                },
                kind: ExprKind::FieldAccess {
                    receiver: Box::new(receiver),
                    field: field.text.clone(),
                },
            })
        }
        Token::QuestionDot => {
            any.parse_next(input)?;
            let field = ident(input)?;
            if peek(token(Token::LParen)).parse_next(input).is_ok() {
                token(Token::LParen).parse_next(input)?;
                let args: Vec<Expr> =
                    separated(0.., expr_parser, token(Token::Comma)).parse_next(input)?;
                let close = token(Token::RParen).parse_next(input)?;
                return Ok(Expr {
                    span: Span {
                        start: receiver.span.start,
                        end: close.span.end,
                    },
                    kind: ExprKind::OptionalChain {
                        receiver: Box::new(receiver),
                        access: OptionalAccess::Method {
                            name: field.text.clone(),
                            args,
                        },
                    },
                });
            }
            Ok(Expr {
                span: Span {
                    start: receiver.span.start,
                    end: field.span.end,
                },
                kind: ExprKind::OptionalChain {
                    receiver: Box::new(receiver),
                    access: OptionalAccess::Field(field.text.clone()),
                },
            })
        }
        Token::LBracket => {
            any.parse_next(input)?;
            let index = expr_parser(input)?;
            let close = token(Token::RBracket).parse_next(input)?;
            Ok(Expr {
                span: Span {
                    start: receiver.span.start,
                    end: close.span.end,
                },
                kind: ExprKind::Index {
                    receiver: Box::new(receiver),
                    index: Box::new(index),
                },
            })
        }
        Token::LParen => {
            any.parse_next(input)?;
            let args: Vec<Expr> =
                separated(0.., expr_parser, token(Token::Comma)).parse_next(input)?;
            let close = token(Token::RParen).parse_next(input)?;
            Ok(Expr {
                span: Span {
                    start: receiver.span.start,
                    end: close.span.end,
                },
                kind: ExprKind::FnCall {
                    func: Box::new(receiver),
                    args,
                },
            })
        }
        _ => Err(backtrack()),
    }
}
