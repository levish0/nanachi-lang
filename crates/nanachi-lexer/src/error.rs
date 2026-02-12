use crate::span::Span;
use std::fmt;

/// Lexer error — unexpected character at a given span.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub span: Span,
    pub text: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unexpected character '{}' at {}..{}",
            self.text, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for LexError {}
