use nanachi_ast::expr::*;
use nanachi_ast::types::Path;
use nanachi_lexer::{Span, Token};
use winnow::combinator::{peek, separated};
use winnow::prelude::*;
use winnow::token::any;

use super::core::{block_parser, expr_no_struct, expr_parser};
use super::super::common::{backtrack, ident, token};
use super::super::ParserInput;

pub fn atom_parser(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let next = input.input.first().ok_or(backtrack())?;

    match &next.token {
        Token::IntLiteral => {
            let t = any.parse_next(input)?;
            Ok(Expr {
                span: t.span,
                kind: ExprKind::Literal(Literal::Int(t.text.clone())),
            })
        }
        Token::FloatLiteral => {
            let t = any.parse_next(input)?;
            Ok(Expr {
                span: t.span,
                kind: ExprKind::Literal(Literal::Float(t.text.clone())),
            })
        }
        Token::StringLiteral => {
            let t = any.parse_next(input)?;
            let content = t.text[1..t.text.len() - 1].to_string();
            Ok(Expr {
                span: t.span,
                kind: ExprKind::Literal(Literal::String(content)),
            })
        }
        Token::CharLiteral => {
            let t = any.parse_next(input)?;
            let ch = t.text.chars().nth(1).unwrap_or('\0');
            Ok(Expr {
                span: t.span,
                kind: ExprKind::Literal(Literal::Char(ch)),
            })
        }
        Token::True => {
            let t = any.parse_next(input)?;
            Ok(Expr {
                span: t.span,
                kind: ExprKind::Literal(Literal::Bool(true)),
            })
        }
        Token::False => {
            let t = any.parse_next(input)?;
            Ok(Expr {
                span: t.span,
                kind: ExprKind::Literal(Literal::Bool(false)),
            })
        }
        Token::Return => {
            let t = any.parse_next(input)?;
            let value = if !matches!(
                input.input.first().map(|t| &t.token),
                Some(Token::Semi | Token::RBrace)
            ) {
                Some(Box::new(expr_parser(input)?))
            } else {
                None
            };
            let end = value.as_ref().map(|v| v.span.end).unwrap_or(t.span.end);
            Ok(Expr {
                span: Span {
                    start: t.span.start,
                    end,
                },
                kind: ExprKind::Return(value),
            })
        }
        Token::If => if_parser(input),
        Token::Match => match_parser(input),
        Token::LBrace => {
            let block = block_parser(input)?;
            Ok(Expr {
                span: block.span,
                kind: ExprKind::Block(block),
            })
        }
        Token::LParen => paren_or_tuple(input),
        Token::Pipe => closure_parser(input),
        Token::Ident | Token::SelfLower | Token::SelfUpper => path_or_macro(input),
        _ => Err(backtrack()),
    }
}

fn if_parser(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let if_tok = token(Token::If).parse_next(input)?;
    let condition = Box::new(expr_no_struct(input)?);
    let then_block = block_parser(input)?;

    let else_expr = if token(Token::Else).parse_next(input).is_ok() {
        if input.input.first().is_some_and(|t| t.token == Token::If) {
            Some(Box::new(if_parser(input)?))
        } else {
            let block = block_parser(input)?;
            Some(Box::new(Expr {
                span: block.span,
                kind: ExprKind::Block(block),
            }))
        }
    } else {
        None
    };

    let end = else_expr
        .as_ref()
        .map(|e| e.span.end)
        .unwrap_or(then_block.span.end);

    Ok(Expr {
        span: Span {
            start: if_tok.span.start,
            end,
        },
        kind: ExprKind::If {
            condition,
            then_block,
            else_expr,
        },
    })
}

fn match_parser(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let match_tok = token(Token::Match).parse_next(input)?;
    let scrutinee = Box::new(expr_no_struct(input)?);
    token(Token::LBrace).parse_next(input)?;

    let mut arms = Vec::new();
    while peek(token(Token::RBrace)).parse_next(input).is_err() {
        let pattern = super::super::pattern::pattern_parser(input)?;
        let guard = if token(Token::If).parse_next(input).is_ok() {
            Some(Box::new(expr_parser(input)?))
        } else {
            None
        };
        token(Token::FatArrow).parse_next(input)?;
        let body = expr_parser(input)?;
        let end = body.span.end;

        arms.push(MatchArm {
            span: Span {
                start: pattern.span.start,
                end,
            },
            pattern,
            guard,
            body,
        });
        let _ = token(Token::Comma).parse_next(input);
    }

    let close = token(Token::RBrace).parse_next(input)?;
    Ok(Expr {
        span: Span {
            start: match_tok.span.start,
            end: close.span.end,
        },
        kind: ExprKind::Match {
            expr: scrutinee,
            arms,
        },
    })
}

