use nanachi_lexer::Span;

use crate::expr::{Block, Expr};
use crate::item::Item;
use crate::pattern::Pattern;
use crate::types::TypeExpr;

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `let pattern [: type] [= value];`.
    Let {
        pattern: Pattern,
        ty: Option<TypeExpr>,
        value: Option<Expr>,
    },
    /// Expression statement (expression followed by `;`).
    Expr(Expr),
    /// `while condition { body }`.
    While { condition: Expr, body: Block },
    /// `for pattern [: type] in iter { body }`.
    For {
        pattern: Pattern,
        ty: Option<TypeExpr>,
        iter: Expr,
        body: Block,
    },
    /// `loop { body }`.
    Loop { body: Block },
    /// `break [expr];`.
    Break(Option<Expr>),
    /// `continue;`.
    Continue,
    /// Item declaration in statement position.
    Item(Box<Item>),
}
