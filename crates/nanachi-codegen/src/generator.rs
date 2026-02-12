use std::collections::{HashMap, HashSet};

use nanachi_analyzer::{
    AnalysisResult, ArgAction, CallSiteInfo, FnAnalysis, FnKey, ParamSig, SelfSig,
};
use nanachi_ast::expr::{BinOp, CompoundOp, Literal, MacroDelimiter, UnOp};
use nanachi_ast::item::Visibility;
use nanachi_ast::types::PrimitiveType;
use nanachi_hir::{
    HirBlock, HirExpr, HirExprKind, HirFieldPattern, HirFnParamKind, HirFunction, HirGenericParam,
    HirImpl, HirItem, HirItemKind, HirMatchArm, HirOptionalAccess, HirPattern, HirPatternKind,
    HirProgram, HirStmt, HirStmtKind, HirTrait, HirTraitMethod, HirType, HirUseTree,
    HirVariantFields, HirWherePredicate,
};
use nanachi_lexer::Span;

use crate::CodegenError;

const INDENT: &str = "    ";

#[derive(Debug, Clone, Default)]
struct UnifiedMethodSig {
    self_sig: Option<SelfSig>,
    param_sigs: HashMap<String, ParamSig>,
}

#[derive(Debug, Clone)]
struct FunctionCtx<'a> {
    analysis: Option<&'a FnAnalysis>,
    needs_result_wrap: bool,
    allow_question_mark: bool,
    expects_value: bool,
}

impl<'a> FunctionCtx<'a> {
    fn is_mutable(&self, name: &str) -> bool {
        self.analysis
            .map(|a| a.mutable_vars.contains(name))
            .unwrap_or(false)
    }

    fn call_site(&self, span: Span) -> Option<&CallSiteInfo> {
        self.analysis.and_then(|a| a.call_sites.get(&span))
    }
}

pub fn generate(hir: &HirProgram, analysis: &AnalysisResult) -> Result<String, CodegenError> {
    Generator::new(hir, analysis).generate_program(hir)
}

struct Generator<'a> {
    analysis: &'a AnalysisResult,
    resolved_error_enum_names: HashMap<FnKey, String>,
    trait_method_sigs: HashMap<(String, String), UnifiedMethodSig>,
    emitted_error_enums: HashSet<String>,
}

impl<'a> Generator<'a> {
    fn new(hir: &HirProgram, analysis: &'a AnalysisResult) -> Self {
        Self {
            analysis,
            resolved_error_enum_names: resolve_error_enum_names(analysis),
            trait_method_sigs: collect_trait_method_sigs(hir, analysis),
            emitted_error_enums: HashSet::new(),
        }
    }

    fn generate_program(mut self, hir: &HirProgram) -> Result<String, CodegenError> {
        let mut parts = Vec::new();
        for item in &hir.items {
            parts.push(self.render_item(item)?);
        }
        let mut out = parts
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    }

    fn render_item(&mut self, item: &HirItem) -> Result<String, CodegenError> {
        match &item.kind {
            HirItemKind::Function(func) => {
                let mut chunks = Vec::new();
                if let Some(enum_code) = self.render_error_enum_for_function(None, func) {
                    chunks.push(enum_code);
                }
                chunks.push(self.render_function(func, None, None, true, 0)?);
                Ok(chunks.join("\n\n"))
            }
            HirItemKind::Struct(s) => {
                let mut out = String::new();
                out.push_str(render_visibility(s.visibility));
                out.push_str("struct ");
                out.push_str(&s.name);
                out.push_str(&render_generics(&s.generics));
                out.push_str(" {\n");
                for field in &s.fields {
                    push_indent(&mut out, 1);
                    out.push_str(render_visibility(field.visibility));
                    out.push_str(&field.name);
                    out.push_str(": ");
                    out.push_str(&render_type(&field.ty));
                    out.push_str(",\n");
                }
                out.push('}');
                Ok(out)
            }
            HirItemKind::Enum(e) => {
                let mut out = String::new();
                out.push_str(render_visibility(e.visibility));
                out.push_str("enum ");
                out.push_str(&e.name);
                out.push_str(&render_generics(&e.generics));
                out.push_str(" {\n");
                for variant in &e.variants {
                    push_indent(&mut out, 1);
                    out.push_str(&variant.name);
                    match &variant.fields {
                        HirVariantFields::Unit => {}
                        HirVariantFields::Tuple(types) => {
                            out.push('(');
                            out.push_str(
                                &types.iter().map(render_type).collect::<Vec<_>>().join(", "),
                            );
                            out.push(')');
                        }
                        HirVariantFields::Struct(fields) => {
                            out.push_str(" { ");
                            out.push_str(
                                &fields
                                    .iter()
                                    .map(|f| format!("{}: {}", f.name, render_type(&f.ty)))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            );
                            out.push_str(" }");
                        }
                    }
                    out.push_str(",\n");
                }
                out.push('}');
                Ok(out)
            }
            HirItemKind::Trait(tr) => self.render_trait(tr),
            HirItemKind::Impl(imp) => self.render_impl(imp),
            HirItemKind::Use(u) => {
                let mut out = String::new();
                out.push_str(render_visibility(u.visibility));
                out.push_str("use ");
                out.push_str(&render_use_tree(&u.tree));
                out.push(';');
                Ok(out)
            }
            HirItemKind::RustBlock(rb) => Ok(rb.code.clone()),
        }
    }

    fn render_item_no_enums(&self, item: &HirItem) -> Result<String, CodegenError> {
        match &item.kind {
            HirItemKind::Function(func) => self.render_function(func, None, None, true, 0),
            HirItemKind::Struct(s) => {
                let mut out = String::new();
                out.push_str(render_visibility(s.visibility));
                out.push_str("struct ");
                out.push_str(&s.name);
                out.push_str(&render_generics(&s.generics));
                out.push_str(" {\n");
                for field in &s.fields {
                    push_indent(&mut out, 1);
                    out.push_str(render_visibility(field.visibility));
                    out.push_str(&field.name);
                    out.push_str(": ");
                    out.push_str(&render_type(&field.ty));
                    out.push_str(",\n");
                }
                out.push('}');
                Ok(out)
            }
            HirItemKind::Enum(e) => {
                let mut out = String::new();
                out.push_str(render_visibility(e.visibility));
                out.push_str("enum ");
                out.push_str(&e.name);
                out.push_str(&render_generics(&e.generics));
                out.push_str(" {\n");
                for variant in &e.variants {
                    push_indent(&mut out, 1);
                    out.push_str(&variant.name);
                    match &variant.fields {
                        HirVariantFields::Unit => {}
                        HirVariantFields::Tuple(types) => {
                            out.push('(');
                            out.push_str(
                                &types.iter().map(render_type).collect::<Vec<_>>().join(", "),
                            );
                            out.push(')');
                        }
                        HirVariantFields::Struct(fields) => {
                            out.push_str(" { ");
                            out.push_str(
                                &fields
                                    .iter()
                                    .map(|f| format!("{}: {}", f.name, render_type(&f.ty)))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            );
                            out.push_str(" }");
                        }
                    }
                    out.push_str(",\n");
                }
                out.push('}');
                Ok(out)
            }
            HirItemKind::Trait(tr) => self.render_trait_no_enums(tr),
            HirItemKind::Impl(imp) => self.render_impl_no_enums(imp),
            HirItemKind::Use(u) => {
                let mut out = String::new();
                out.push_str(render_visibility(u.visibility));
                out.push_str("use ");
                out.push_str(&render_use_tree(&u.tree));
                out.push(';');
                Ok(out)
            }
            HirItemKind::RustBlock(rb) => Ok(rb.code.clone()),
        }
    }

