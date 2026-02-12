use nanachi_lexer::{SpannedToken, Token};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::any;

use super::ParserInput;

/// Match and consume a specific token kind.
pub fn token<'i>(expected: Token) -> impl Parser<ParserInput<'i>, SpannedToken, ContextError> {
    any.verify(move |t: &SpannedToken| t.token == expected)
}

/// Match and consume an identifier token.
pub fn ident(input: &mut ParserInput) -> winnow::Result<SpannedToken, ContextError> {
    any.verify(|t: &SpannedToken| t.token == Token::Ident)
        .parse_next(input)
}

/// Create a backtrack error.
pub fn backtrack() -> ContextError {
    ContextError::new()
}
