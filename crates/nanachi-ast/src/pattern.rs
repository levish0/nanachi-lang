use nanachi_lexer::Span;

use crate::expr::Literal;
use crate::types::Path;

/// Field pattern in struct destructuring: `name: pattern` or shorthand `name`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPattern {
    pub name: String,
    pub pattern: Option<Pattern>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// `_`.
    Wildcard,
    /// Variable binding: `x`, `name`.
    Ident(String),
    /// Literal pattern: `42`, `"hello"`, `true`.
    Literal(Literal),
    /// Tuple destructuring: `(a, b)`.
    Tuple(Vec<Pattern>),
    /// Struct destructuring: `Shape::Circle { radius }`.
    Struct {
        path: Path,
        fields: Vec<FieldPattern>,
    },
    /// Tuple-struct destructuring: `Some(x)`.
    TupleStruct { path: Path, fields: Vec<Pattern> },
    /// Constant or enum variant path: `None`, `Color::Red`.
    Path(Path),
    /// Rest pattern: `..`.
    Rest,
}
