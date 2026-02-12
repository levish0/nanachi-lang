use nanachi_lexer::Span;

use crate::expr::Block;
use crate::types::{Path, TypeExpr};

// ── Shared types ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
}

/// Generic type parameter: `T` or `T: Display + Debug`.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<TypeExpr>,
    pub span: Span,
}

/// Where predicate: `T: Display + Debug`.
#[derive(Debug, Clone, PartialEq)]
pub struct WherePredicate {
    pub ty: TypeExpr,
    pub bounds: Vec<TypeExpr>,
    pub span: Span,
}

// ── Function ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FnParam {
    pub kind: FnParamKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FnParamKind {
    /// `self` (compiler infers &self / &mut self / self).
    SelfParam,
    /// `name: Type`.
    Typed { name: String, ty: TypeExpr },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionItem {
    pub visibility: Visibility,
    pub is_async: bool,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_ty: Option<TypeExpr>,
    pub where_clause: Vec<WherePredicate>,
    pub body: Block,
    pub span: Span,
}

// ── Struct ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub visibility: Visibility,
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructItem {
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<StructField>,
    pub span: Span,
}

// ── Enum ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum VariantFields {
    /// `Variant` (no data).
    Unit,
    /// `Variant(i32, String)`.
    Tuple(Vec<TypeExpr>),
    /// `Variant { x: i32, y: f64 }`.
    Struct(Vec<StructField>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub fields: VariantFields,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumItem {
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

// ── Trait ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<FnParam>,
    pub return_ty: Option<TypeExpr>,
    pub default_body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitItem {
    pub visibility: Visibility,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

// ── Impl ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ImplItem {
    pub generics: Vec<GenericParam>,
    pub trait_name: Option<Path>,
    pub target: TypeExpr,
    pub methods: Vec<FunctionItem>,
    pub span: Span,
}

// ── Use ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum UseTree {
    /// `use std::io;` or `use std::io as io_mod;`.
    Simple { path: Path, alias: Option<String> },
    /// `use std::io::*;`.
    Glob { path: Path },
    /// `use std::collections::{HashMap, HashSet};`.
    Nested { path: Path, items: Vec<UseTree> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    pub visibility: Visibility,
    pub tree: UseTree,
    pub span: Span,
}

// ── Rust block ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct RustBlockItem {
    pub code: String,
    pub span: Span,
}

// ── Item ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Function(FunctionItem),
    Struct(StructItem),
    Enum(EnumItem),
    Trait(TraitItem),
    Impl(ImplItem),
    Use(UseItem),
    RustBlock(RustBlockItem),
}

// ── Program ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}
