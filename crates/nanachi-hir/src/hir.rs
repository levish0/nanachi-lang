use nanachi_lexer::Span;

use nanachi_ast::expr::{BinOp, CompoundOp, Literal, MacroDelimiter, UnOp};
use nanachi_ast::item::Visibility;
use nanachi_ast::types::PrimitiveType;

// ── Resolved Type ───────────────────────────────────────────

/// A resolved type. `T?` has been desugared to `Option<T>`.
#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    /// Primitive: i32, f64, bool, char, usize, ...
    Primitive(PrimitiveType),
    /// Named type with generics: `String`, `Vec<i32>`, `std::io::Error`.
    Named {
        path: Vec<String>,
        generics: Vec<HirType>,
    },
    /// `Option<T>` — desugared from `T?`.
    Option(Box<HirType>),
    /// Tuple: `(i32, f64)`.
    Tuple(Vec<HirType>),
    /// Fixed-size array: `[i32; 5]`.
    Array { element: Box<HirType>, size: usize },
    /// Slice: `[i32]`.
    Slice(Box<HirType>),
    /// Unit: `()`.
    Unit,
    /// No annotation — Rust compiler infers the type.
    Unresolved,
}

// ── Expressions ─────────────────────────────────────────────

/// An expression with a resolved (or unresolved) type.
#[derive(Debug, Clone, PartialEq)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
    pub span: Span,
}

/// Optional access for `?.` chains.
#[derive(Debug, Clone, PartialEq)]
pub enum HirOptionalAccess {
    Field(String),
    Method { name: String, args: Vec<HirExpr> },
}

/// Field initializer in struct literal.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFieldInit {
    pub name: String,
    pub value: Option<HirExpr>,
    pub span: Span,
}

/// Closure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct HirClosureParam {
    pub name: String,
    pub ty: HirType,
    pub span: Span,
}

/// A match arm.
#[derive(Debug, Clone, PartialEq)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<Box<HirExpr>>,
    pub body: HirExpr,
    pub span: Span,
}

