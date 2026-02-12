use nanachi_lexer::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    Bool,
    Char,
    Usize,
    Isize,
}

/// A qualified path like `std::io::Error` or a simple name like `User`.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub segments: Vec<String>,
    pub span: Span,
}

/// A type as written in source code.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// Primitive types: i32, f64, bool, char, etc.
    Primitive(PrimitiveType),
    /// Named type with optional generic arguments: `User`, `Vec<i32>`, `std::io::Error`.
    Named { path: Path, generics: Vec<TypeExpr> },
    /// `T?` — desugared to `Option<T>` in HIR.
    Option(Box<TypeExpr>),
    /// Tuple: `(i32, String)`.
    Tuple(Vec<TypeExpr>),
    /// Fixed-size array: `[i32; 5]`.
    Array { element: Box<TypeExpr>, size: usize },
    /// Slice: `[i32]`.
    Slice(Box<TypeExpr>),
    /// Unit type: `()`.
    Unit,
    /// Type to be inferred: `_`.
    Inferred,
}
