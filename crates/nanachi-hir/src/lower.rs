use nanachi_lexer::Span;

use nanachi_ast::expr::{
    Block, ClosureParam, Expr, ExprKind, FieldInit, Literal, MatchArm, OptionalAccess,
};
use nanachi_ast::item::{
    EnumItem, EnumVariant, FnParam, FnParamKind, FunctionItem, GenericParam, ImplItem, Item,
    ItemKind, Program, RustBlockItem, StructField, StructItem, TraitItem, TraitMethod, UseItem,
    UseTree, VariantFields, WherePredicate,
};
use nanachi_ast::pattern::{FieldPattern, Pattern, PatternKind};
use nanachi_ast::stmt::{Stmt, StmtKind};
use nanachi_ast::types::{Path, PrimitiveType, TypeExpr, TypeKind};

use crate::hir::*;
use crate::scope::{Scope, Symbol, TraitMethodSig};

// ── Error ───────────────────────────────────────────────────

#[derive(Debug)]
pub struct HirError {
    pub span: Span,
    pub message: String,
}

impl std::fmt::Display for HirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "HIR error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for HirError {}

// ── Lowering context ────────────────────────────────────────

struct LowerCtx {
    scope: Scope,
}

impl LowerCtx {
    fn new() -> Self {
        Self {
            scope: Scope::new(),
        }
    }
}

// ── Public entry point ──────────────────────────────────────

/// Lower an AST `Program` into an `HirProgram`.
pub fn lower(program: &Program) -> Result<HirProgram, HirError> {
    let mut ctx = LowerCtx::new();

    // Pass 1: collect all top-level signatures into scope.
    for item in &program.items {
        collect_item_signature(&mut ctx, item);
    }

    // Pass 2: lower all items (including function bodies).
    let mut items = Vec::new();
    for item in &program.items {
        items.push(lower_item(&mut ctx, item)?);
    }

    Ok(HirProgram { items })
}

// ── Pass 1: Signature Collection ────────────────────────────

fn collect_item_signature(ctx: &mut LowerCtx, item: &Item) {
    match &item.kind {
        ItemKind::Function(f) => collect_fn_signature(ctx, f),
        ItemKind::Struct(s) => collect_struct_signature(ctx, s),
        ItemKind::Enum(e) => collect_enum_signature(ctx, e),
        ItemKind::Trait(t) => collect_trait_signature(ctx, t),
        ItemKind::Impl(i) => collect_impl_signatures(ctx, i),
        ItemKind::Use(_) | ItemKind::RustBlock(_) => {}
    }
}

fn collect_fn_signature(ctx: &mut LowerCtx, f: &FunctionItem) {
    let params: Vec<(String, HirType)> = f
        .params
        .iter()
        .filter_map(|p| match &p.kind {
            FnParamKind::SelfParam => None,
            FnParamKind::Typed { name, ty } => Some((name.clone(), lower_type(ty))),
        })
        .collect();
    let return_ty = f
        .return_ty
        .as_ref()
        .map(|t| lower_type(t))
        .unwrap_or(HirType::Unit);
    ctx.scope.define(
        f.name.clone(),
        Symbol::Function { params, return_ty },
    );
}

fn collect_struct_signature(ctx: &mut LowerCtx, s: &StructItem) {
    let fields: Vec<(String, HirType)> = s
        .fields
        .iter()
        .map(|f| (f.name.clone(), lower_type(&f.ty)))
        .collect();
    ctx.scope
        .define(s.name.clone(), Symbol::Struct { fields });
}

fn collect_enum_signature(ctx: &mut LowerCtx, e: &EnumItem) {
    let variants: Vec<(String, HirVariantFields)> = e
        .variants
        .iter()
        .map(|v| (v.name.clone(), lower_variant_fields(&v.fields)))
        .collect();
    ctx.scope
        .define(e.name.clone(), Symbol::Enum { variants });
}

fn collect_trait_signature(ctx: &mut LowerCtx, t: &TraitItem) {
    let methods: Vec<TraitMethodSig> = t
        .methods
        .iter()
        .map(|m| {
            let params: Vec<(String, HirType)> = m
                .params
                .iter()
                .filter_map(|p| match &p.kind {
                    FnParamKind::SelfParam => None,
                    FnParamKind::Typed { name, ty } => Some((name.clone(), lower_type(ty))),
                })
                .collect();
            let return_ty = m
                .return_ty
                .as_ref()
                .map(|t| lower_type(t))
                .unwrap_or(HirType::Unit);
            TraitMethodSig {
                name: m.name.clone(),
                params,
                return_ty,
            }
        })
        .collect();
    ctx.scope
        .define(t.name.clone(), Symbol::Trait { methods });
}

fn collect_impl_signatures(ctx: &mut LowerCtx, i: &ImplItem) {
    // Register each method as a function in scope.
    for method in &i.methods {
        collect_fn_signature(ctx, method);
    }
}

// ── Pass 2: Item Lowering ───────────────────────────────────

fn lower_item(ctx: &mut LowerCtx, item: &Item) -> Result<HirItem, HirError> {
    let kind = match &item.kind {
        ItemKind::Function(f) => HirItemKind::Function(lower_function(ctx, f)?),
        ItemKind::Struct(s) => HirItemKind::Struct(lower_struct(s)),
        ItemKind::Enum(e) => HirItemKind::Enum(lower_enum(e)),
        ItemKind::Trait(t) => HirItemKind::Trait(lower_trait(ctx, t)?),
        ItemKind::Impl(i) => HirItemKind::Impl(lower_impl(ctx, i)?),
        ItemKind::Use(u) => HirItemKind::Use(lower_use(u)),
        ItemKind::RustBlock(rb) => HirItemKind::RustBlock(lower_rust_block(rb)),
    };
    Ok(HirItem {
        kind,
        span: item.span,
    })
}

// ── Function ────────────────────────────────────────────────

fn lower_function(ctx: &mut LowerCtx, f: &FunctionItem) -> Result<HirFunction, HirError> {
    ctx.scope.push();

    // Register generic type params in scope (as types).
    // Not tracked as symbols — they just exist as names.

    let generics = lower_generics(&f.generics);
    let params = lower_fn_params(ctx, &f.params);
    let return_ty = f
        .return_ty
        .as_ref()
        .map(|t| lower_type(t))
        .unwrap_or(HirType::Unit);
    let where_clause = lower_where_clause(&f.where_clause);

    let body = lower_block(ctx, &f.body)?;

    ctx.scope.pop();

    Ok(HirFunction {
        visibility: f.visibility,
        is_async: f.is_async,
        name: f.name.clone(),
        generics,
        params,
        return_ty,
        where_clause,
        body,
        span: f.span,
    })
}