    fn render_trait(&mut self, tr: &HirTrait) -> Result<String, CodegenError> {
        let mut chunks = Vec::new();
        let owner = Some(format!("trait::{}", tr.name));
        for method in &tr.methods {
            if let Some(enum_code) = self.render_error_enum_for_method(owner.clone(), method) {
                chunks.push(enum_code);
            }
        }

        let mut out = String::new();
        out.push_str(render_visibility(tr.visibility));
        out.push_str("trait ");
        out.push_str(&tr.name);
        out.push_str(&render_generics(&tr.generics));
        out.push_str(" {\n");

        for method in &tr.methods {
            let key = (tr.name.clone(), method.name.clone());
            let override_sig = self.trait_method_sigs.get(&key);
            out.push_str(&self.render_trait_method(tr, method, override_sig, 1)?);
            out.push('\n');
        }

        out.push('}');
        chunks.push(out);
        Ok(chunks.join("\n\n"))
    }

    fn render_trait_no_enums(&self, tr: &HirTrait) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str(render_visibility(tr.visibility));
        out.push_str("trait ");
        out.push_str(&tr.name);
        out.push_str(&render_generics(&tr.generics));
        out.push_str(" {\n");

        for method in &tr.methods {
            let key = (tr.name.clone(), method.name.clone());
            let override_sig = self.trait_method_sigs.get(&key);
            out.push_str(&self.render_trait_method(tr, method, override_sig, 1)?);
            out.push('\n');
        }

