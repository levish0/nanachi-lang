use nanachi_ast::expr::{BinOp, CompoundOp, Expr, ExprKind};
use nanachi_lexer::{Span, SpannedToken, Token};

/// Returns (left_bp, right_bp) for infix operators.
pub fn infix_bp(tok: &Token) -> Option<(u8, u8)> {
    match tok {
        Token::Eq
        | Token::PlusEq
        | Token::MinusEq
        | Token::StarEq
        | Token::SlashEq
        | Token::PercentEq
        | Token::AmpEq
        | Token::PipeEq
        | Token::CaretEq
        | Token::ShlEq
        | Token::ShrEq => Some((2, 1)),
        Token::QuestionQuestion => Some((3, 4)),
        Token::PipePipe => Some((5, 6)),
        Token::AmpAmp => Some((7, 8)),
        Token::EqEq | Token::BangEq | Token::Lt | Token::Gt | Token::LtEq | Token::GtEq => {
            Some((9, 10))
        }
        Token::Pipe => Some((11, 12)),
        Token::Caret => Some((13, 14)),
        Token::Amp => Some((15, 16)),
        Token::Shl | Token::Shr => Some((17, 18)),
        Token::Plus | Token::Minus => Some((19, 20)),
        Token::Star | Token::Slash | Token::Percent => Some((21, 22)),
        _ => None,
    }
}

pub fn make_infix(left: Expr, op_tok: &SpannedToken, right: Expr) -> Expr {
    let span = Span {
        start: left.span.start,
        end: right.span.end,
    };

    let kind = match op_tok.token {
        Token::Eq => ExprKind::Assign {
            target: Box::new(left),
            value: Box::new(right),
        },
        Token::PlusEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Add,
            value: Box::new(right),
        },
        Token::MinusEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Sub,
            value: Box::new(right),
        },
        Token::StarEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Mul,
            value: Box::new(right),
        },
        Token::SlashEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Div,
            value: Box::new(right),
        },
        Token::PercentEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Rem,
            value: Box::new(right),
        },
        Token::AmpEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::BitAnd,
            value: Box::new(right),
        },
        Token::PipeEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::BitOr,
            value: Box::new(right),
        },
        Token::CaretEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::BitXor,
            value: Box::new(right),
        },
        Token::ShlEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Shl,
            value: Box::new(right),
        },
        Token::ShrEq => ExprKind::CompoundAssign {
            target: Box::new(left),
            op: CompoundOp::Shr,
            value: Box::new(right),
        },
        Token::QuestionQuestion => ExprKind::NullCoalesce {
            expr: Box::new(left),
            default: Box::new(right),
        },
        Token::PipePipe => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Or,
            right: Box::new(right),
        },
        Token::AmpAmp => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::And,
            right: Box::new(right),
        },
        Token::EqEq => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Eq,
            right: Box::new(right),
        },
        Token::BangEq => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Ne,
            right: Box::new(right),
        },
        Token::Lt => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Lt,
            right: Box::new(right),
        },
        Token::Gt => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Gt,
            right: Box::new(right),
        },
        Token::LtEq => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Le,
            right: Box::new(right),
        },
        Token::GtEq => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Ge,
            right: Box::new(right),
        },
        Token::Pipe => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::BitOr,
            right: Box::new(right),
        },
        Token::Caret => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::BitXor,
            right: Box::new(right),
        },
        Token::Amp => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::BitAnd,
            right: Box::new(right),
        },
        Token::Shl => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Shl,
            right: Box::new(right),
        },
        Token::Shr => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Shr,
            right: Box::new(right),
        },
        Token::Plus => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Add,
            right: Box::new(right),
        },
        Token::Minus => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Sub,
            right: Box::new(right),
        },
        Token::Star => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Mul,
            right: Box::new(right),
        },
        Token::Slash => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Div,
            right: Box::new(right),
        },
        Token::Percent => ExprKind::BinaryOp {
            left: Box::new(left),
            op: BinOp::Rem,
            right: Box::new(right),
        },
        _ => unreachable!(),
    };

    Expr { span, kind }
}