fn lower_fn_params(ctx: &mut LowerCtx, params: &[FnParam]) -> Vec<HirFnParam> {
    params
        .iter()
        .map(|p| {
            let kind = match &p.kind {
                FnParamKind::SelfParam => HirFnParamKind::SelfParam,
                FnParamKind::Typed { name, ty } => {
                    let hir_ty = lower_type(ty);
                    ctx.scope.define(
                        name.clone(),
                        Symbol::Variable {
                            ty: hir_ty.clone(),
                        },
                    );
                    HirFnParamKind::Typed {
                        name: name.clone(),
                        ty: hir_ty,
                    }
                }
            };
            HirFnParam {
                kind,
                span: p.span,
            }
        })
        .collect()
}

// ── Struct ──────────────────────────────────────────────────

fn lower_struct(s: &StructItem) -> HirStruct {
    HirStruct {
        visibility: s.visibility,
        name: s.name.clone(),
        generics: lower_generics(&s.generics),
        fields: lower_struct_fields(&s.fields),
        span: s.span,
    }
}

fn lower_struct_fields(fields: &[StructField]) -> Vec<HirStructField> {
    fields
        .iter()
        .map(|f| HirStructField {
            visibility: f.visibility,
            name: f.name.clone(),
            ty: lower_type(&f.ty),
            span: f.span,
        })
        .collect()
}

// ── Enum ────────────────────────────────────────────────────

fn lower_enum(e: &EnumItem) -> HirEnum {
    HirEnum {
        visibility: e.visibility,
        name: e.name.clone(),
        generics: lower_generics(&e.generics),
        variants: e
            .variants
            .iter()
            .map(|v| lower_enum_variant(v))
            .collect(),
        span: e.span,
    }
}

fn lower_enum_variant(v: &EnumVariant) -> HirEnumVariant {
    HirEnumVariant {
        name: v.name.clone(),
        fields: lower_variant_fields(&v.fields),
        span: v.span,
    }
}

fn lower_variant_fields(fields: &VariantFields) -> HirVariantFields {
    match fields {
        VariantFields::Unit => HirVariantFields::Unit,
        VariantFields::Tuple(types) => {
            HirVariantFields::Tuple(types.iter().map(|t| lower_type(t)).collect())
        }
        VariantFields::Struct(fields) => HirVariantFields::Struct(lower_struct_fields(fields)),
    }
}

// ── Trait ────────────────────────────────────────────────────

fn lower_trait(ctx: &mut LowerCtx, t: &TraitItem) -> Result<HirTrait, HirError> {
    let mut methods = Vec::new();
    for m in &t.methods {
        methods.push(lower_trait_method(ctx, m)?);
    }
    Ok(HirTrait {
        visibility: t.visibility,
        name: t.name.clone(),
        generics: lower_generics(&t.generics),
        methods,
        span: t.span,
    })
}

fn lower_trait_method(ctx: &mut LowerCtx, m: &TraitMethod) -> Result<HirTraitMethod, HirError> {
    let generics = lower_generics(&m.generics);
    let params = lower_fn_params_no_scope(&m.params);
    let return_ty = m
        .return_ty
        .as_ref()
        .map(|t| lower_type(t))
        .unwrap_or(HirType::Unit);

    let default_body = if let Some(body) = &m.default_body {
        ctx.scope.push();
        // Register params in scope for default body.
        for p in &m.params {
            if let FnParamKind::Typed { name, ty } = &p.kind {
                ctx.scope.define(
                    name.clone(),
                    Symbol::Variable {
                        ty: lower_type(ty),
                    },
                );
            }
        }
        let block = lower_block(ctx, body)?;
        ctx.scope.pop();
        Some(block)
    } else {
        None
    };

    Ok(HirTraitMethod {
        name: m.name.clone(),
        generics,
        params,
        return_ty,
        default_body,
        span: m.span,
    })
}

/// Lower fn params without registering them in scope (for trait method signatures).
fn lower_fn_params_no_scope(params: &[FnParam]) -> Vec<HirFnParam> {
    params
        .iter()
        .map(|p| {
            let kind = match &p.kind {
                FnParamKind::SelfParam => HirFnParamKind::SelfParam,
                FnParamKind::Typed { name, ty } => HirFnParamKind::Typed {
                    name: name.clone(),
                    ty: lower_type(ty),
                },
            };
            HirFnParam {
                kind,
                span: p.span,
            }
        })
        .collect()
}

// ── Impl ────────────────────────────────────────────────────

fn lower_impl(ctx: &mut LowerCtx, i: &ImplItem) -> Result<HirImpl, HirError> {
    let mut methods = Vec::new();
    for method in &i.methods {
        methods.push(lower_function(ctx, method)?);
    }
    Ok(HirImpl {
        generics: lower_generics(&i.generics),
        trait_name: i.trait_name.as_ref().map(|p| p.segments.clone()),
        target: lower_type(&i.target),
        methods,
        span: i.span,
    })
}

// ── Use ─────────────────────────────────────────────────────

fn lower_use(u: &UseItem) -> HirUse {
    HirUse {
        visibility: u.visibility,
        tree: lower_use_tree(&u.tree),
        span: u.span,
    }
}

fn lower_use_tree(tree: &UseTree) -> HirUseTree {
    match tree {
        UseTree::Simple { path, alias } => HirUseTree::Simple {
            path: path.segments.clone(),
            alias: alias.clone(),
        },
        UseTree::Glob { path } => HirUseTree::Glob {
            path: path.segments.clone(),
        },
        UseTree::Nested { path, items } => HirUseTree::Nested {
            path: path.segments.clone(),
            items: items.iter().map(|t| lower_use_tree(t)).collect(),
        },
    }
}

// ── Rust block ──────────────────────────────────────────────

fn lower_rust_block(rb: &RustBlockItem) -> HirRustBlock {
    HirRustBlock {
        code: rb.code.clone(),
        span: rb.span,
    }
}

// ── Generics & Where ────────────────────────────────────────

fn lower_generics(generics: &[GenericParam]) -> Vec<HirGenericParam> {
    generics
        .iter()
        .map(|g| HirGenericParam {
            name: g.name.clone(),
            bounds: g.bounds.iter().map(|b| lower_type(b)).collect(),
            span: g.span,
        })
        .collect()
}

