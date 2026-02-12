use crate::context::ParseContext;
use nanachi_lexer::SpannedToken;
use winnow::Stateful;

pub mod common;
pub mod expr;
pub mod item;
pub mod pattern;
pub mod stmt;
pub mod types;

pub type ParserInput<'i> = Stateful<&'i [SpannedToken], ParseContext>;
