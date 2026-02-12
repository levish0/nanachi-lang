use nanachi_ast::expr::{Expr, ExprKind};
use nanachi_ast::stmt::{Stmt, StmtKind};
use nanachi_lexer::{Span, Token};
use winnow::prelude::*;

use super::super::ParserInput;
use super::super::common::token;

/// Expression statement: `expr;`
///
/// Note: block-like expressions (if, match, block) don't need semicolons.
pub fn expr_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let expr = super::super::expr::expr_parser(input)?;
    let start = expr.span.start;

    let end = if is_block_like(&expr) {
        if token(Token::Semi).parse_next(input).is_ok() {}
        expr.span.end
    } else {
        let semi = token(Token::Semi).parse_next(input)?;
        semi.span.end
    };

    Ok(Stmt {
        span: Span { start, end },
        kind: StmtKind::Expr(expr),
    })
}

/// Item in statement position.
pub fn item_stmt(input: &mut ParserInput<'_>) -> winnow::Result<Stmt> {
    let item = super::super::item::item_parser(input)?;
    Ok(Stmt {
        span: item.span,
        kind: StmtKind::Item(Box::new(item)),
    })
}

fn is_block_like(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Block(_) | ExprKind::If { .. } | ExprKind::Match { .. }
    )
}