fn lower_where_clause(preds: &[WherePredicate]) -> Vec<HirWherePredicate> {
    preds
        .iter()
        .map(|p| HirWherePredicate {
            ty: lower_type(&p.ty),
            bounds: p.bounds.iter().map(|b| lower_type(b)).collect(),
            span: p.span,
        })
        .collect()
}

// ── Type Lowering ───────────────────────────────────────────

/// Lower an AST `TypeExpr` to `HirType`. This is where `T?` → `Option<T>` desugaring happens.
fn lower_type(ty: &TypeExpr) -> HirType {
    match &ty.kind {
        TypeKind::Primitive(p) => HirType::Primitive(*p),
        TypeKind::Named { path, generics } => HirType::Named {
            path: path.segments.clone(),
            generics: generics.iter().map(|g| lower_type(g)).collect(),
        },
        TypeKind::Option(inner) => HirType::Option(Box::new(lower_type(inner))),
        TypeKind::Tuple(types) => HirType::Tuple(types.iter().map(|t| lower_type(t)).collect()),
        TypeKind::Array { element, size } => HirType::Array {
            element: Box::new(lower_type(element)),
            size: *size,
        },
        TypeKind::Slice(inner) => HirType::Slice(Box::new(lower_type(inner))),
        TypeKind::Unit => HirType::Unit,
        TypeKind::Inferred => HirType::Unresolved,
    }
}

// ── Block & Statement Lowering ──────────────────────────────

fn lower_block(ctx: &mut LowerCtx, block: &Block) -> Result<HirBlock, HirError> {
    ctx.scope.push();

    let mut stmts = Vec::new();
    for stmt in &block.stmts {
        stmts.push(lower_stmt(ctx, stmt)?);
    }

    let tail_expr = if let Some(tail) = &block.tail_expr {
        Some(Box::new(lower_expr(ctx, tail)?))
    } else {
        None
    };

    ctx.scope.pop();

    Ok(HirBlock {
        stmts,
        tail_expr,
        span: block.span,
    })
}

fn lower_stmt(ctx: &mut LowerCtx, stmt: &Stmt) -> Result<HirStmt, HirError> {
    let kind = match &stmt.kind {
        StmtKind::Let { pattern, ty, value } => {
            let hir_ty = ty.as_ref().map(|t| lower_type(t)).unwrap_or(HirType::Unresolved);
            let hir_value = if let Some(v) = value {
                Some(lower_expr(ctx, v)?)
            } else {
                None
            };
            let hir_pattern = lower_pattern(pattern);

            // Register bindings from pattern into scope.
            register_pattern_bindings(ctx, &hir_pattern, &hir_ty);

            HirStmtKind::Let {
                pattern: hir_pattern,
                ty: hir_ty,
                value: hir_value,
            }
        }
        StmtKind::Expr(expr) => HirStmtKind::Expr(lower_expr(ctx, expr)?),
        StmtKind::While { condition, body } => HirStmtKind::While {
            condition: lower_expr(ctx, condition)?,
            body: lower_block(ctx, body)?,
        },
        StmtKind::For {
            pattern,
            ty,
            iter,
            body,
        } => {
            let iter_ty = ty
                .as_ref()
                .map(|t| lower_type(t))
                .unwrap_or(HirType::Unresolved);
            let hir_iter = lower_expr(ctx, iter)?;
            let hir_pattern = lower_pattern(pattern);

            // For loop body gets its own scope (from lower_block), but the
            // loop variable needs to be visible inside. We push a scope,
            // register the binding, lower the body (which pushes its own), then pop.
            ctx.scope.push();
            register_pattern_bindings(ctx, &hir_pattern, &iter_ty);
            let hir_body = lower_block_inner(ctx, body)?;
            ctx.scope.pop();

            HirStmtKind::For {
                pattern: hir_pattern,
                iter_ty,
                iter: hir_iter,
                body: hir_body,
            }
        }
        StmtKind::Loop { body } => HirStmtKind::Loop {
            body: lower_block(ctx, body)?,
        },
        StmtKind::Break(expr) => {
            let hir_expr = if let Some(e) = expr {
                Some(lower_expr(ctx, e)?)
            } else {
                None
            };
            HirStmtKind::Break(hir_expr)
        }
        StmtKind::Continue => HirStmtKind::Continue,
        StmtKind::Item(item) => HirStmtKind::Item(Box::new(lower_item(ctx, item)?)),
    };

    Ok(HirStmt {
        kind,
        span: stmt.span,
    })
}

/// Lower a block without pushing/popping its own scope (caller manages scope).
fn lower_block_inner(ctx: &mut LowerCtx, block: &Block) -> Result<HirBlock, HirError> {
    let mut stmts = Vec::new();
    for stmt in &block.stmts {
        stmts.push(lower_stmt(ctx, stmt)?);
    }

    let tail_expr = if let Some(tail) = &block.tail_expr {
        Some(Box::new(lower_expr(ctx, tail)?))
    } else {
        None
    };

    Ok(HirBlock {
        stmts,
        tail_expr,
        span: block.span,
    })
}

/// Register variable bindings from a pattern into the current scope.
fn register_pattern_bindings(ctx: &mut LowerCtx, pattern: &HirPattern, ty: &HirType) {
    match &pattern.kind {
        HirPatternKind::Ident(name) => {
            ctx.scope
                .define(name.clone(), Symbol::Variable { ty: ty.clone() });
        }
        HirPatternKind::Tuple(pats) => {
            // If the type is a tuple, propagate element types.
            if let HirType::Tuple(types) = ty {
                for (pat, elem_ty) in pats.iter().zip(types.iter()) {
                    register_pattern_bindings(ctx, pat, elem_ty);
                }
            } else {
                for pat in pats {
                    register_pattern_bindings(ctx, pat, &HirType::Unresolved);
                }
            }
        }
        HirPatternKind::Struct { fields, .. } => {
            for field in fields {
                if let Some(inner_pat) = &field.pattern {
                    register_pattern_bindings(ctx, inner_pat, &HirType::Unresolved);
                } else {
                    // Shorthand: `{ name }` — the field name is the binding.
                    ctx.scope.define(
                        field.name.clone(),
                        Symbol::Variable {
                            ty: HirType::Unresolved,
                        },
                    );
                }
            }
        }
        HirPatternKind::TupleStruct { fields, .. } => {
            for field in fields {
                register_pattern_bindings(ctx, field, &HirType::Unresolved);
            }
        }
        HirPatternKind::Wildcard
        | HirPatternKind::Literal(_)
        | HirPatternKind::Path(_)
        | HirPatternKind::Rest => {}
    }
}

// ── Expression Lowering ─────────────────────────────────────