fn paren_or_tuple(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let open = token(Token::LParen).parse_next(input)?;

    if let Ok(close) = token(Token::RParen).parse_next(input) {
        return Ok(Expr {
            span: Span {
                start: open.span.start,
                end: close.span.end,
            },
            kind: ExprKind::Tuple(vec![]),
        });
    }

    let first = expr_parser(input)?;

    if token(Token::Comma).parse_next(input).is_ok() {
        let mut elements = vec![first];
        if peek(token(Token::RParen)).parse_next(input).is_err() {
            let rest: Vec<Expr> = separated(1.., expr_parser, token(Token::Comma)).parse_next(input)?;
            elements.extend(rest);
            let _ = token(Token::Comma).parse_next(input);
        }
        let close = token(Token::RParen).parse_next(input)?;
        return Ok(Expr {
            span: Span {
                start: open.span.start,
                end: close.span.end,
            },
            kind: ExprKind::Tuple(elements),
        });
    }

    let close = token(Token::RParen).parse_next(input)?;
    Ok(Expr {
        span: Span {
            start: open.span.start,
            end: close.span.end,
        },
        kind: first.kind,
    })
}

fn closure_parser(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let open = token(Token::Pipe).parse_next(input)?;
    let mut params = Vec::new();

    if peek(token(Token::Pipe)).parse_next(input).is_err() {
        loop {
            let name_tok = ident(input)?;
            let ty = if token(Token::Colon).parse_next(input).is_ok() {
                Some(super::super::types::type_expr_parser(input)?)
            } else {
                None
            };
            params.push(ClosureParam {
                span: name_tok.span,
                name: name_tok.text.clone(),
                ty,
            });
            if token(Token::Comma).parse_next(input).is_err() {
                break;
            }
        }
    }

    token(Token::Pipe).parse_next(input)?;
    let return_ty = if token(Token::Arrow).parse_next(input).is_ok() {
        Some(super::super::types::type_expr_parser(input)?)
    } else {
        None
    };
    let body = Box::new(expr_parser(input)?);

    Ok(Expr {
        span: Span {
            start: open.span.start,
            end: body.span.end,
        },
        kind: ExprKind::Closure {
            params,
            return_ty,
            body,
        },
    })
}

fn path_or_macro(input: &mut ParserInput<'_>) -> winnow::Result<Expr> {
    let first = any.parse_next(input)?;
    let mut segments = vec![first.text.clone()];
    let mut end = first.span.end;

    while token(Token::ColonColon).parse_next(input).is_ok() {
        let seg = ident(input)?;
        end = seg.span.end;
        segments.push(seg.text.clone());
    }

    let path = Path {
        segments,
        span: Span {
            start: first.span.start,
            end,
        },
    };

    if token(Token::Bang).parse_next(input).is_ok() {
        return macro_call_parser(input, path);
    }

    if !input.state.no_struct_literal && peek(token(Token::LBrace)).parse_next(input).is_ok() {
        return struct_literal(input, path);
    }

    Ok(Expr {
        span: path.span,
        kind: ExprKind::Path(path),
    })
}

fn struct_literal(input: &mut ParserInput<'_>, path: Path) -> winnow::Result<Expr> {
    token(Token::LBrace).parse_next(input)?;
    let mut fields = Vec::new();

    while peek(token(Token::RBrace)).parse_next(input).is_err() {
        let name_tok = ident(input)?;

        let value = if token(Token::Colon).parse_next(input).is_ok() {
            Some(expr_parser(input)?)
        } else {
            None
        };

        let end = value
            .as_ref()
            .map(|v| v.span.end)
            .unwrap_or(name_tok.span.end);

        fields.push(FieldInit {
            name: name_tok.text.clone(),
            value,
            span: Span {
                start: name_tok.span.start,
                end,
            },
        });

        if token(Token::Comma).parse_next(input).is_err() {
            break;
        }
    }

    let close = token(Token::RBrace).parse_next(input)?;
    Ok(Expr {
        span: Span {
            start: path.span.start,
            end: close.span.end,
        },
        kind: ExprKind::StructLiteral { path, fields },
    })
}

fn macro_call_parser(input: &mut ParserInput<'_>, path: Path) -> winnow::Result<Expr> {
    let (delimiter, open_tok, close_tok) =
        if peek(token(Token::LParen)).parse_next(input).is_ok() {
            (MacroDelimiter::Paren, Token::LParen, Token::RParen)
        } else if peek(token(Token::LBracket)).parse_next(input).is_ok() {
            (MacroDelimiter::Bracket, Token::LBracket, Token::RBracket)
        } else {
            (MacroDelimiter::Brace, Token::LBrace, Token::RBrace)
        };

    any.parse_next(input)?;

    let mut depth = 1u32;
    let mut raw = String::new();

    loop {
        if input.input.is_empty() {
            return Err(backtrack());
        }
        let t = any.parse_next(input)?;
        if t.token == open_tok {
            depth += 1;
        } else if t.token == close_tok {
            depth -= 1;
            if depth == 0 {
                return Ok(Expr {
                    span: Span {
                        start: path.span.start,
                        end: t.span.end,
                    },
                    kind: ExprKind::MacroCall {
                        path,
                        delimiter,
                        tokens: raw,
                    },
                });
            }
        }
        if !raw.is_empty() {
            raw.push(' ');
        }
        raw.push_str(&t.text);
    }
}
