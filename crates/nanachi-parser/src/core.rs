use crate::context::ParseContext;
use crate::error::ParseError;
use nanachi_ast::item::Program;
use nanachi_lexer::{Span, SpannedToken};
use winnow::Stateful;

/// Parse a token stream into a Program AST.
pub fn parse(tokens: &[SpannedToken]) -> Result<Program, ParseError> {
    let mut input = Stateful {
        input: tokens,
        state: ParseContext::new(),
    };

    crate::parser::item::program_parser(&mut input).map_err(|e| ParseError {
        span: tokens
            .last()
            .map(|t| t.span)
            .unwrap_or(Span { start: 0, end: 0 }),
        message: format!("{e}"),
    })
}