fn lower_expr(ctx: &mut LowerCtx, expr: &Expr) -> Result<HirExpr, HirError> {
    let (kind, ty) = match &expr.kind {
        ExprKind::Literal(lit) => {
            let ty = literal_type(lit);
            (HirExprKind::Literal(lit.clone()), ty)
        }
        ExprKind::Path(path) => {
            let ty = resolve_path_type(ctx, path);
            (HirExprKind::Path(path.segments.clone()), ty)
        }
        ExprKind::BinaryOp { left, op, right } => {
            let hir_left = lower_expr(ctx, left)?;
            let hir_right = lower_expr(ctx, right)?;
            (
                HirExprKind::BinaryOp {
                    left: Box::new(hir_left),
                    op: *op,
                    right: Box::new(hir_right),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::UnaryOp { op, operand } => {
            let hir_operand = lower_expr(ctx, operand)?;
            (
                HirExprKind::UnaryOp {
                    op: *op,
                    operand: Box::new(hir_operand),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::FnCall { func, args } => {
            let hir_func = lower_expr(ctx, func)?;
            let hir_args = lower_exprs(ctx, args)?;
            // Try to resolve return type from function signature.
            let ty = resolve_call_return_type(ctx, &func.kind);
            (
                HirExprKind::FnCall {
                    func: Box::new(hir_func),
                    args: hir_args,
                },
                ty,
            )
        }
        ExprKind::MacroCall {
            path,
            delimiter,
            tokens,
        } => (
            HirExprKind::MacroCall {
                path: path.segments.clone(),
                delimiter: *delimiter,
                tokens: tokens.clone(),
            },
            HirType::Unresolved,
        ),
        ExprKind::MethodCall {
            receiver,
            method,
            args,
        } => {
            let hir_receiver = lower_expr(ctx, receiver)?;
            let hir_args = lower_exprs(ctx, args)?;
            (
                HirExprKind::MethodCall {
                    receiver: Box::new(hir_receiver),
                    method: method.clone(),
                    args: hir_args,
                },
                HirType::Unresolved,
            )
        }
        ExprKind::FieldAccess { receiver, field } => {
            let hir_receiver = lower_expr(ctx, receiver)?;
            (
                HirExprKind::FieldAccess {
                    receiver: Box::new(hir_receiver),
                    field: field.clone(),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::OptionalChain { receiver, access } => {
            let hir_receiver = lower_expr(ctx, receiver)?;
            let hir_access = match access {
                OptionalAccess::Field(name) => HirOptionalAccess::Field(name.clone()),
                OptionalAccess::Method { name, args } => HirOptionalAccess::Method {
                    name: name.clone(),
                    args: lower_exprs(ctx, args)?,
                },
            };
            (
                HirExprKind::OptionalChain {
                    receiver: Box::new(hir_receiver),
                    access: hir_access,
                },
                HirType::Unresolved,
            )
        }
        ExprKind::NullCoalesce { expr, default } => {
            let hir_expr = lower_expr(ctx, expr)?;
            let hir_default = lower_expr(ctx, default)?;
            (
                HirExprKind::NullCoalesce {
                    expr: Box::new(hir_expr),
                    default: Box::new(hir_default),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Index { receiver, index } => {
            let hir_receiver = lower_expr(ctx, receiver)?;
            let hir_index = lower_expr(ctx, index)?;
            (
                HirExprKind::Index {
                    receiver: Box::new(hir_receiver),
                    index: Box::new(hir_index),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Block(block) => {
            let hir_block = lower_block(ctx, block)?;
            let ty = hir_block
                .tail_expr
                .as_ref()
                .map(|e| e.ty.clone())
                .unwrap_or(HirType::Unit);
            (HirExprKind::Block(hir_block), ty)
        }
        ExprKind::If {
            condition,
            then_block,
            else_expr,
        } => {
            let hir_cond = lower_expr(ctx, condition)?;
            let hir_then = lower_block(ctx, then_block)?;
            let hir_else = if let Some(e) = else_expr {
                Some(Box::new(lower_expr(ctx, e)?))
            } else {
                None
            };
            (
                HirExprKind::If {
                    condition: Box::new(hir_cond),
                    then_block: hir_then,
                    else_expr: hir_else,
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Match { expr, arms } => {
            let hir_expr = lower_expr(ctx, expr)?;
            let hir_arms = lower_match_arms(ctx, arms)?;
            (
                HirExprKind::Match {
                    expr: Box::new(hir_expr),
                    arms: hir_arms,
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Await { expr } => {
            let hir_expr = lower_expr(ctx, expr)?;
            (
                HirExprKind::Await {
                    expr: Box::new(hir_expr),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Assign { target, value } => {
            let hir_target = lower_expr(ctx, target)?;
            let hir_value = lower_expr(ctx, value)?;
            (
                HirExprKind::Assign {
                    target: Box::new(hir_target),
                    value: Box::new(hir_value),
                },
                HirType::Unit,
            )
        }
        ExprKind::CompoundAssign { target, op, value } => {
            let hir_target = lower_expr(ctx, target)?;
            let hir_value = lower_expr(ctx, value)?;
            (
                HirExprKind::CompoundAssign {
                    target: Box::new(hir_target),
                    op: *op,
                    value: Box::new(hir_value),
                },
                HirType::Unit,
            )
        }
        ExprKind::StructLiteral { path, fields } => {
            let hir_fields = lower_field_inits(ctx, fields)?;
            let ty = resolve_struct_type(ctx, path);
            (
                HirExprKind::StructLiteral {
                    path: path.segments.clone(),
                    fields: hir_fields,
                },
                ty,
            )
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            let hir_start = if let Some(s) = start {
                Some(Box::new(lower_expr(ctx, s)?))
            } else {
                None
            };
            let hir_end = if let Some(e) = end {
                Some(Box::new(lower_expr(ctx, e)?))
            } else {
                None
            };
            (
                HirExprKind::Range {
                    start: hir_start,
                    end: hir_end,
                    inclusive: *inclusive,
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Closure {
            params,
            return_ty,
            body,
        } => {
            ctx.scope.push();
            let hir_params = lower_closure_params(ctx, params);
            let hir_return_ty = return_ty
                .as_ref()
                .map(|t| lower_type(t))
                .unwrap_or(HirType::Unresolved);
            let hir_body = lower_expr(ctx, body)?;
            ctx.scope.pop();
            (
                HirExprKind::Closure {
                    params: hir_params,
                    return_ty: hir_return_ty,
                    body: Box::new(hir_body),
                },
                HirType::Unresolved,
            )
        }
        ExprKind::Return(expr) => {
            let hir_expr = if let Some(e) = expr {
                Some(Box::new(lower_expr(ctx, e)?))
            } else {
                None
            };
            (HirExprKind::Return(hir_expr), HirType::Unit)
        }
        ExprKind::Tuple(elems) => {
            let hir_elems = lower_exprs(ctx, elems)?;
            let ty = if hir_elems.is_empty() {
                HirType::Unit
            } else {
                HirType::Tuple(hir_elems.iter().map(|e| e.ty.clone()).collect())
            };
            (HirExprKind::Tuple(hir_elems), ty)
        }
    };

    Ok(HirExpr {
        kind,
        ty,
        span: expr.span,
    })
}

fn lower_exprs(ctx: &mut LowerCtx, exprs: &[Expr]) -> Result<Vec<HirExpr>, HirError> {
    exprs.iter().map(|e| lower_expr(ctx, e)).collect()
}

fn lower_match_arms(
    ctx: &mut LowerCtx,
    arms: &[MatchArm],
) -> Result<Vec<HirMatchArm>, HirError> {
    arms.iter()
        .map(|arm| {
            ctx.scope.push();
            let hir_pattern = lower_pattern(&arm.pattern);
            // Register match arm pattern bindings.
            register_pattern_bindings(ctx, &hir_pattern, &HirType::Unresolved);
            let guard = if let Some(g) = &arm.guard {
                Some(Box::new(lower_expr(ctx, g)?))
            } else {
                None
            };
            let body = lower_expr(ctx, &arm.body)?;
            ctx.scope.pop();
            Ok(HirMatchArm {
                pattern: hir_pattern,
                guard,
                body,
                span: arm.span,
            })
        })
        .collect()
}

fn lower_field_inits(
    ctx: &mut LowerCtx,
    fields: &[FieldInit],
) -> Result<Vec<HirFieldInit>, HirError> {
    fields
        .iter()
        .map(|f| {
            let value = if let Some(v) = &f.value {
                Some(lower_expr(ctx, v)?)
            } else {
                None
            };
            Ok(HirFieldInit {
                name: f.name.clone(),
                value,
                span: f.span,
            })
        })
        .collect()
}

fn lower_closure_params(ctx: &mut LowerCtx, params: &[ClosureParam]) -> Vec<HirClosureParam> {
    params
        .iter()
        .map(|p| {
            let ty = p
                .ty
                .as_ref()
                .map(|t| lower_type(t))
                .unwrap_or(HirType::Unresolved);
            ctx.scope
                .define(p.name.clone(), Symbol::Variable { ty: ty.clone() });
            HirClosureParam {
                name: p.name.clone(),
                ty,
                span: p.span,
            }
        })
        .collect()
}

// ── Pattern Lowering ────────────────────────────────────────

fn lower_pattern(pattern: &Pattern) -> HirPattern {
    let kind = match &pattern.kind {
        PatternKind::Wildcard => HirPatternKind::Wildcard,
        PatternKind::Ident(name) => HirPatternKind::Ident(name.clone()),
        PatternKind::Literal(lit) => HirPatternKind::Literal(lit.clone()),
        PatternKind::Tuple(pats) => {
            HirPatternKind::Tuple(pats.iter().map(|p| lower_pattern(p)).collect())
        }
        PatternKind::Struct { path, fields } => HirPatternKind::Struct {
            path: path.segments.clone(),
            fields: fields.iter().map(|f| lower_field_pattern(f)).collect(),
        },
        PatternKind::TupleStruct { path, fields } => HirPatternKind::TupleStruct {
            path: path.segments.clone(),
            fields: fields.iter().map(|p| lower_pattern(p)).collect(),
        },
        PatternKind::Path(path) => HirPatternKind::Path(path.segments.clone()),
        PatternKind::Rest => HirPatternKind::Rest,
    };
    HirPattern {
        kind,
        span: pattern.span,
    }
}

fn lower_field_pattern(fp: &FieldPattern) -> HirFieldPattern {
    HirFieldPattern {
        name: fp.name.clone(),
        pattern: fp.pattern.as_ref().map(|p| lower_pattern(p)),
        span: fp.span,
    }
}

// ── Type Resolution Helpers ─────────────────────────────────

/// Determine the type of a literal.
fn literal_type(lit: &Literal) -> HirType {
    match lit {
        // Int/Float: leave as Unresolved — Rust infers the exact numeric type.
        Literal::Int(_) => HirType::Unresolved,
        Literal::Float(_) => HirType::Unresolved,
        Literal::String(_) => HirType::Named {
            path: vec!["String".to_string()],
            generics: vec![],
        },
        Literal::Char(_) => HirType::Primitive(PrimitiveType::Char),
        Literal::Bool(_) => HirType::Primitive(PrimitiveType::Bool),
    }
}

/// Resolve the type of a path expression by looking up the scope.
fn resolve_path_type(ctx: &LowerCtx, path: &Path) -> HirType {
    if path.segments.len() == 1 {
        if let Some(sym) = ctx.scope.lookup(&path.segments[0]) {
            match sym {
                Symbol::Variable { ty } => return ty.clone(),
                Symbol::Function { .. } => {
                    return HirType::Unresolved;
                }
                Symbol::Struct { .. } => return HirType::Unresolved,
                Symbol::Enum { .. } => return HirType::Unresolved,
                Symbol::Trait { .. } => return HirType::Unresolved,
            }
        }
    }
    HirType::Unresolved
}

/// Resolve the return type of a function call.
fn resolve_call_return_type(ctx: &LowerCtx, func_expr: &ExprKind) -> HirType {
    match func_expr {
        ExprKind::Path(path) => {
            // Simple function call: `foo(args)`.
            let name = if path.segments.len() == 1 {
                &path.segments[0]
            } else {
                // Qualified path like `Vec::new()` — can't resolve without
                // full type system, leave unresolved.
                return HirType::Unresolved;
            };
            if let Some(Symbol::Function { return_ty, .. }) = ctx.scope.lookup(name) {
                return return_ty.clone();
            }
            HirType::Unresolved
        }
        _ => HirType::Unresolved,
    }
}

/// Resolve the type of a struct literal.
fn resolve_struct_type(ctx: &LowerCtx, path: &Path) -> HirType {
    if path.segments.len() == 1 {
        if let Some(Symbol::Struct { .. }) = ctx.scope.lookup(&path.segments[0]) {
            return HirType::Named {
                path: path.segments.clone(),
                generics: vec![],
            };
        }
    }
    // Even if not found in scope (external type), use the path as the type.
    HirType::Named {
        path: path.segments.clone(),
        generics: vec![],
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nanachi_ast::types::PrimitiveType;

    fn lower_str(src: &str) -> HirProgram {
        let tokens = nanachi_lexer::lex(src).expect("lex failed");
        let ast = nanachi_parser::parse(&tokens).expect("parse failed");
        lower(&ast).expect("lower failed")
    }

    fn expect_fn<'a>(prog: &'a HirProgram, idx: usize, name: &str) -> &'a HirFunction {
        match &prog.items[idx].kind {
            HirItemKind::Function(f) => {
                assert_eq!(f.name, name);
                f
            }
            _ => panic!("expected function '{name}' at index {idx}"),
        }
    }

    // ── Type lowering ───────────────────────────────────────

    #[test]
    fn type_primitive_annotation() {
        let prog = lower_str("fn f() { let x: i32 = 5; }");
        let f = expect_fn(&prog, 0, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::Let { ty, .. } => {
                assert_eq!(ty, &HirType::Primitive(PrimitiveType::I32));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn type_option_desugaring() {
        // `User?` → `HirType::Option(Named "User")`
        let prog = lower_str("fn find_user(id: i32) -> User? { None }");
        let f = expect_fn(&prog, 0, "find_user");
        match &f.return_ty {
            HirType::Option(inner) => {
                assert!(matches!(
                    inner.as_ref(),
                    HirType::Named { path, generics } if path == &["User"] && generics.is_empty()
                ));
            }
            _ => panic!("expected Option type, got {:?}", f.return_ty),
        }
    }

    #[test]
    fn type_generic_named() {
        let prog = lower_str("fn f(v: Vec<i32>) {}");
        let f = expect_fn(&prog, 0, "f");
        match &f.params[0].kind {
            HirFnParamKind::Typed { ty, .. } => match ty {
                HirType::Named { path, generics } => {
                    assert_eq!(path, &["Vec"]);
                    assert_eq!(generics.len(), 1);
                    assert_eq!(generics[0], HirType::Primitive(PrimitiveType::I32));
                }
                _ => panic!("expected Named type"),
            },
            _ => panic!("expected typed param"),
        }
    }

    #[test]
    fn type_tuple() {
        let prog = lower_str("fn f(t: (i32, f64)) {}");
        let f = expect_fn(&prog, 0, "f");
        match &f.params[0].kind {
            HirFnParamKind::Typed { ty, .. } => match ty {
                HirType::Tuple(types) => {
                    assert_eq!(types.len(), 2);
                    assert_eq!(types[0], HirType::Primitive(PrimitiveType::I32));
                    assert_eq!(types[1], HirType::Primitive(PrimitiveType::F64));
                }
                _ => panic!("expected Tuple type"),
            },
            _ => panic!("expected typed param"),
        }
    }

    #[test]
    fn type_unit_return() {
        let prog = lower_str("fn f() {}");
        let f = expect_fn(&prog, 0, "f");
        assert_eq!(f.return_ty, HirType::Unit);
    }

    // ── Scope & name resolution ─────────────────────────────

    #[test]
    fn scope_variable_type_from_let() {
        let prog = lower_str(
            r#"fn f() {
                let x: i32 = 5;
                x;
            }"#,
        );
        let f = expect_fn(&prog, 0, "f");
        // The second stmt (x;) should resolve x's type from scope.
        match &f.body.stmts[1].kind {
            HirStmtKind::Expr(expr) => {
                assert_eq!(expr.ty, HirType::Primitive(PrimitiveType::I32));
            }
            _ => panic!("expected expr stmt"),
        }
    }

    #[test]
    fn scope_fn_call_return_type() {
        let prog = lower_str(
            r#"
            fn get_value() -> i32 { 42 }
            fn f() { let x = get_value(); }
        "#,
        );
        let f = expect_fn(&prog, 1, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::Let { value, .. } => {
                let call = value.as_ref().unwrap();
                assert_eq!(call.ty, HirType::Primitive(PrimitiveType::I32));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn scope_fn_param_type() {
        let prog = lower_str(
            r#"fn greet(name: String) {
                name;
            }"#,
        );
        let f = expect_fn(&prog, 0, "greet");
        // `name;` should resolve to String type.
        match &f.body.stmts[0].kind {
            HirStmtKind::Expr(expr) => {
                assert!(matches!(
                    &expr.ty,
                    HirType::Named { path, .. } if path == &["String"]
                ));
            }
            _ => panic!("expected expr stmt"),
        }
    }

    // ── Struct ──────────────────────────────────────────────

    #[test]
    fn struct_definition_in_scope() {
        let prog = lower_str(
            r#"
            struct User { name: String, age: i32 }
            fn f() { let u = User { name: "a", age: 1 }; }
        "#,
        );
        let f = expect_fn(&prog, 1, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::Let { value, .. } => {
                let expr = value.as_ref().unwrap();
                assert!(matches!(
                    &expr.ty,
                    HirType::Named { path, .. } if path == &["User"]
                ));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn struct_fields_lowered() {
        let prog = lower_str("struct Point { x: f64, y: f64 }");
        match &prog.items[0].kind {
            HirItemKind::Struct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].ty, HirType::Primitive(PrimitiveType::F64));
                assert_eq!(s.fields[1].ty, HirType::Primitive(PrimitiveType::F64));
            }
            _ => panic!("expected struct"),
        }
    }

    // ── Enum ────────────────────────────────────────────────

    #[test]
    fn enum_variants_lowered() {
        let prog = lower_str("enum Shape { Circle(f64), Rect { w: f64, h: f64 } }");
        match &prog.items[0].kind {
            HirItemKind::Enum(e) => {
                assert_eq!(e.name, "Shape");
                assert_eq!(e.variants.len(), 2);
                assert!(matches!(&e.variants[0].fields, HirVariantFields::Tuple(types) if types.len() == 1));
                assert!(matches!(&e.variants[1].fields, HirVariantFields::Struct(fields) if fields.len() == 2));
            }
            _ => panic!("expected enum"),
        }
    }

    // ── Trait / Impl ────────────────────────────────────────

    #[test]
    fn trait_lowered() {
        let prog = lower_str("trait Greet { fn greet(self) -> String; }");
        match &prog.items[0].kind {
            HirItemKind::Trait(t) => {
                assert_eq!(t.name, "Greet");
                assert_eq!(t.methods.len(), 1);
                assert_eq!(t.methods[0].name, "greet");
                assert!(matches!(
                    &t.methods[0].return_ty,
                    HirType::Named { path, .. } if path == &["String"]
                ));
            }
            _ => panic!("expected trait"),
        }
    }

    #[test]
    fn impl_block_lowered() {
        let prog = lower_str(
            r#"
            struct User { name: String, age: i32 }
            impl User {
                fn new(name: String, age: i32) -> User {
                    User { name, age }
                }
            }
        "#,
        );
        match &prog.items[1].kind {
            HirItemKind::Impl(i) => {
                assert!(i.trait_name.is_none());
                assert_eq!(i.methods.len(), 1);
                assert_eq!(i.methods[0].name, "new");
                assert!(matches!(
                    &i.methods[0].return_ty,
                    HirType::Named { path, .. } if path == &["User"]
                ));
            }
            _ => panic!("expected impl"),
        }
    }

    // ── Expression type tracking ────────────────────────────

    #[test]
    fn literal_types() {
        let prog = lower_str(
            r#"fn f() {
                "hello";
                true;
                'c';
                42;
            }"#,
        );
        let f = expect_fn(&prog, 0, "f");
        // String literal → Named "String"
        match &f.body.stmts[0].kind {
            HirStmtKind::Expr(e) => assert!(matches!(
                &e.ty,
                HirType::Named { path, .. } if path == &["String"]
            )),
            _ => panic!("expected expr"),
        }
        // Bool → Primitive(Bool)
        match &f.body.stmts[1].kind {
            HirStmtKind::Expr(e) => assert_eq!(e.ty, HirType::Primitive(PrimitiveType::Bool)),
            _ => panic!("expected expr"),
        }
        // Char → Primitive(Char)
        match &f.body.stmts[2].kind {
            HirStmtKind::Expr(e) => assert_eq!(e.ty, HirType::Primitive(PrimitiveType::Char)),
            _ => panic!("expected expr"),
        }
        // Int → Unresolved (Rust infers)
        match &f.body.stmts[3].kind {
            HirStmtKind::Expr(e) => assert_eq!(e.ty, HirType::Unresolved),
            _ => panic!("expected expr"),
        }
    }

    #[test]
    fn assign_type_is_unit() {
        let prog = lower_str("fn f() { let x: i32 = 5; x = 10; }");
        let f = expect_fn(&prog, 0, "f");
        match &f.body.stmts[1].kind {
            HirStmtKind::Expr(e) => assert_eq!(e.ty, HirType::Unit),
            _ => panic!("expected expr"),
        }
    }

    // ── OptionalChain / NullCoalesce preserved ──────────────

    #[test]
    fn optional_chain_preserved() {
        let prog = lower_str(
            r#"
            fn find_user(id: i32) -> User? { None }
            fn f() { let name = find_user(1)?.name; }
        "#,
        );
        let f = expect_fn(&prog, 1, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::Let { value, .. } => {
                let expr = value.as_ref().unwrap();
                assert!(matches!(&expr.kind, HirExprKind::OptionalChain { access: HirOptionalAccess::Field(f), .. } if f == "name"));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn null_coalesce_preserved() {
        let prog = lower_str(
            r#"
            fn find_user(id: i32) -> User? { None }
            fn f() { let age = find_user(1)?.age ?? 0; }
        "#,
        );
        let f = expect_fn(&prog, 1, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::Let { value, .. } => {
                let expr = value.as_ref().unwrap();
                assert!(matches!(&expr.kind, HirExprKind::NullCoalesce { .. }));
            }
            _ => panic!("expected let"),
        }
    }

    // ── Async ───────────────────────────────────────────────

    #[test]
    fn async_fn_lowered() {
        let prog = lower_str("async fn fetch() {}");
        let f = expect_fn(&prog, 0, "fetch");
        assert!(f.is_async);
    }

    // ── Use ─────────────────────────────────────────────────

    #[test]
    fn use_item_lowered() {
        let prog = lower_str("use std::io;");
        match &prog.items[0].kind {
            HirItemKind::Use(u) => match &u.tree {
                HirUseTree::Simple { path, alias } => {
                    assert_eq!(path, &["std", "io"]);
                    assert!(alias.is_none());
                }
                _ => panic!("expected simple use"),
            },
            _ => panic!("expected use"),
        }
    }

    // ── Rust block ──────────────────────────────────────────

    #[test]
    fn rust_block_lowered() {
        let prog = lower_str(
            r#"fn f() {
                rust {
                    let mut x = 5;
                }
            }"#,
        );
        let f = expect_fn(&prog, 0, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::Item(item) => match &item.kind {
                HirItemKind::RustBlock(rb) => {
                    assert!(rb.code.contains("let mut x = 5"));
                }
                _ => panic!("expected rust block"),
            },
            _ => panic!("expected item stmt"),
        }
    }

    // ── Generics ────────────────────────────────────────────

    #[test]
    fn generic_fn_lowered() {
        let prog = lower_str("fn identity<T>(x: T) -> T { x }");
        let f = expect_fn(&prog, 0, "identity");
        assert_eq!(f.generics.len(), 1);
        assert_eq!(f.generics[0].name, "T");
    }

    #[test]
    fn generic_fn_with_bound() {
        let prog = lower_str("fn print_it<T: Display>(item: T) {}");
        let f = expect_fn(&prog, 0, "print_it");
        assert_eq!(f.generics.len(), 1);
        assert_eq!(f.generics[0].bounds.len(), 1);
        assert!(matches!(
            &f.generics[0].bounds[0],
            HirType::Named { path, .. } if path == &["Display"]
        ));
    }

    // ── Control flow ────────────────────────────────────────

    #[test]
    fn for_loop_lowered() {
        let prog = lower_str(
            r#"fn f() {
                for i: i32 in items {
                    i;
                }
            }"#,
        );
        let f = expect_fn(&prog, 0, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::For {
                pattern, iter_ty, body, ..
            } => {
                assert!(matches!(&pattern.kind, HirPatternKind::Ident(n) if n == "i"));
                assert_eq!(iter_ty, &HirType::Primitive(PrimitiveType::I32));
                // `i;` inside loop body should resolve i's type
                match &body.stmts[0].kind {
                    HirStmtKind::Expr(e) => {
                        assert_eq!(e.ty, HirType::Primitive(PrimitiveType::I32));
                    }
                    _ => panic!("expected expr"),
                }
            }
            _ => panic!("expected for"),
        }
    }

    #[test]
    fn while_loop_lowered() {
        let prog = lower_str("fn f() { while true { break; } }");
        let f = expect_fn(&prog, 0, "f");
        match &f.body.stmts[0].kind {
            HirStmtKind::While { condition, body } => {
                assert!(matches!(&condition.kind, HirExprKind::Literal(Literal::Bool(true))));
                assert!(matches!(&body.stmts[0].kind, HirStmtKind::Break(None)));
            }
            _ => panic!("expected while"),
        }
    }

    // ── Match ───────────────────────────────────────────────

    #[test]
    fn match_arms_lowered() {
        let prog = lower_str(
            r#"fn f() {
                match x {
                    0 => 1,
                    _ => 2,
                }
            }"#,
        );
        let f = expect_fn(&prog, 0, "f");
        let expr = match &f.body.stmts[0].kind {
            HirStmtKind::Expr(e) => e,
            _ => panic!("expected expr"),
        };
        match &expr.kind {
            HirExprKind::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(&arms[1].pattern.kind, HirPatternKind::Wildcard));
            }
            _ => panic!("expected match"),
        }
    }

    // ── Closure ─────────────────────────────────────────────

    #[test]
    fn closure_lowered() {
        let prog = lower_str("fn f() { |x: i32| x + 1; }");
        let f = expect_fn(&prog, 0, "f");
        let expr = match &f.body.stmts[0].kind {
            HirStmtKind::Expr(e) => e,
            _ => panic!("expected expr stmt"),
        };
        match &expr.kind {
            HirExprKind::Closure { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "x");
                assert_eq!(params[0].ty, HirType::Primitive(PrimitiveType::I32));
            }
            _ => panic!("expected closure"),
        }
    }

    // ── Comprehensive sketch tests ──────────────────────────

    #[test]
    fn sketch_struct_impl_full() {
        let prog = lower_str(
            r#"
            struct User {
                name: String,
                age: i32,
            }

            impl User {
                fn new(name: String, age: i32) -> User {
                    User { name, age }
                }

                fn greet(self) {
                    println!("Hi, I'm {} ({})", self.name, self.age);
                }

                fn grow(self) {
                    self.age = self.age + 1;
                }
            }
        "#,
        );
        assert_eq!(prog.items.len(), 2);

        // Struct fields have resolved types.
        match &prog.items[0].kind {
            HirItemKind::Struct(s) => {
                assert_eq!(s.name, "User");
                assert!(matches!(
                    &s.fields[0].ty,
                    HirType::Named { path, .. } if path == &["String"]
                ));
                assert_eq!(s.fields[1].ty, HirType::Primitive(PrimitiveType::I32));
            }
            _ => panic!("expected struct"),
        }

        // Impl methods lowered.
        match &prog.items[1].kind {
            HirItemKind::Impl(i) => {
                assert_eq!(i.methods.len(), 3);
                assert_eq!(i.methods[0].name, "new");
                assert_eq!(i.methods[1].name, "greet");
                assert_eq!(i.methods[2].name, "grow");
                // new() returns User
                assert!(matches!(
                    &i.methods[0].return_ty,
                    HirType::Named { path, .. } if path == &["User"]
                ));
                // greet has self param
                assert!(matches!(
                    &i.methods[1].params[0].kind,
                    HirFnParamKind::SelfParam
                ));
            }
            _ => panic!("expected impl"),
        }
    }

    #[test]
    fn sketch_option_full() {
        let prog = lower_str(
            r#"
            fn find_user(id: i32) -> User? {
                if id == 1 {
                    Some(User { name: "nanachi", age: 3 })
                } else {
                    None
                }
            }

            fn option_example() {
                let user: User? = find_user(42);
                let name: String? = find_user(1)?.name;
                let age: i32 = find_user(1)?.age ?? 0;
            }
        "#,
        );
        // find_user return type: Option(Named "User")
        let fu = expect_fn(&prog, 0, "find_user");
        assert!(matches!(&fu.return_ty, HirType::Option(inner) if matches!(inner.as_ref(), HirType::Named { path, .. } if path == &["User"])));

        // option_example variables
        let oe = expect_fn(&prog, 1, "option_example");
        // let user: User? → Option(Named "User")
        match &oe.body.stmts[0].kind {
            HirStmtKind::Let { ty, value, .. } => {
                assert!(matches!(ty, HirType::Option(inner) if matches!(inner.as_ref(), HirType::Named { path, .. } if path == &["User"])));
                // find_user(42) should return Option(Named "User")
                let call = value.as_ref().unwrap();
                assert!(matches!(call.ty, HirType::Option(_)));
            }
            _ => panic!("expected let"),
        }
        // let name: String? → Option(Named "String")
        match &oe.body.stmts[1].kind {
            HirStmtKind::Let { ty, .. } => {
                assert!(matches!(ty, HirType::Option(inner) if matches!(inner.as_ref(), HirType::Named { path, .. } if path == &["String"])));
            }
            _ => panic!("expected let"),
        }
        // let age: i32
        match &oe.body.stmts[2].kind {
            HirStmtKind::Let { ty, value, .. } => {
                assert_eq!(ty, &HirType::Primitive(PrimitiveType::I32));
                // value is NullCoalesce
                let val = value.as_ref().unwrap();
                assert!(matches!(&val.kind, HirExprKind::NullCoalesce { .. }));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn sketch_comprehensive() {
        let prog = lower_str(
            r#"
            fn load_users(path: String) -> Vec<User> {
                let content: String = fs::read_to_string(path);
                let lines: Vec<String> = content.lines().collect();
                let users: Vec<User> = Vec::new();
                for line: String in lines {
                    let parts: Vec<String> = line.split(',').collect();
                    let name: String = parts[0];
                    let age: i32 = parts[1].parse();
                    users.push(User::new(name, age));
                }
                users
            }
        "#,
        );
        let f = expect_fn(&prog, 0, "load_users");
        // Return type: Vec<User>
        match &f.return_ty {
            HirType::Named { path, generics } => {
                assert_eq!(path, &["Vec"]);
                assert_eq!(generics.len(), 1);
                assert!(matches!(
                    &generics[0],
                    HirType::Named { path, .. } if path == &["User"]
                ));
            }
            _ => panic!("expected Vec<User>"),
        }
        // 4 stmts + tail expr
        assert_eq!(f.body.stmts.len(), 4);
        assert!(f.body.tail_expr.is_some());
        // for loop
        match &f.body.stmts[3].kind {
            HirStmtKind::For { pattern, iter_ty, body, .. } => {
                assert!(matches!(&pattern.kind, HirPatternKind::Ident(n) if n == "line"));
                assert!(matches!(iter_ty, HirType::Named { path, .. } if path == &["String"]));
                assert_eq!(body.stmts.len(), 4);
            }
            _ => panic!("expected for"),
        }
    }
}