/// A block: `{ stmts; [tail] }`.
#[derive(Debug, Clone, PartialEq)]
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub tail_expr: Option<Box<HirExpr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirExprKind {
    /// Literal value.
    Literal(Literal),
    /// Resolved path: variable, function, constant.
    Path(Vec<String>),
    /// Binary operation.
    BinaryOp {
        left: Box<HirExpr>,
        op: BinOp,
        right: Box<HirExpr>,
    },
    /// Unary operation.
    UnaryOp { op: UnOp, operand: Box<HirExpr> },
    /// Function call.
    FnCall {
        func: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    /// Macro call (passed through verbatim).
    MacroCall {
        path: Vec<String>,
        delimiter: MacroDelimiter,
        tokens: String,
    },
    /// Method call.
    MethodCall {
        receiver: Box<HirExpr>,
        method: String,
        args: Vec<HirExpr>,
    },
    /// Field access.
    FieldAccess {
        receiver: Box<HirExpr>,
        field: String,
    },
    /// Optional chaining: `expr?.field`, `expr?.method()`.
    OptionalChain {
        receiver: Box<HirExpr>,
        access: HirOptionalAccess,
    },
    /// Null coalescing: `expr ?? default`.
    NullCoalesce {
        expr: Box<HirExpr>,
        default: Box<HirExpr>,
    },
    /// Index: `receiver[index]`.
    Index {
        receiver: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    /// Block expression.
    Block(HirBlock),
    /// If expression.
    If {
        condition: Box<HirExpr>,
        then_block: HirBlock,
        else_expr: Option<Box<HirExpr>>,
    },
    /// Match expression.
    Match {
        expr: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
    /// `.await` expression.
    Await { expr: Box<HirExpr> },
    /// Assignment.
    Assign {
        target: Box<HirExpr>,
        value: Box<HirExpr>,
    },
    /// Compound assignment: `+=`, `-=`, etc.
    CompoundAssign {
        target: Box<HirExpr>,
        op: CompoundOp,
        value: Box<HirExpr>,
    },
    /// Struct literal: `User { name: "a", age: 1 }`.
    StructLiteral {
        path: Vec<String>,
        fields: Vec<HirFieldInit>,
    },
    /// Range: `start..end`, `start..=end`.
    Range {
        start: Option<Box<HirExpr>>,
        end: Option<Box<HirExpr>>,
        inclusive: bool,
    },
    /// Closure: `|params| body`.
    Closure {
        params: Vec<HirClosureParam>,
        return_ty: HirType,
        body: Box<HirExpr>,
    },
    /// Return: `return [expr]`.
    Return(Option<Box<HirExpr>>),
    /// Tuple: `(a, b, c)`.
    Tuple(Vec<HirExpr>),
}

// ── Patterns ────────────────────────────────────────────────

/// Field pattern in struct destructuring.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFieldPattern {
    pub name: String,
    pub pattern: Option<HirPattern>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirPattern {
    pub kind: HirPatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirPatternKind {
    Wildcard,
    Ident(String),
    Literal(Literal),
    Tuple(Vec<HirPattern>),
    Struct {
        path: Vec<String>,
        fields: Vec<HirFieldPattern>,
    },
    TupleStruct {
        path: Vec<String>,
        fields: Vec<HirPattern>,
    },
    Path(Vec<String>),
    Rest,
}

// ── Statements ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct HirStmt {
    pub kind: HirStmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirStmtKind {
    /// `let pattern [: type] [= value];`.
    Let {
        pattern: HirPattern,
        ty: HirType,
        value: Option<HirExpr>,
    },
    /// Expression statement.
    Expr(HirExpr),
    /// `while condition { body }`.
    While { condition: HirExpr, body: HirBlock },
    /// `for pattern [: type] in iter { body }`.
    For {
        pattern: HirPattern,
        iter_ty: HirType,
        iter: HirExpr,
        body: HirBlock,
    },
    /// `loop { body }`.
    Loop { body: HirBlock },
    /// `break [expr];`.
    Break(Option<HirExpr>),
    /// `continue;`.
    Continue,
    /// Nested item (e.g., `rust { }` block).
    Item(Box<HirItem>),
}

// ── Items ───────────────────────────────────────────────────

/// Generic parameter with resolved bound types.
#[derive(Debug, Clone, PartialEq)]
pub struct HirGenericParam {
    pub name: String,
    pub bounds: Vec<HirType>,
    pub span: Span,
}

/// Where predicate with resolved types.
#[derive(Debug, Clone, PartialEq)]
pub struct HirWherePredicate {
    pub ty: HirType,
    pub bounds: Vec<HirType>,
    pub span: Span,
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFnParam {
    pub kind: HirFnParamKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirFnParamKind {
    /// `self` — analyzer will resolve to &self/&mut self/self.
    SelfParam,
    /// `name: Type`.
    Typed { name: String, ty: HirType },
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirFunction {
    pub visibility: Visibility,
    pub is_async: bool,
    pub name: String,
    pub generics: Vec<HirGenericParam>,
    pub params: Vec<HirFnParam>,
    pub return_ty: HirType,
    pub where_clause: Vec<HirWherePredicate>,
    pub body: HirBlock,
    pub span: Span,
}

/// Struct field with resolved type.
#[derive(Debug, Clone, PartialEq)]
pub struct HirStructField {
    pub visibility: Visibility,
    pub name: String,
    pub ty: HirType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirStruct {
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<HirGenericParam>,
    pub fields: Vec<HirStructField>,
    pub span: Span,
}

/// Enum variant fields with resolved types.
#[derive(Debug, Clone, PartialEq)]
pub enum HirVariantFields {
    Unit,
    Tuple(Vec<HirType>),
    Struct(Vec<HirStructField>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirEnumVariant {
    pub name: String,
    pub fields: HirVariantFields,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirEnum {
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<HirGenericParam>,
    pub variants: Vec<HirEnumVariant>,
    pub span: Span,
}

/// Trait method signature with resolved types.
#[derive(Debug, Clone, PartialEq)]
pub struct HirTraitMethod {
    pub name: String,
    pub generics: Vec<HirGenericParam>,
    pub params: Vec<HirFnParam>,
    pub return_ty: HirType,
    pub default_body: Option<HirBlock>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirTrait {
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<HirGenericParam>,
    pub methods: Vec<HirTraitMethod>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirImpl {
    pub generics: Vec<HirGenericParam>,
    pub trait_name: Option<Vec<String>>,
    pub target: HirType,
    pub methods: Vec<HirFunction>,
    pub span: Span,
}

/// Use tree (kept as-is — no resolution of external crates).
#[derive(Debug, Clone, PartialEq)]
pub enum HirUseTree {
    Simple {
        path: Vec<String>,
        alias: Option<String>,
    },
    Glob {
        path: Vec<String>,
    },
    Nested {
        path: Vec<String>,
        items: Vec<HirUseTree>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirUse {
    pub visibility: Visibility,
    pub tree: HirUseTree,
    pub span: Span,
}

/// Raw Rust block (passed through verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct HirRustBlock {
    pub code: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirItem {
    pub kind: HirItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirItemKind {
    Function(HirFunction),
    Struct(HirStruct),
    Enum(HirEnum),
    Trait(HirTrait),
    Impl(HirImpl),
    Use(HirUse),
    RustBlock(HirRustBlock),
}

// ── Program ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct HirProgram {
    pub items: Vec<HirItem>,
}
