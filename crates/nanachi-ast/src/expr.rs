use nanachi_lexer::Span;

use crate::pattern::Pattern;
use crate::stmt::Stmt;
use crate::types::{Path, TypeExpr};

// ── Literals ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(String),
    Float(String),
    String(String),
    Char(char),
    Bool(bool),
}

// ── Operators ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

// ── Helper types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroDelimiter {
    Paren,
    Bracket,
    Brace,
}

/// `?.field` or `?.method(args)`.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionalAccess {
    Field(String),
    Method { name: String, args: Vec<Expr> },
}

/// Field initializer in a struct literal: `name: expr` or shorthand `name`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

/// Closure parameter: `x` or `x: i32`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureParam {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

/// A match arm: `pattern [if guard] => body`.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Expr,
    pub span: Span,
}

/// A block: `{ stmts; [tail_expr] }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail_expr: Option<Box<Expr>>,
    pub span: Span,
}

// ── Expression ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// `42`, `3.14`, `"hello"`, `'c'`, `true`.
    Literal(Literal),
    /// `x`, `Vec::new`, `std::io::stdin`.
    Path(Path),
    /// `left op right`: `a + b`, `x == y`.
    BinaryOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// `op operand`: `-x`, `!flag`.
    UnaryOp { op: UnOp, operand: Box<Expr> },
    /// `func(args)`: `foo(1, 2)`, `Vec::new()`.
    FnCall { func: Box<Expr>, args: Vec<Expr> },
    /// `name!(tokens)`, `vec![1, 2]`.
    MacroCall {
        path: Path,
        delimiter: MacroDelimiter,
        tokens: String,
    },
    /// `receiver.method(args)`: `obj.greet()`.
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    /// `receiver.field`: `user.name`.
    FieldAccess { receiver: Box<Expr>, field: String },
    /// `receiver?.access`: `user?.name`, `user?.greet()`.
    OptionalChain {
        receiver: Box<Expr>,
        access: OptionalAccess,
    },
    /// `expr ?? default`.
    NullCoalesce { expr: Box<Expr>, default: Box<Expr> },
    /// `receiver[index]`.
    Index {
        receiver: Box<Expr>,
        index: Box<Expr>,
    },
    /// `{ stmts; [expr] }`.
    Block(Block),
    /// `if cond { ... } [else { ... }]`.
    If {
        condition: Box<Expr>,
        then_block: Block,
        else_expr: Option<Box<Expr>>,
    },
    /// `match expr { arms }`.
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// `expr.await`.
    Await { expr: Box<Expr> },
    /// `target = value`.
    Assign { target: Box<Expr>, value: Box<Expr> },
    /// `target op= value`: `x += 1`.
    CompoundAssign {
        target: Box<Expr>,
        op: CompoundOp,
        value: Box<Expr>,
    },
    /// `Path { field: value, ... }`: `User { name: "a", age: 1 }`.
    StructLiteral { path: Path, fields: Vec<FieldInit> },
    /// `start..end` or `start..=end`.
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
    },
    /// `|params| body`.
    Closure {
        params: Vec<ClosureParam>,
        return_ty: Option<TypeExpr>,
        body: Box<Expr>,
    },
    /// `return [expr]`.
    Return(Option<Box<Expr>>),
    /// `(a, b, c)`. Empty tuple `()` is `Tuple(vec![])`.
    Tuple(Vec<Expr>),
}