        out.push('}');
        Ok(out)
    }

    fn render_trait_method(
        &self,
        tr: &HirTrait,
        method: &HirTraitMethod,
        sig_override: Option<&UnifiedMethodSig>,
        indent: usize,
    ) -> Result<String, CodegenError> {
        let key = FnKey {
            name: method.name.clone(),
            owner: Some(format!("trait::{}", tr.name)),
        };
        let analysis = self.analysis.functions.get(&key);
        let (return_ty, needs_result_wrap, allow_question_mark) =
            self.render_return_type(&method.return_ty, analysis, &key)?;
        let ctx = FunctionCtx {
            analysis,
            needs_result_wrap,
            allow_question_mark,
            expects_value: !matches!(method.return_ty, HirType::Unit),
        };

        let params = method
            .params
            .iter()
            .map(|param| self.render_fn_param_kind(&param.kind, &ctx, sig_override))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");

        let mut out = String::new();
        push_indent(&mut out, indent);
        out.push_str("fn ");
        out.push_str(&method.name);
        out.push_str(&render_generics(&method.generics));
        out.push('(');
        out.push_str(&params);
        out.push(')');
        out.push_str(&return_ty);

        if let Some(body) = &method.default_body {
            out.push(' ');
            out.push_str(&self.render_function_block(body, &ctx, indent)?);
        } else {
            out.push(';');
        }

        Ok(out)
    }

    fn render_impl(&mut self, imp: &HirImpl) -> Result<String, CodegenError> {
        let owner = impl_owner(&imp.target);
        let mut chunks = Vec::new();

        for method in &imp.methods {
            if let Some(enum_code) = self.render_error_enum_for_function(owner.clone(), method) {
                chunks.push(enum_code);
            }
        }

        let mut out = String::new();
        out.push_str("impl");
        out.push_str(&render_generics(&imp.generics));
        out.push(' ');
        if let Some(trait_name) = &imp.trait_name {
            out.push_str(&trait_name.join("::"));
            out.push_str(" for ");
        }
        out.push_str(&render_type(&imp.target));
        out.push_str(" {\n");

        for (idx, method) in imp.methods.iter().enumerate() {
            out.push_str(&self.render_function(
                method,
                owner.clone(),
                None,
                imp.trait_name.is_none(),
                1,
            )?);
            if idx + 1 < imp.methods.len() {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }

        out.push('}');
        chunks.push(out);
        Ok(chunks.join("\n\n"))
    }

    fn render_impl_no_enums(&self, imp: &HirImpl) -> Result<String, CodegenError> {
        let owner = impl_owner(&imp.target);
        let mut out = String::new();
        out.push_str("impl");
        out.push_str(&render_generics(&imp.generics));
        out.push(' ');
        if let Some(trait_name) = &imp.trait_name {
            out.push_str(&trait_name.join("::"));
            out.push_str(" for ");
        }
        out.push_str(&render_type(&imp.target));
        out.push_str(" {\n");

        for (idx, method) in imp.methods.iter().enumerate() {
            out.push_str(&self.render_function(
                method,
                owner.clone(),
                None,
                imp.trait_name.is_none(),
                1,
            )?);
            if idx + 1 < imp.methods.len() {
                out.push_str("\n\n");
            } else {
                out.push('\n');
            }
        }

        out.push('}');
        Ok(out)
    }

    fn render_error_enum_for_function(
        &mut self,
        owner: Option<String>,
        func: &HirFunction,
    ) -> Option<String> {
        let key = FnKey {
            name: func.name.clone(),
            owner,
        };
        self.render_error_enum_for_key(&key)
    }

    fn render_error_enum_for_method(
        &mut self,
        owner: Option<String>,
        method: &HirTraitMethod,
    ) -> Option<String> {
        let key = FnKey {
            name: method.name.clone(),
            owner,
        };
        self.render_error_enum_for_key(&key)
    }

    fn render_error_enum_for_key(&mut self, key: &FnKey) -> Option<String> {
        let analysis = self.analysis.functions.get(key)?;
        let info = analysis.error_info.as_ref()?;
        if !info.needs_result_wrap || info.error_types.len() < 2 {
            return None;
        }
        let name = self
            .resolved_error_enum_names
            .get(key)
            .cloned()
            .or_else(|| info.error_enum_name.clone())
            .unwrap_or_else(|| make_error_enum_name(key));
        if self.emitted_error_enums.contains(&name) {
            return None;
        }
        self.emitted_error_enums.insert(name.clone());
        Some(render_error_enum(&name, info))
    }

    fn render_function(
        &self,
        func: &HirFunction,
        owner: Option<String>,
        sig_override: Option<&UnifiedMethodSig>,
        allow_visibility: bool,
        indent: usize,
    ) -> Result<String, CodegenError> {
        let key = FnKey {
            name: func.name.clone(),
            owner,
        };
        let analysis = self.analysis.functions.get(&key);
        let (return_ty, needs_result_wrap, allow_question_mark) =
            self.render_return_type(&func.return_ty, analysis, &key)?;
        let ctx = FunctionCtx {
            analysis,
            needs_result_wrap,
            allow_question_mark,
            expects_value: !matches!(func.return_ty, HirType::Unit),
        };

        let params = func
            .params
            .iter()
            .map(|param| self.render_fn_param_kind(&param.kind, &ctx, sig_override))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");

        let mut out = String::new();
        if func.is_async && key.owner.is_none() && func.name == "main" {
            push_indent(&mut out, indent);
            out.push_str("#[tokio::main]\n");
        }

        push_indent(&mut out, indent);
        if allow_visibility {
            out.push_str(render_visibility(func.visibility));
        }
        if func.is_async {
            out.push_str("async ");
        }
        out.push_str("fn ");
        out.push_str(&func.name);
        out.push_str(&render_generics(&func.generics));
        out.push('(');
        out.push_str(&params);
        out.push(')');
        out.push_str(&return_ty);

        if func.where_clause.is_empty() {
            out.push(' ');
            out.push_str(&self.render_function_block(&func.body, &ctx, indent)?);
        } else {
            out.push('\n');
            push_indent(&mut out, indent);
            out.push_str("where\n");
            for pred in &func.where_clause {
                push_indent(&mut out, indent + 1);
                out.push_str(&render_where_predicate(pred));
                out.push_str(",\n");
            }
            out.push_str(&self.render_function_block(&func.body, &ctx, indent)?);
        }

        Ok(out)
    }

    fn render_fn_param_kind(
        &self,
        param: &HirFnParamKind,
        ctx: &FunctionCtx<'_>,
        sig_override: Option<&UnifiedMethodSig>,
    ) -> Result<String, CodegenError> {
        match param {
            HirFnParamKind::SelfParam => {
                let mut sig = sig_override
                    .and_then(|s| s.self_sig)
                    .or_else(|| ctx.analysis.and_then(|a| a.self_sig))
                    .unwrap_or(SelfSig::Owned);
                if sig == SelfSig::Ref && ctx.is_mutable("self") {
                    sig = SelfSig::Owned;
                }
                let s = match sig {
                    SelfSig::Ref => "&self".to_string(),
                    SelfSig::RefMut => "&mut self".to_string(),
                    SelfSig::Owned => {
                        if ctx.is_mutable("self") {
                            "mut self".to_string()
                        } else {
                            "self".to_string()
                        }
                    }
                };
                Ok(s)
            }
            HirFnParamKind::Typed { name, ty } => {
                let mut sig = sig_override
                    .and_then(|s| s.param_sigs.get(name).copied())
                    .or_else(|| ctx.analysis.and_then(|a| a.param_sigs.get(name).copied()))
                    .unwrap_or(ParamSig::Owned);
                if sig == ParamSig::Ref && ctx.is_mutable(name) {
                    sig = ParamSig::Owned;
                }

                let ty_str = match sig {
                    ParamSig::Ref => render_ref_param_type(ty),
                    ParamSig::RefMut => format!("&mut {}", render_type(ty)),
                    ParamSig::Owned => render_type(ty),
                };

                if sig == ParamSig::Owned && ctx.is_mutable(name) {
                    Ok(format!("mut {name}: {ty_str}"))
                } else {
                    Ok(format!("{name}: {ty_str}"))
                }
            }
        }
    }

    fn render_return_type(
        &self,
        return_ty: &HirType,
        analysis: Option<&FnAnalysis>,
        key: &FnKey,
    ) -> Result<(String, bool, bool), CodegenError> {
        let explicit_result = is_result_type(return_ty);
        let mut needs_result_wrap = false;
        let mut error_ty = None;

        if let Some(info) = analysis.and_then(|a| a.error_info.as_ref()) {
            if info.needs_result_wrap {
                needs_result_wrap = true;
                if info.error_types.is_empty() {
                    return Err(CodegenError::new(format!(
                        "missing error type for fallible function {}",
                        key.name
                    )));
                }
                if info.error_types.len() == 1 {
                    error_ty = Some(render_type(&info.error_types[0].ty));
                } else if let Some(name) = self.resolved_error_enum_names.get(key) {
                    error_ty = Some(name.clone());
                } else if let Some(name) = &info.error_enum_name {
                    error_ty = Some(name.clone());
                } else {
                    error_ty = Some(make_error_enum_name(key));
                }
            }
        }

        let render = if needs_result_wrap {
            let ok_ty = if matches!(return_ty, HirType::Unit) {
                "()".to_string()
            } else {
                render_type(return_ty)
            };
            let err_ty = error_ty.unwrap_or_else(|| "std::io::Error".to_string());
            format!(" -> Result<{ok_ty}, {err_ty}>")
        } else if matches!(return_ty, HirType::Unit) {
            String::new()
        } else {
            format!(" -> {}", render_type(return_ty))
        };

        Ok((
            render,
            needs_result_wrap,
            explicit_result || needs_result_wrap,
        ))
    }

    fn render_function_block(
        &self,
        block: &HirBlock,
        ctx: &FunctionCtx<'_>,
        indent: usize,
    ) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str("{\n");

        let mut stmt_end = block.stmts.len();
        let mut inferred_tail: Option<&HirExpr> = None;
        if block.tail_expr.is_none() && ctx.expects_value {
            if let Some(HirStmt {
                kind: HirStmtKind::Expr(expr),
                ..
            }) = block.stmts.last()
            {
                inferred_tail = Some(expr);
                stmt_end = stmt_end.saturating_sub(1);
            }
        }

        let stmt_slice = &block.stmts[..stmt_end];
        let tail = block.tail_expr.as_deref().or(inferred_tail);

        for (idx, stmt) in stmt_slice.iter().enumerate() {
            let later_stmts = &stmt_slice[idx + 1..];
            out.push_str(&self.render_stmt(
                stmt,
                ctx,
                indent + 1,
                false,
                later_stmts,
                tail,
            )?);
            out.push('\n');
        }

        if let Some(tail) = tail {
            push_indent(&mut out, indent + 1);
            if ctx.needs_result_wrap {
                out.push_str("Ok(");
                out.push_str(&self.render_expr(tail, ctx)?);
                out.push(')');
            } else {
                out.push_str(&self.render_expr(tail, ctx)?);
            }
            out.push('\n');
        } else if ctx.needs_result_wrap {
            push_indent(&mut out, indent + 1);
            out.push_str("Ok(())\n");
        }

        push_indent(&mut out, indent);
        out.push('}');
        Ok(out)
    }

    fn render_stmt(
        &self,
        stmt: &HirStmt,
        ctx: &FunctionCtx<'_>,
        indent: usize,
        compact: bool,
        later_stmts: &[HirStmt],
        later_tail: Option<&HirExpr>,
    ) -> Result<String, CodegenError> {
        let mut out = String::new();
        if !compact {
            push_indent(&mut out, indent);
        }
        match &stmt.kind {
            HirStmtKind::Let { pattern, ty, value } => {
                out.push_str("let ");
                out.push_str(&self.render_pattern(pattern, ctx, true));
                if !matches!(ty, HirType::Unresolved) {
                    out.push_str(": ");
                    out.push_str(&render_type(ty));
                }
                if let Some(v) = value {
                    out.push_str(" = ");
                    out.push_str(&self.render_expr(v, ctx)?);
                }
                out.push(';');
            }
            HirStmtKind::Expr(expr) => {
                out.push_str(&self.render_expr(expr, ctx)?);
                out.push(';');
            }
            HirStmtKind::While { condition, body } => {
                out.push_str("while ");
                out.push_str(&self.render_expr(condition, ctx)?);
                if compact {
                    out.push(' ');
                    out.push_str(&self.render_block_compact(body, ctx)?);
                } else {
                    out.push(' ');
                    out.push_str(&self.render_block_stmt(body, ctx, indent)?);
                }
            }
            HirStmtKind::For {
                pattern,
                iter,
                body,
                ..
            } => {
                out.push_str("for ");
                out.push_str(&self.render_pattern(pattern, ctx, true));
                out.push_str(" in ");
                let iter_code = self.render_expr(iter, ctx)?;
                let iter_code = maybe_borrow_for_iter(iter, &iter_code, ctx, later_stmts, later_tail);
                out.push_str(&iter_code);
                if compact {
                    out.push(' ');
                    out.push_str(&self.render_block_compact(body, ctx)?);
                } else {
                    out.push(' ');
                    out.push_str(&self.render_block_stmt(body, ctx, indent)?);
                }
            }
            HirStmtKind::Loop { body } => {
                out.push_str("loop");
                if compact {
                    out.push(' ');
                    out.push_str(&self.render_block_compact(body, ctx)?);
                } else {
                    out.push(' ');
                    out.push_str(&self.render_block_stmt(body, ctx, indent)?);
                }
            }
            HirStmtKind::Break(expr) => {
                out.push_str("break");
                if let Some(e) = expr {
                    out.push(' ');
                    out.push_str(&self.render_expr(e, ctx)?);
                }
                out.push(';');
            }
            HirStmtKind::Continue => out.push_str("continue;"),
            HirStmtKind::Item(item) => match &item.kind {
                HirItemKind::RustBlock(rb) => {
                    if compact {
                        out.push_str("{ ");
                        out.push_str(&rb.code);
                        out.push_str(" }");
                    } else {
                        out.push_str("{\n");
                        push_indent(&mut out, indent + 1);
                        out.push_str(&rb.code);
                        out.push('\n');
                        push_indent(&mut out, indent);
                        out.push('}');
                    }
                }
                _ => {
                    let nested = self.render_item_no_enums(item)?;
                    if compact {
                        out.push_str(&nested.replace('\n', " "));
                    } else {
                        out.push_str(&nested);
                    }
                }
            },
        }
        Ok(out)
    }

    fn render_block_stmt(
        &self,
        block: &HirBlock,
        ctx: &FunctionCtx<'_>,
        indent: usize,
    ) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str("{\n");
        for (idx, stmt) in block.stmts.iter().enumerate() {
            let later_stmts = &block.stmts[idx + 1..];
            out.push_str(&self.render_stmt(
                stmt,
                ctx,
                indent + 1,
                false,
                later_stmts,
                block.tail_expr.as_deref(),
            )?);
            out.push('\n');
        }
        if let Some(tail) = &block.tail_expr {
            push_indent(&mut out, indent + 1);
            out.push_str(&self.render_expr(tail, ctx)?);
            out.push('\n');
        }
        push_indent(&mut out, indent);
        out.push('}');
        Ok(out)
    }

    fn render_block_compact(
        &self,
        block: &HirBlock,
        ctx: &FunctionCtx<'_>,
    ) -> Result<String, CodegenError> {
        let mut parts = Vec::new();
        for (idx, stmt) in block.stmts.iter().enumerate() {
            let later_stmts = &block.stmts[idx + 1..];
            parts.push(self.render_stmt(
                stmt,
                ctx,
                0,
                true,
                later_stmts,
                block.tail_expr.as_deref(),
            )?);
        }
        if let Some(tail) = &block.tail_expr {
            parts.push(self.render_expr(tail, ctx)?);
        }
        Ok(format!("{{ {} }}", parts.join(" ")))
    }

    fn render_expr(&self, expr: &HirExpr, ctx: &FunctionCtx<'_>) -> Result<String, CodegenError> {
        match &expr.kind {
            HirExprKind::Literal(lit) => Ok(render_expr_literal(lit)),
            HirExprKind::Path(path) => Ok(path.join("::")),
            HirExprKind::BinaryOp { left, op, right } => Ok(format!(
                "({} {} {})",
                self.render_expr(left, ctx)?,
                render_bin_op(*op),
                self.render_expr(right, ctx)?
            )),
            HirExprKind::UnaryOp { op, operand } => Ok(format!(
                "({}{})",
                render_un_op(*op),
                self.render_expr(operand, ctx)?
            )),
            HirExprKind::FnCall { func, args } => {
                let call_site = ctx.call_site(expr.span);
                let is_constructor = is_constructor_callee(func);
                let func_code = self.render_expr(func, ctx)?;
                let rendered_args = args
                    .iter()
                    .enumerate()
                    .map(|(idx, arg)| {
                        let arg_code = self.render_expr(arg, ctx)?;
                        let action = if is_constructor {
                            ArgAction::Move
                        } else {
                            call_site
                                .and_then(|site| site.arg_actions.get(idx))
                                .copied()
                                .unwrap_or(ArgAction::Move)
                        };
                        let action = normalize_arg_action_for_expr(arg, action, ctx);
                        Ok(apply_arg_action(arg_code, action))
                    })
                    .collect::<Result<Vec<_>, CodegenError>>()?;
                let call = format!("{func_code}({})", rendered_args.join(", "));
                if is_constructor {
                    Ok(call)
                } else {
                    Ok(apply_fallible(call, call_site, ctx.allow_question_mark))
                }
            }
            HirExprKind::MacroCall {
                path,
                delimiter,
                tokens,
            } => Ok(format!(
                "{}!{}{}{}",
                path.join("::"),
                macro_open(*delimiter),
                normalize_macro_tokens(tokens),
                macro_close(*delimiter)
            )),
            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                let call_site = ctx.call_site(expr.span);
                let recv_action = call_site
                    .and_then(|site| site.arg_actions.first())
                    .copied()
                    .unwrap_or(ArgAction::Move);
                let receiver_code =
                    apply_method_receiver_action(self.render_expr(receiver, ctx)?, recv_action);
                let rendered_args = args
                    .iter()
                    .enumerate()
                    .map(|(idx, arg)| {
                        let arg_code = self.render_expr(arg, ctx)?;
                        let action = call_site
                            .and_then(|site| site.arg_actions.get(idx + 1))
                            .copied()
                            .unwrap_or(ArgAction::Move);
                        let action = normalize_arg_action_for_expr(arg, action, ctx);
                        Ok(apply_arg_action(arg_code, action))
                    })
                    .collect::<Result<Vec<_>, CodegenError>>()?;
                let call = format!("{receiver_code}.{method}({})", rendered_args.join(", "));
                Ok(apply_fallible(call, call_site, ctx.allow_question_mark))
            }
            HirExprKind::FieldAccess { receiver, field } => {
                Ok(format!("({}).{field}", self.render_expr(receiver, ctx)?))
            }
            HirExprKind::OptionalChain { receiver, access } => {
                let recv = self.render_expr(receiver, ctx)?;
                match access {
                    HirOptionalAccess::Field(name) => Ok(format!("({recv}).map(|v| v.{name})")),
                    HirOptionalAccess::Method { name, args } => {
                        let args = args
                            .iter()
                            .map(|a| self.render_expr(a, ctx))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ");
                        Ok(format!("({recv}).map(|v| v.{name}({args}))"))
                    }
                }
            }
            HirExprKind::NullCoalesce { expr, default } => Ok(format!(
                "({}).unwrap_or_else(|| {})",
                self.render_expr(expr, ctx)?,
                self.render_expr(default, ctx)?
            )),
            HirExprKind::Index { receiver, index } => Ok(format!(
                "({})[{}]",
                self.render_expr(receiver, ctx)?,
                self.render_expr(index, ctx)?
            )),
            HirExprKind::Block(block) => self.render_block_compact(block, ctx),
            HirExprKind::If {
                condition,
                then_block,
                else_expr,
            } => {
                let mut out = format!(
                    "if {} {}",
                    self.render_expr(condition, ctx)?,
                    self.render_block_compact(then_block, ctx)?
                );
                if let Some(e) = else_expr {
                    out.push_str(" else ");
                    out.push_str(&self.render_expr(e, ctx)?);
                }
                Ok(out)
            }
            HirExprKind::Match { expr, arms } => {
                let mut out = String::new();
                out.push_str("match ");
                out.push_str(&self.render_expr(expr, ctx)?);
                out.push_str(" { ");
                for (idx, arm) in arms.iter().enumerate() {
                    if idx > 0 {
                        out.push(' ');
                    }
                    out.push_str(&self.render_match_arm(arm, ctx)?);
                }
                out.push_str(" }");
                Ok(out)
            }
            HirExprKind::Await { expr: awaited_expr } => {
                let inner = self.render_expr(awaited_expr, ctx)?;
                let mut awaited = format!("({inner}).await");
                let site_fallible = ctx
                    .call_site(expr.span)
                    .map(|site| site.is_fallible)
                    .unwrap_or(false);
                if site_fallible {
                    if ctx.allow_question_mark {
                        awaited.push('?');
                    } else {
                        awaited.push_str(".expect(\"fallible call failed\")");
                    }
                }
                Ok(awaited)
            }
            HirExprKind::Assign { target, value } => Ok(format!(
                "{} = {}",
                self.render_expr(target, ctx)?,
                self.render_expr(value, ctx)?
            )),
            HirExprKind::CompoundAssign { target, op, value } => Ok(format!(
                "{} {}= {}",
                self.render_expr(target, ctx)?,
                render_compound_op(*op),
                self.render_expr(value, ctx)?
            )),
            HirExprKind::StructLiteral { path, fields } => {
                let mut out = String::new();
                out.push_str(&path.join("::"));
                out.push_str(" { ");
                for (idx, field) in fields.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&field.name);
                    if let Some(value) = &field.value {
                        out.push_str(": ");
                        out.push_str(&self.render_expr(value, ctx)?);
                    }
                }
                out.push_str(" }");
                Ok(out)
            }
            HirExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let start = start
                    .as_ref()
                    .map(|e| self.render_expr(e, ctx))
                    .transpose()?
                    .unwrap_or_default();
                let end = end
                    .as_ref()
                    .map(|e| self.render_expr(e, ctx))
                    .transpose()?
                    .unwrap_or_default();
                let dots = if *inclusive { "..=" } else { ".." };
                Ok(format!("{start}{dots}{end}"))
            }
            HirExprKind::Closure {
                params,
                return_ty,
                body,
            } => {
                let params = params
                    .iter()
                    .map(|p| {
                        if matches!(p.ty, HirType::Unresolved) {
                            p.name.clone()
                        } else {
                            format!("{}: {}", p.name, render_type(&p.ty))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let body_code = self.render_expr(body, ctx)?;
                if matches!(return_ty, HirType::Unresolved) {
                    Ok(format!("|{params}| {body_code}"))
                } else {
                    Ok(format!(
                        "|{params}| -> {} {{ {body_code} }}",
                        render_type(return_ty)
                    ))
                }
            }
            HirExprKind::Return(expr) => {
                if ctx.needs_result_wrap {
                    if let Some(e) = expr {
                        Ok(format!("return Ok({})", self.render_expr(e, ctx)?))
                    } else {
                        Ok("return Ok(())".to_string())
                    }
                } else if let Some(e) = expr {
                    Ok(format!("return {}", self.render_expr(e, ctx)?))
                } else {
                    Ok("return".to_string())
                }
            }
            HirExprKind::Tuple(elems) => {
                if elems.is_empty() {
                    Ok("()".to_string())
                } else if elems.len() == 1 {
                    Ok(format!("({},)", self.render_expr(&elems[0], ctx)?))
                } else {
                    Ok(format!(
                        "({})",
                        elems
                            .iter()
                            .map(|e| self.render_expr(e, ctx))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ")
                    ))
                }
            }
        }
    }

    fn render_match_arm(
        &self,
        arm: &HirMatchArm,
        ctx: &FunctionCtx<'_>,
    ) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str(&self.render_pattern(&arm.pattern, ctx, false));
        if let Some(guard) = &arm.guard {
            out.push_str(" if ");
            out.push_str(&self.render_expr(guard, ctx)?);
        }
        out.push_str(" => ");
        out.push_str(&self.render_expr(&arm.body, ctx)?);
        out.push(',');
        Ok(out)
    }

    fn render_pattern(&self, pat: &HirPattern, ctx: &FunctionCtx<'_>, allow_mut: bool) -> String {
        match &pat.kind {
            HirPatternKind::Wildcard => "_".to_string(),
            HirPatternKind::Ident(name) => {
                if allow_mut && ctx.is_mutable(name) {
                    format!("mut {name}")
                } else {
                    name.clone()
                }
            }
            HirPatternKind::Literal(lit) => render_pattern_literal(lit),
            HirPatternKind::Tuple(items) => {
                if items.is_empty() {
                    "()".to_string()
                } else if items.len() == 1 {
                    format!("({},)", self.render_pattern(&items[0], ctx, allow_mut))
                } else {
                    format!(
                        "({})",
                        items
                            .iter()
                            .map(|p| self.render_pattern(p, ctx, allow_mut))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            HirPatternKind::Struct { path, fields } => {
                format!(
                    "{} {{ {} }}",
                    path.join("::"),
                    fields
                        .iter()
                        .map(|f| self.render_field_pattern(f, ctx, allow_mut))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            HirPatternKind::TupleStruct { path, fields } => format!(
                "{}({})",
                path.join("::"),
                fields
                    .iter()
                    .map(|p| self.render_pattern(p, ctx, allow_mut))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            HirPatternKind::Path(path) => path.join("::"),
            HirPatternKind::Rest => "..".to_string(),
        }
    }

    fn render_field_pattern(
        &self,
        field: &HirFieldPattern,
        ctx: &FunctionCtx<'_>,
        allow_mut: bool,
    ) -> String {
        if let Some(pattern) = &field.pattern {
            format!(
                "{}: {}",
                field.name,
                self.render_pattern(pattern, ctx, allow_mut)
            )
        } else if allow_mut && ctx.is_mutable(&field.name) {
            format!("{}: mut {}", field.name, field.name)
        } else {
            field.name.clone()
        }
    }
}

fn render_visibility(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "",
        Visibility::Public => "pub ",
    }
}

fn render_generics(generics: &[HirGenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }
    let rendered = generics
        .iter()
        .map(|g| {
            if g.bounds.is_empty() {
                g.name.clone()
            } else {
                format!(
                    "{}: {}",
                    g.name,
                    g.bounds
                        .iter()
                        .map(render_type)
                        .collect::<Vec<_>>()
                        .join(" + ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{rendered}>")
}

fn render_where_predicate(pred: &HirWherePredicate) -> String {
    format!(
        "{}: {}",
        render_type(&pred.ty),
        pred.bounds
            .iter()
            .map(render_type)
            .collect::<Vec<_>>()
            .join(" + ")
    )
}

fn render_use_tree(tree: &HirUseTree) -> String {
    match tree {
        HirUseTree::Simple { path, alias } => {
            if let Some(alias) = alias {
                format!("{} as {}", path.join("::"), alias)
            } else {
                path.join("::")
            }
        }
        HirUseTree::Glob { path } => format!("{}::*", path.join("::")),
        HirUseTree::Nested { path, items } => format!(
            "{}::{{{}}}",
            path.join("::"),
            items
                .iter()
                .map(render_use_tree)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn render_type(ty: &HirType) -> String {
    match ty {
        HirType::Primitive(p) => render_primitive(*p).to_string(),
        HirType::Named { path, generics } => {
            if generics.is_empty() {
                path.join("::")
            } else {
                format!(
                    "{}<{}>",
                    path.join("::"),
                    generics
                        .iter()
                        .map(render_type)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
        HirType::Option(inner) => format!("Option<{}>", render_type(inner)),
        HirType::Tuple(items) => {
            if items.is_empty() {
                "()".to_string()
            } else if items.len() == 1 {
                format!("({},)", render_type(&items[0]))
            } else {
                format!(
                    "({})",
                    items.iter().map(render_type).collect::<Vec<_>>().join(", ")
                )
            }
        }
        HirType::Array { element, size } => format!("[{}; {}]", render_type(element), size),
        HirType::Slice(inner) => format!("[{}]", render_type(inner)),
        HirType::Unit => "()".to_string(),
        HirType::Unresolved => "_".to_string(),
    }
}

fn render_ref_param_type(ty: &HirType) -> String {
    match ty {
        HirType::Named { path, .. } if path.last().map(|s| s == "String").unwrap_or(false) => {
            "&str".to_string()
        }
        _ => format!("&{}", render_type(ty)),
    }
}

fn render_primitive(primitive: PrimitiveType) -> &'static str {
    match primitive {
        PrimitiveType::I8 => "i8",
        PrimitiveType::I16 => "i16",
        PrimitiveType::I32 => "i32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::I128 => "i128",
        PrimitiveType::U8 => "u8",
        PrimitiveType::U16 => "u16",
        PrimitiveType::U32 => "u32",
        PrimitiveType::U64 => "u64",
        PrimitiveType::U128 => "u128",
        PrimitiveType::F32 => "f32",
        PrimitiveType::F64 => "f64",
        PrimitiveType::Bool => "bool",
        PrimitiveType::Char => "char",
        PrimitiveType::Usize => "usize",
        PrimitiveType::Isize => "isize",
    }
}

fn render_expr_literal(lit: &Literal) -> String {
    match lit {
        Literal::Int(v) => v.clone(),
        Literal::Float(v) => v.clone(),
        Literal::String(v) => format!("String::from({v:?})"),
        Literal::Char(v) => format!("{v:?}"),
        Literal::Bool(v) => v.to_string(),
    }
}

fn render_pattern_literal(lit: &Literal) -> String {
    match lit {
        Literal::Int(v) => v.clone(),
        Literal::Float(v) => v.clone(),
        Literal::String(v) => format!("{v:?}"),
        Literal::Char(v) => format!("{v:?}"),
        Literal::Bool(v) => v.to_string(),
    }
}

fn render_bin_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
    }
}

fn render_un_op(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "!",
        UnOp::BitNot => "~",
    }
}

fn render_compound_op(op: CompoundOp) -> &'static str {
    match op {
        CompoundOp::Add => "+",
        CompoundOp::Sub => "-",
        CompoundOp::Mul => "*",
        CompoundOp::Div => "/",
        CompoundOp::Rem => "%",
        CompoundOp::BitAnd => "&",
        CompoundOp::BitOr => "|",
        CompoundOp::BitXor => "^",
        CompoundOp::Shl => "<<",
        CompoundOp::Shr => ">>",
    }
}

fn macro_open(d: MacroDelimiter) -> &'static str {
    match d {
        MacroDelimiter::Paren => "(",
        MacroDelimiter::Bracket => "[",
        MacroDelimiter::Brace => "{",
    }
}

fn macro_close(d: MacroDelimiter) -> &'static str {
    match d {
        MacroDelimiter::Paren => ")",
        MacroDelimiter::Bracket => "]",
        MacroDelimiter::Brace => "}",
    }
}

fn normalize_macro_tokens(tokens: &str) -> String {
    let chars: Vec<char> = tokens.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut in_string = false;
    let mut in_char = false;
    let mut escape = false;

    while i < chars.len() {
        let c = chars[i];

        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if in_char {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '\'' {
            in_char = true;
            out.push(c);
            i += 1;
            continue;
        }

        if c.is_whitespace() {
            let prev = out.chars().rev().find(|ch| !ch.is_whitespace());
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let next = if j < chars.len() { Some(chars[j]) } else { None };

            let strip = is_tight_left(prev) || is_tight_right(next);
            if !strip && !out.ends_with(' ') && !out.is_empty() && next.is_some() {
                out.push(' ');
            }
            i = j;
            continue;
        }

        out.push(c);
        i += 1;
    }

    out.trim().to_string()
}

fn is_tight_left(c: Option<char>) -> bool {
    c.map(|ch| matches!(ch, '.' | ':' | '(' | '[' | '{' | '!' | '&'))
        .unwrap_or(false)
}

fn is_tight_right(c: Option<char>) -> bool {
    c.map(|ch| matches!(ch, '.' | ',' | ';' | ':' | ')' | ']' | '}' | '(' | '[' | '{' | '!'))
        .unwrap_or(false)
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str(INDENT);
    }
}

fn apply_arg_action(expr: String, action: ArgAction) -> String {
    match action {
        ArgAction::Move => expr,
        ArgAction::Clone => format!("({expr}).clone()"),
        ArgAction::Borrow => format!("&({expr})"),
        ArgAction::BorrowMut => format!("&mut ({expr})"),
    }
}

fn apply_method_receiver_action(expr: String, action: ArgAction) -> String {
    match action {
        // Rust method syntax auto-borrows receiver for &self/&mut self methods.
        ArgAction::Borrow | ArgAction::BorrowMut | ArgAction::Move => expr,
        ArgAction::Clone => format!("({expr}).clone()"),
    }
}

fn normalize_arg_action_for_expr(expr: &HirExpr, action: ArgAction, ctx: &FunctionCtx<'_>) -> ArgAction {
    match action {
        ArgAction::Borrow | ArgAction::BorrowMut if expr_is_reference_like(expr, ctx) => {
            ArgAction::Move
        }
        _ => action,
    }
}

fn expr_is_reference_like(expr: &HirExpr, ctx: &FunctionCtx<'_>) -> bool {
    let Some(name) = single_path_name(expr) else {
        return false;
    };
    if name == "self" {
        return ctx
            .analysis
            .and_then(|a| a.self_sig)
            .map(|sig| matches!(sig, SelfSig::Ref | SelfSig::RefMut))
            .unwrap_or(false);
    }
    ctx.analysis
        .and_then(|a| a.param_sigs.get(name))
        .map(|sig| matches!(sig, ParamSig::Ref | ParamSig::RefMut))
        .unwrap_or(false)
}

fn maybe_borrow_for_iter(
    iter: &HirExpr,
    iter_code: &str,
    ctx: &FunctionCtx<'_>,
    later_stmts: &[HirStmt],
    later_tail: Option<&HirExpr>,
) -> String {
    let Some(name) = single_path_name(iter) else {
        return iter_code.to_string();
    };
    let used_after = later_stmts.iter().any(|s| stmt_uses_name(s, name))
        || later_tail.map(|e| expr_uses_name(e, name)).unwrap_or(false);
    if !used_after {
        return iter_code.to_string();
    }
    if expr_is_reference_like(iter, ctx) {
        return iter_code.to_string();
    }
    format!("&({iter_code})")
}

fn single_path_name(expr: &HirExpr) -> Option<&str> {
    match &expr.kind {
        HirExprKind::Path(path) if path.len() == 1 => Some(path[0].as_str()),
        _ => None,
    }
}

fn stmt_uses_name(stmt: &HirStmt, name: &str) -> bool {
    match &stmt.kind {
        HirStmtKind::Let { value, .. } => value.as_ref().map(|e| expr_uses_name(e, name)).unwrap_or(false),
        HirStmtKind::Expr(expr) => expr_uses_name(expr, name),
        HirStmtKind::While { condition, body } => {
            expr_uses_name(condition, name) || block_uses_name(body, name)
        }
        HirStmtKind::For { iter, body, .. } => expr_uses_name(iter, name) || block_uses_name(body, name),
        HirStmtKind::Loop { body } => block_uses_name(body, name),
        HirStmtKind::Break(expr) => expr.as_ref().map(|e| expr_uses_name(e, name)).unwrap_or(false),
        HirStmtKind::Continue => false,
        HirStmtKind::Item(item) => match &item.kind {
            HirItemKind::Function(_)
            | HirItemKind::Struct(_)
            | HirItemKind::Enum(_)
            | HirItemKind::Trait(_)
            | HirItemKind::Impl(_)
            | HirItemKind::Use(_) => false,
            HirItemKind::RustBlock(rb) => rb.code.contains(name),
        },
    }
}

fn block_uses_name(block: &HirBlock, name: &str) -> bool {
    block.stmts.iter().any(|s| stmt_uses_name(s, name))
        || block
            .tail_expr
            .as_ref()
            .map(|e| expr_uses_name(e, name))
            .unwrap_or(false)
}

fn expr_uses_name(expr: &HirExpr, name: &str) -> bool {
    match &expr.kind {
        HirExprKind::Literal(_) => false,
        HirExprKind::Path(path) => path.len() == 1 && path[0] == name,
        HirExprKind::BinaryOp { left, right, .. } => {
            expr_uses_name(left, name) || expr_uses_name(right, name)
        }
        HirExprKind::UnaryOp { operand, .. } => expr_uses_name(operand, name),
        HirExprKind::FnCall { func, args } => {
            expr_uses_name(func, name) || args.iter().any(|a| expr_uses_name(a, name))
        }
        HirExprKind::MacroCall { tokens, .. } => tokens.contains(name),
        HirExprKind::MethodCall { receiver, args, .. } => {
            expr_uses_name(receiver, name) || args.iter().any(|a| expr_uses_name(a, name))
        }
        HirExprKind::FieldAccess { receiver, .. } => expr_uses_name(receiver, name),
        HirExprKind::OptionalChain { receiver, access } => {
            expr_uses_name(receiver, name)
                || match access {
                    HirOptionalAccess::Field(_) => false,
                    HirOptionalAccess::Method { args, .. } => {
                        args.iter().any(|a| expr_uses_name(a, name))
                    }
                }
        }
        HirExprKind::NullCoalesce { expr, default } => {
            expr_uses_name(expr, name) || expr_uses_name(default, name)
        }
        HirExprKind::Index { receiver, index } => {
            expr_uses_name(receiver, name) || expr_uses_name(index, name)
        }
        HirExprKind::Block(block) => block_uses_name(block, name),
        HirExprKind::If {
            condition,
            then_block,
            else_expr,
        } => {
            expr_uses_name(condition, name)
                || block_uses_name(then_block, name)
                || else_expr
                    .as_ref()
                    .map(|e| expr_uses_name(e, name))
                    .unwrap_or(false)
        }
        HirExprKind::Match { expr, arms } => {
            expr_uses_name(expr, name)
                || arms.iter().any(|arm| {
                    arm.guard
                        .as_ref()
                        .map(|g| expr_uses_name(g, name))
                        .unwrap_or(false)
                        || expr_uses_name(&arm.body, name)
                })
        }
        HirExprKind::Await { expr } => expr_uses_name(expr, name),
        HirExprKind::Assign { target, value } => {
            expr_uses_name(target, name) || expr_uses_name(value, name)
        }
        HirExprKind::CompoundAssign { target, value, .. } => {
            expr_uses_name(target, name) || expr_uses_name(value, name)
        }
        HirExprKind::StructLiteral { fields, .. } => fields
            .iter()
            .filter_map(|f| f.value.as_ref())
            .any(|v| expr_uses_name(v, name)),
        HirExprKind::Range { start, end, .. } => {
            start.as_ref().map(|e| expr_uses_name(e, name)).unwrap_or(false)
                || end.as_ref().map(|e| expr_uses_name(e, name)).unwrap_or(false)
        }
        HirExprKind::Closure { body, .. } => expr_uses_name(body, name),
        HirExprKind::Return(expr) => expr.as_ref().map(|e| expr_uses_name(e, name)).unwrap_or(false),
        HirExprKind::Tuple(items) => items.iter().any(|e| expr_uses_name(e, name)),
    }
}

fn apply_fallible(call: String, site: Option<&CallSiteInfo>, allow_question_mark: bool) -> String {
    let is_fallible = site.map(|s| s.is_fallible).unwrap_or(false);
    if !is_fallible {
        return call;
    }
    if allow_question_mark {
        format!("{call}?")
    } else {
        format!("{call}.expect(\"fallible call failed\")")
    }
}

fn is_constructor_callee(func: &HirExpr) -> bool {
    match &func.kind {
        HirExprKind::Path(path) if path.len() == 1 => path[0]
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false),
        _ => false,
    }
}

fn is_result_type(ty: &HirType) -> bool {
    matches!(ty, HirType::Named { path, .. } if path.last().map(|s| s == "Result").unwrap_or(false))
}

fn impl_owner(target: &HirType) -> Option<String> {
    match target {
        HirType::Named { path, .. } if !path.is_empty() => Some(path.join("::")),
        _ => None,
    }
}

fn resolve_error_enum_names(analysis: &AnalysisResult) -> HashMap<FnKey, String> {
    let mut keys: Vec<_> = analysis.functions.keys().cloned().collect();
    keys.sort_by(|a, b| {
        let a_owner = a.owner.as_deref().unwrap_or("");
        let b_owner = b.owner.as_deref().unwrap_or("");
        (a_owner, a.name.as_str()).cmp(&(b_owner, b.name.as_str()))
    });

    let mut used = HashSet::new();
    let mut resolved = HashMap::new();

    for key in keys {
        let Some(info) = analysis
            .functions
            .get(&key)
            .and_then(|a| a.error_info.as_ref())
        else {
            continue;
        };
        if !info.needs_result_wrap || info.error_types.len() < 2 {
            continue;
        }

        let base = info
            .error_enum_name
            .clone()
            .unwrap_or_else(|| make_error_enum_name(&key));

        let mut name = base.clone();
        let mut idx = 2;
        while used.contains(&name) {
            name = format!("{base}{idx}");
            idx += 1;
        }

        used.insert(name.clone());
        resolved.insert(key, name);
    }

    resolved
}

fn make_error_enum_name(key: &FnKey) -> String {
    if let Some(owner) = &key.owner {
        format!("{}{}Error", to_pascal(owner), to_pascal(&key.name))
    } else {
        format!("{}Error", to_pascal(&key.name))
    }
}

fn render_error_enum(name: &str, info: &nanachi_analyzer::ErrorInfo) -> String {
    let mut variant_names = HashSet::new();
    let mut variants = Vec::new();

    for err in &info.error_types {
        let mut variant = error_variant_name(&err.ty);
        let base = variant.clone();
        let mut idx = 2;
        while variant_names.contains(&variant) {
            variant = format!("{base}{idx}");
            idx += 1;
        }
        variant_names.insert(variant.clone());
        variants.push((variant, render_type(&err.ty)));
    }

    let mut out = String::new();
    out.push_str("#[derive(Debug)]\n");
    out.push_str("pub enum ");
    out.push_str(name);
    out.push_str(" {\n");
    for (variant, ty) in &variants {
        push_indent(&mut out, 1);
        out.push_str(variant);
        out.push('(');
        out.push_str(ty);
        out.push_str("),\n");
    }
    out.push_str("}\n\n");

    out.push_str("impl std::fmt::Display for ");
    out.push_str(name);
    out.push_str(" {\n");
    push_indent(&mut out, 1);
    out.push_str("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n");
    push_indent(&mut out, 2);
    out.push_str("write!(f, \"{self:?}\")\n");
    push_indent(&mut out, 1);
    out.push_str("}\n");
    out.push_str("}\n\n");

    out.push_str("impl std::error::Error for ");
    out.push_str(name);
    out.push_str(" {}\n\n");

    for (variant, ty) in &variants {
        out.push_str("impl From<");
        out.push_str(ty);
        out.push_str("> for ");
        out.push_str(name);
        out.push_str(" {\n");
        push_indent(&mut out, 1);
        out.push_str("fn from(value: ");
        out.push_str(ty);
        out.push_str(") -> Self {\n");
        push_indent(&mut out, 2);
        out.push_str("Self::");
        out.push_str(variant);
        out.push_str("(value)\n");
        push_indent(&mut out, 1);
        out.push_str("}\n");
        out.push_str("}\n\n");
    }

    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn error_variant_name(ty: &HirType) -> String {
    match ty {
        HirType::Primitive(p) => to_pascal(render_primitive(*p)),
        HirType::Named { path, .. } => {
            if path.is_empty() {
                "Error".to_string()
            } else if path.last().map(|s| s == "Error").unwrap_or(false) && path.len() >= 2 {
                to_pascal(&format!(
                    "{}_{}",
                    path[path.len() - 2],
                    path[path.len() - 1]
                ))
            } else {
                to_pascal(path.last().expect("non-empty path"))
            }
        }
        HirType::Option(_) => "OptionError".to_string(),
        HirType::Tuple(_) => "TupleError".to_string(),
        HirType::Array { .. } => "ArrayError".to_string(),
        HirType::Slice(_) => "SliceError".to_string(),
        HirType::Unit => "UnitError".to_string(),
        HirType::Unresolved => "UnknownError".to_string(),
    }
}

fn to_pascal(s: &str) -> String {
    let mut out = String::new();
    for part in s
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|p| !p.is_empty())
    {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() { "X".to_string() } else { out }
}

fn collect_trait_method_sigs(
    hir: &HirProgram,
    analysis: &AnalysisResult,
) -> HashMap<(String, String), UnifiedMethodSig> {
    let mut trait_impl_owners: HashMap<String, Vec<String>> = HashMap::new();
    for item in &hir.items {
        if let HirItemKind::Impl(imp) = &item.kind {
            let Some(trait_name) = &imp.trait_name else {
                continue;
            };
            let Some(owner) = impl_owner(&imp.target) else {
                continue;
            };
            let trait_name = trait_name.last().cloned().unwrap_or_default();
            trait_impl_owners.entry(trait_name).or_default().push(owner);
        }
    }

    let mut result = HashMap::new();
    for item in &hir.items {
        let HirItemKind::Trait(tr) = &item.kind else {
            continue;
        };
        for method in &tr.methods {
            let mut merged = UnifiedMethodSig::default();
            if let Some(owners) = trait_impl_owners.get(&tr.name) {
                for owner in owners {
                    let key = FnKey {
                        name: method.name.clone(),
                        owner: Some(owner.clone()),
                    };
                    if let Some(a) = analysis.functions.get(&key) {
                        if let Some(sig) = a.self_sig {
                            merged.self_sig = Some(match merged.self_sig {
                                Some(prev) if prev > sig => prev,
                                _ => sig,
                            });
                        }
                        for (name, sig) in &a.param_sigs {
                            let entry = merged
                                .param_sigs
                                .entry(name.clone())
                                .or_insert(ParamSig::Ref);
                            if *sig > *entry {
                                *entry = *sig;
                            }
                        }
                    }
                }
            }

            if merged.self_sig.is_none() && merged.param_sigs.is_empty() {
                let key = FnKey {
                    name: method.name.clone(),
                    owner: Some(format!("trait::{}", tr.name)),
                };
                if let Some(a) = analysis.functions.get(&key) {
                    merged.self_sig = a.self_sig;
                    merged.param_sigs = a.param_sigs.clone();
                }
            }

            if merged.self_sig.is_some() || !merged.param_sigs.is_empty() {
                result.insert((tr.name.clone(), method.name.clone()), merged);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_src(src: &str) -> String {
        let tokens = nanachi_lexer::lex(src).expect("lex");
        let ast = nanachi_parser::parse(&tokens).expect("parse");
        let hir = nanachi_hir::lower(&ast).expect("lower");
        let mir = nanachi_mir::build(&hir).expect("build mir");
        let analysis = nanachi_analyzer::analyze(&mir, &hir);
        generate(&hir, &analysis).expect("generate")
    }

    #[test]
    fn mutability_and_param_sig_codegen() {
        let rs = generate_src(
            r#"fn f(x: i32) {
                x = 5;
            }"#,
        );
        assert!(rs.contains("fn f(mut x: i32)"));
    }

    #[test]
    fn method_self_sig_codegen() {
        let rs = generate_src(
            r#"struct User { age: i32 }
            impl User {
                fn grow(self) {
                    self.age = self.age + 1;
                }
            }"#,
        );
        assert!(rs.contains("fn grow(&mut self)"));
    }

    #[test]
    fn error_wrap_and_question_mark() {
        let rs = generate_src(
            r#"use std::fs;
            fn read_config(path: String) -> String {
                fs::read_to_string(path)
            }"#,
        );
        assert!(rs.contains("fn read_config(path: &str) -> Result<String, std::io::Error>"));
        assert!(rs.contains("fs::read_to_string(path)?"));
        assert!(!rs.contains("fs::read_to_string(&(path))?"));
    }

    #[test]
    fn async_main_gets_tokio_attr() {
        let rs = generate_src(
            r#"async fn main() {
                println!("hello");
            }"#,
        );
        assert!(rs.contains("#[tokio::main]"));
    }

    #[test]
    fn optional_and_null_codegen() {
        let rs = generate_src(
            r#"fn f(u: User?) -> String {
                u?.name ?? "anon"
            }"#,
        );
        assert!(rs.contains(".map(|v| v.name)"));
        assert!(rs.contains(".unwrap_or_else(||"));
    }

    #[test]
    fn main_fallible_uses_expect_without_wrap() {
        let rs = generate_src(
            r#"use std::fs;
            fn main() {
                let content: String = fs::read_to_string("config.toml");
                println!("{}", content);
            }"#,
        );
        assert!(rs.contains("fn main()"));
        assert!(!rs.contains("fn main() -> Result"));
        assert!(rs.contains(".expect(\"fallible call failed\")"));
    }

    #[test]
    fn trait_method_sig_follows_impl_analysis() {
        let rs = generate_src(
            r#"trait Printable {
                fn to_string(self) -> String;
            }
            struct User { name: String }
            impl Printable for User {
                fn to_string(self) -> String {
                    format!("{}", self.name)
                }
            }"#,
        );
        assert!(rs.contains("trait Printable"));
        assert!(rs.contains("fn to_string(&self) -> String;"));
        assert!(rs.contains("fn to_string(&self) -> String"));
    }

    #[test]
    fn method_receiver_borrow_does_not_emit_prefix_ref() {
        let rs = generate_src(
            r#"fn push_name(list: Vec<String>, name: String) {
                list.push(name);
            }"#,
        );
        assert!(rs.contains("list.push(name);"));
        assert!(!rs.contains("&mut (list).push"));
        assert!(!rs.contains("&(list).push"));
    }

    #[test]
    fn for_loop_borrows_iter_when_used_after() {
        let rs = generate_src(
            r#"fn control_flow() {
                let numbers: Vec<i32> = vec![1, 2, 3];
                for n: i32 in numbers {
                    println!("{}", n);
                }
                println!("len: {}", numbers.len());
            }"#,
        );
        assert!(rs.contains("for n in &(numbers)"));
        assert!(rs.contains("numbers.len()") || rs.contains("numbers . len"));
    }

    #[test]
    fn macro_tokens_are_compacted() {
        let rs = generate_src(
            r#"fn greet(name: String) {
                println!("Hello, {}", name);
            }"#,
        );
        assert!(rs.contains("println!(\"Hello, {}\", name);"));
    }

    #[test]
    fn reqwest_get_text_chain_uses_expect_in_non_result_fn() {
        let rs = generate_src(
            r#"async fn fetch(url: String) -> String {
                let response: String = reqwest::get(url).await.text().await;
                response
            }"#,
        );
        assert!(rs.contains(".await.expect(\"fallible call failed\")"));
    }
}
