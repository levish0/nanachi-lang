use std::collections::HashMap;
use std::fmt;

use nanachi_ast::expr::{BinOp, CompoundOp, Literal};
use nanachi_hir::{
    HirBlock, HirExpr, HirExprKind, HirFnParam, HirFnParamKind, HirFunction, HirItem, HirItemKind,
    HirPattern, HirPatternKind, HirProgram, HirStmt, HirStmtKind, HirTrait, HirTraitMethod,
    HirType,
};
use nanachi_lexer::Span;

use crate::mir::{
    AggregateKind, BasicBlock, BlockId, Local, LocalDecl, LocalKind, MirBody, MirConstant,
    MirProgram, Operand, Place, Rvalue, Statement, StatementKind, SwitchTarget, Terminator,
    TerminatorKind,
};

#[derive(Debug, Clone, PartialEq)]
pub struct MirError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for MirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mir build error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for MirError {}

#[derive(Debug, Default, Clone)]
struct WorkBlock {
    statements: Vec<Statement>,
    terminator: Option<Terminator>,
}

#[derive(Debug, Clone, Copy)]
struct LoopCtx {
    continue_bb: BlockId,
    break_bb: BlockId,
}

pub fn build(hir: &HirProgram) -> Result<MirProgram, MirError> {
    let mut bodies = Vec::new();

    for item in &hir.items {
        match &item.kind {
            HirItemKind::Function(func) => bodies.push(build_function(func, None)?),
            HirItemKind::Impl(imp) => {
                let owner = impl_owner(&imp.target);
                for method in &imp.methods {
                    bodies.push(build_function(method, owner.clone())?);
                }
            }
            HirItemKind::Trait(tr) => {
                for method in &tr.methods {
                    if let Some(default_body) = &method.default_body {
                        bodies.push(build_trait_default_body(tr, method, default_body)?);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(MirProgram { bodies })
}

fn impl_owner(target: &HirType) -> Option<String> {
    match target {
        HirType::Named { path, .. } if !path.is_empty() => Some(path.join("::")),
        _ => None,
    }
}

fn build_trait_default_body(
    tr: &HirTrait,
    method: &HirTraitMethod,
    body: &HirBlock,
) -> Result<MirBody, MirError> {
    let owner = Some(format!("trait::{}", tr.name));
    let mut builder = MirBuilder::new(
        &method.name,
        owner,
        false,
        &method.params,
        method.return_ty.clone(),
        method.span,
    );
    builder.lower_function_body(body)?;
    Ok(builder.finish())
}

fn build_function(func: &HirFunction, owner: Option<String>) -> Result<MirBody, MirError> {
    let mut builder = MirBuilder::new(
        &func.name,
        owner,
        func.is_async,
        &func.params,
        func.return_ty.clone(),
        func.span,
    );
    builder.lower_function_body(&func.body)?;
    Ok(builder.finish())
}

struct MirBuilder {
    name: String,
    owner: Option<String>,
    is_async: bool,
    return_ty: HirType,
    span: Span,

    locals: Vec<LocalDecl>,
    blocks: Vec<WorkBlock>,
    current_block: BlockId,
    next_local: u32,
    next_temp: u32,
    arg_count: usize,

    scopes: Vec<HashMap<String, Local>>,
    loop_stack: Vec<LoopCtx>,
}

impl MirBuilder {
    fn new(
        name: &str,
        owner: Option<String>,
        is_async: bool,
        params: &[HirFnParam],
        return_ty: HirType,
        span: Span,
    ) -> Self {
        let mut this = Self {
            name: name.to_string(),
            owner,
            is_async,
            return_ty: return_ty.clone(),
            span,
            locals: Vec::new(),
            blocks: vec![WorkBlock::default()],
            current_block: BlockId(0),
            next_local: 0,
            next_temp: 0,
            arg_count: 0,
            scopes: vec![HashMap::new()],
            loop_stack: Vec::new(),
        };

        let ret_local =
            this.new_local_decl("_0".to_string(), return_ty, LocalKind::ReturnPlace, span);
        debug_assert_eq!(ret_local.0, 0);

        for param in params {
            match &param.kind {
                HirFnParamKind::SelfParam => {
                    let local = this.new_local_decl(
                        "self".to_string(),
                        HirType::Unresolved,
                        LocalKind::SelfParam,
                        param.span,
                    );
                    this.define_local("self".to_string(), local);
                    this.arg_count += 1;
                }
                HirFnParamKind::Typed { name, ty } => {
                    let local =
                        this.new_local_decl(name.clone(), ty.clone(), LocalKind::Param, param.span);
                    this.define_local(name.clone(), local);
                    this.arg_count += 1;
                }
            }
        }

        this
    }

    fn finish(self) -> MirBody {
        let default_term = Terminator {
            kind: TerminatorKind::Unreachable,
            span: self.span,
        };
        let blocks = self
            .blocks
            .into_iter()
            .map(|b| BasicBlock {
                statements: b.statements,
                terminator: b.terminator.unwrap_or_else(|| default_term.clone()),
            })
            .collect();

        MirBody {
            name: self.name,
            owner: self.owner,
            is_async: self.is_async,
            locals: self.locals,
            blocks,
            arg_count: self.arg_count,
            return_ty: self.return_ty,
            span: self.span,
        }
    }

    fn lower_function_body(&mut self, body: &HirBlock) -> Result<(), MirError> {
        self.push_scope();
        for stmt in &body.stmts {
            self.lower_stmt(stmt)?;
        }

        if !self.is_current_terminated() {
            if let Some(tail) = &body.tail_expr {
                let value = self.lower_expr(tail)?;
                self.emit_assign(Place::from_local(Local(0)), Rvalue::Use(value), tail.span);
            }
            self.terminate(TerminatorKind::Return, body.span);
        }
        self.pop_scope();
        Ok(())
    }

    fn lower_stmt(&mut self, stmt: &HirStmt) -> Result<(), MirError> {
        match &stmt.kind {
            HirStmtKind::Let { pattern, ty, value } => {
                let value_op = if let Some(expr) = value {
                    Some(self.lower_expr(expr)?)
                } else {
                    None
                };
                self.bind_let_pattern(pattern, ty, value_op, stmt.span);
                Ok(())
            }
            HirStmtKind::Expr(expr) => {
                self.lower_expr(expr)?;
                Ok(())
            }
            HirStmtKind::While { condition, body } => self.lower_while(condition, body, stmt.span),
            HirStmtKind::For {
                pattern,
                iter_ty,
                iter,
                body,
            } => self.lower_for(pattern, iter_ty, iter, body, stmt.span),
            HirStmtKind::Loop { body } => self.lower_loop(body, stmt.span),
            HirStmtKind::Break(value) => {
                if let Some(expr) = value {
                    let _ = self.lower_expr(expr)?;
                }
                let ctx = self.loop_stack.last().copied().ok_or_else(|| MirError {
                    span: stmt.span,
                    message: "`break` outside loop".to_string(),
                })?;
                self.terminate(TerminatorKind::Goto(ctx.break_bb), stmt.span);
                self.current_block = self.new_block();
                Ok(())
            }
            HirStmtKind::Continue => {
                let ctx = self.loop_stack.last().copied().ok_or_else(|| MirError {
                    span: stmt.span,
                    message: "`continue` outside loop".to_string(),
                })?;
                self.terminate(TerminatorKind::Goto(ctx.continue_bb), stmt.span);
                self.current_block = self.new_block();
                Ok(())
            }
            HirStmtKind::Item(item) => {
                self.lower_nested_item(item, stmt.span);
                Ok(())
            }
        }
    }

    fn lower_nested_item(&mut self, item: &HirItem, span: Span) {
        match &item.kind {
            HirItemKind::RustBlock(rb) => {
                self.emit_stmt(StatementKind::RustBlock(rb.code.clone()), rb.span);
            }
            _ => self.emit_stmt(StatementKind::Nop, span),
        }
    }

    fn lower_while(
        &mut self,
        condition: &HirExpr,
        body: &HirBlock,
        span: Span,
    ) -> Result<(), MirError> {
        let head_bb = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        self.terminate(TerminatorKind::Goto(head_bb), span);

        self.switch_to(head_bb);
        let cond = self.lower_expr(condition)?;
        self.terminate(
            TerminatorKind::SwitchBool {
                cond,
                true_bb: body_bb,
                false_bb: exit_bb,
            },
            condition.span,
        );

        self.loop_stack.push(LoopCtx {
            continue_bb: head_bb,
            break_bb: exit_bb,
        });
        self.switch_to(body_bb);
        self.push_scope();
        for stmt in &body.stmts {
            self.lower_stmt(stmt)?;
        }
        if let Some(tail) = &body.tail_expr {
            let _ = self.lower_expr(tail)?;
        }
        self.pop_scope();
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(head_bb), body.span);
        }
        self.loop_stack.pop();

        self.switch_to(exit_bb);
        Ok(())
    }

    fn lower_for(
        &mut self,
        pattern: &HirPattern,
        _iter_ty: &HirType,
        iter: &HirExpr,
        body: &HirBlock,
        span: Span,
    ) -> Result<(), MirError> {
        let iter_op = self.lower_expr(iter)?;
        let iter_local = self.operand_to_local(iter_op, &iter.ty, iter.span);
        let iter_place = Place::from_local(iter_local);

        let head_bb = self.new_block();
        let check_bb = self.new_block();
        let body_bb = self.new_block();
        let exit_bb = self.new_block();

        self.terminate(TerminatorKind::Goto(head_bb), span);

        self.switch_to(head_bb);
        let next_val = self.new_temp(HirType::Unresolved, iter.span);
        self.terminate(
            TerminatorKind::MethodCall {
                receiver: Operand::Place(iter_place),
                method: "next".to_string(),
                args: Vec::new(),
                dest: Place::from_local(next_val),
                target: check_bb,
            },
            iter.span,
        );

        self.switch_to(check_bb);
        self.terminate(
            TerminatorKind::SwitchInt {
                discr: Operand::Place(Place::from_local(next_val)),
                targets: vec![(
                    SwitchTarget::Pattern(self.some_pattern(pattern.span)),
                    body_bb,
                )],
                otherwise: exit_bb,
            },
            pattern.span,
        );

        self.loop_stack.push(LoopCtx {
            continue_bb: head_bb,
            break_bb: exit_bb,
        });
        self.switch_to(body_bb);
        self.push_scope();
        self.bind_pattern_only(pattern, HirType::Unresolved);
        for stmt in &body.stmts {
            self.lower_stmt(stmt)?;
        }
        if let Some(tail) = &body.tail_expr {
            let _ = self.lower_expr(tail)?;
        }
        self.pop_scope();
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(head_bb), body.span);
        }
        self.loop_stack.pop();

        self.switch_to(exit_bb);
        Ok(())
    }

    fn lower_loop(&mut self, body: &HirBlock, span: Span) -> Result<(), MirError> {
        let head_bb = self.new_block();
        let exit_bb = self.new_block();
        self.terminate(TerminatorKind::Goto(head_bb), span);

        self.loop_stack.push(LoopCtx {
            continue_bb: head_bb,
            break_bb: exit_bb,
        });

        self.switch_to(head_bb);
        self.push_scope();
        for stmt in &body.stmts {
            self.lower_stmt(stmt)?;
        }
        if let Some(tail) = &body.tail_expr {
            let _ = self.lower_expr(tail)?;
        }
        self.pop_scope();
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(head_bb), body.span);
        }
        self.loop_stack.pop();

        self.switch_to(exit_bb);
        Ok(())
    }

    fn lower_expr(&mut self, expr: &HirExpr) -> Result<Operand, MirError> {
        match &expr.kind {
            HirExprKind::Literal(lit) => Ok(Operand::Constant(self.literal_to_constant(lit))),
            HirExprKind::Path(path) => Ok(self.lower_path(path)),
            HirExprKind::BinaryOp { left, op, right } => {
                let left = self.lower_expr(left)?;
                let right = self.lower_expr(right)?;
                let tmp = self.new_temp(expr.ty.clone(), expr.span);
                self.emit_assign(
                    Place::from_local(tmp),
                    Rvalue::BinaryOp {
                        op: *op,
                        left,
                        right,
                    },
                    expr.span,
                );
                Ok(Operand::Place(Place::from_local(tmp)))
            }
            HirExprKind::UnaryOp { op, operand } => {
                let operand = self.lower_expr(operand)?;
                let tmp = self.new_temp(expr.ty.clone(), expr.span);
                self.emit_assign(
                    Place::from_local(tmp),
                    Rvalue::UnaryOp { op: *op, operand },
                    expr.span,
                );
                Ok(Operand::Place(Place::from_local(tmp)))
            }
            HirExprKind::FnCall { func, args } => {
                let func = self.lower_expr(func)?;
                let args = self.lower_expr_vec(args)?;
                let dest = self.new_temp(expr.ty.clone(), expr.span);
                let next = self.new_block();
                self.terminate(
                    TerminatorKind::Call {
                        func,
                        args,
                        dest: Place::from_local(dest),
                        target: next,
                    },
                    expr.span,
                );
                self.switch_to(next);
                Ok(Operand::Place(Place::from_local(dest)))
            }
            HirExprKind::MacroCall { path, tokens, .. } => {
                let args = self.extract_macro_var_refs(tokens);
                self.emit_stmt(
                    StatementKind::MacroCall {
                        path: path.clone(),
                        args,
                    },
                    expr.span,
                );
                Ok(Operand::Constant(MirConstant::Unit))
            }
            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                let receiver = self.lower_expr(receiver)?;
                let args = self.lower_expr_vec(args)?;
                let dest = self.new_temp(expr.ty.clone(), expr.span);
                let next = self.new_block();
                self.terminate(
                    TerminatorKind::MethodCall {
                        receiver,
                        method: method.clone(),
                        args,
                        dest: Place::from_local(dest),
                        target: next,
                    },
                    expr.span,
                );
                self.switch_to(next);
                Ok(Operand::Place(Place::from_local(dest)))
            }
            HirExprKind::FieldAccess { receiver, field } => {
                let recv = self.lower_expr(receiver)?;
                let place = self.operand_to_place(recv, &receiver.ty, receiver.span);
                Ok(Operand::Place(place.field(field)))
            }
            HirExprKind::OptionalChain { receiver, access } => {
                self.lower_optional_chain(expr, receiver, access)
            }
            HirExprKind::NullCoalesce {
                expr: opt_expr,
                default,
            } => self.lower_null_coalesce(expr, opt_expr, default),
            HirExprKind::Index { receiver, index } => {
                let recv = self.lower_expr(receiver)?;
                let recv_place = self.operand_to_place(recv, &receiver.ty, receiver.span);
                let idx = self.lower_expr(index)?;
                let idx_local = self.operand_to_local(idx, &index.ty, index.span);
                Ok(Operand::Place(recv_place.index(idx_local)))
            }
            HirExprKind::Block(block) => self.lower_block_expr(block),
            HirExprKind::If {
                condition,
                then_block,
                else_expr,
            } => self.lower_if_expr(expr, condition, then_block, else_expr.as_deref()),
            HirExprKind::Match { expr: discr, arms } => self.lower_match_expr(expr, discr, arms),
            HirExprKind::Await { expr: awaited } => {
                let receiver = self.lower_expr(awaited)?;
                let dest = self.new_temp(expr.ty.clone(), expr.span);
                let next = self.new_block();
                self.terminate(
                    TerminatorKind::MethodCall {
                        receiver,
                        method: "await".to_string(),
                        args: Vec::new(),
                        dest: Place::from_local(dest),
                        target: next,
                    },
                    expr.span,
                );
                self.switch_to(next);
                Ok(Operand::Place(Place::from_local(dest)))
            }
            HirExprKind::Assign { target, value } => {
                let target_place = self.lower_expr_place(target)?;
                let value = self.lower_expr(value)?;
                self.emit_assign(target_place, Rvalue::Use(value), expr.span);
                Ok(Operand::Constant(MirConstant::Unit))
            }
            HirExprKind::CompoundAssign { target, op, value } => {
                let target_place = self.lower_expr_place(target)?;
                let value = self.lower_expr(value)?;
                self.emit_assign(
                    target_place.clone(),
                    Rvalue::BinaryOp {
                        op: self.compound_to_binop(*op),
                        left: Operand::Place(target_place.clone()),
                        right: value,
                    },
                    expr.span,
                );
                Ok(Operand::Constant(MirConstant::Unit))
            }
            HirExprKind::StructLiteral { path, fields } => {
                let mut lowered = Vec::with_capacity(fields.len());
                for field in fields {
                    let op = if let Some(v) = &field.value {
                        self.lower_expr(v)?
                    } else {
                        self.lower_path(std::slice::from_ref(&field.name))
                    };
                    lowered.push((field.name.clone(), op));
                }

                let tmp = self.new_temp(expr.ty.clone(), expr.span);
                self.emit_assign(
                    Place::from_local(tmp),
                    Rvalue::Aggregate(AggregateKind::Struct(path.clone()), lowered),
                    expr.span,
                );
                Ok(Operand::Place(Place::from_local(tmp)))
            }
            HirExprKind::Range { .. } => {
                self.emit_stmt(StatementKind::Nop, expr.span);
                Ok(Operand::Constant(MirConstant::Unit))
            }
            HirExprKind::Closure { .. } => {
                self.emit_stmt(StatementKind::Nop, expr.span);
                Ok(Operand::Constant(MirConstant::Unit))
            }
            HirExprKind::Return(value) => {
                if let Some(v) = value {
                    let op = self.lower_expr(v)?;
                    self.emit_assign(Place::from_local(Local(0)), Rvalue::Use(op), v.span);
                }
                self.terminate(TerminatorKind::Return, expr.span);
                self.current_block = self.new_block();
                Ok(Operand::Constant(MirConstant::Unit))
            }
            HirExprKind::Tuple(items) => {
                let lowered = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| self.lower_expr(item).map(|op| (i.to_string(), op)))
                    .collect::<Result<Vec<_>, MirError>>()?;
                let tmp = self.new_temp(expr.ty.clone(), expr.span);
                self.emit_assign(
                    Place::from_local(tmp),
                    Rvalue::Aggregate(AggregateKind::Tuple, lowered),
                    expr.span,
                );
                Ok(Operand::Place(Place::from_local(tmp)))
            }
        }
    }

    fn lower_optional_chain(
        &mut self,
        expr: &HirExpr,
        receiver: &HirExpr,
        access: &nanachi_hir::HirOptionalAccess,
    ) -> Result<Operand, MirError> {
        let receiver_op = self.lower_expr(receiver)?;
        let some_bb = self.new_block();
        let none_bb = self.new_block();
        let join_bb = self.new_block();
        let result_local = self.new_temp(expr.ty.clone(), expr.span);
        let result_place = Place::from_local(result_local);

        self.terminate(
            TerminatorKind::SwitchInt {
                discr: receiver_op.clone(),
                targets: vec![(SwitchTarget::Pattern(self.some_pattern(expr.span)), some_bb)],
                otherwise: none_bb,
            },
            expr.span,
        );

        self.switch_to(some_bb);
        let value = match access {
            nanachi_hir::HirOptionalAccess::Field(name) => {
                let recv_place =
                    self.operand_to_place(receiver_op.clone(), &receiver.ty, receiver.span);
                Operand::Place(recv_place.field(name))
            }
            nanachi_hir::HirOptionalAccess::Method { name, args } => {
                let args = self.lower_expr_vec(args)?;
                let call_dest = self.new_temp(HirType::Unresolved, expr.span);
                let after_call = self.new_block();
                self.terminate(
                    TerminatorKind::MethodCall {
                        receiver: receiver_op.clone(),
                        method: name.clone(),
                        args,
                        dest: Place::from_local(call_dest),
                        target: after_call,
                    },
                    expr.span,
                );
                self.switch_to(after_call);
                Operand::Place(Place::from_local(call_dest))
            }
        };

        self.emit_assign(
            result_place.clone(),
            Rvalue::Aggregate(
                AggregateKind::Struct(vec!["Some".to_string()]),
                vec![("0".to_string(), value)],
            ),
            expr.span,
        );
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(join_bb), expr.span);
        }

        self.switch_to(none_bb);
        self.emit_assign(
            result_place.clone(),
            Rvalue::Use(Operand::Constant(MirConstant::Path(vec![
                "None".to_string(),
            ]))),
            expr.span,
        );
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(join_bb), expr.span);
        }

        self.switch_to(join_bb);
        Ok(Operand::Place(result_place))
    }

    fn lower_null_coalesce(
        &mut self,
        expr: &HirExpr,
        opt_expr: &HirExpr,
        default: &HirExpr,
    ) -> Result<Operand, MirError> {
        let opt = self.lower_expr(opt_expr)?;
        let some_bb = self.new_block();
        let none_bb = self.new_block();
        let join_bb = self.new_block();
        let result_local = self.new_temp(expr.ty.clone(), expr.span);
        let result_place = Place::from_local(result_local);

        self.terminate(
            TerminatorKind::SwitchInt {
                discr: opt.clone(),
                targets: vec![(SwitchTarget::Pattern(self.some_pattern(expr.span)), some_bb)],
                otherwise: none_bb,
            },
            expr.span,
        );

        self.switch_to(some_bb);
        self.emit_assign(result_place.clone(), Rvalue::Use(opt), expr.span);
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(join_bb), expr.span);
        }

        self.switch_to(none_bb);
        let fallback = self.lower_expr(default)?;
        self.emit_assign(result_place.clone(), Rvalue::Use(fallback), default.span);
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Goto(join_bb), expr.span);
        }

        self.switch_to(join_bb);
        Ok(Operand::Place(result_place))
    }

    fn lower_if_expr(
        &mut self,
        expr: &HirExpr,
        condition: &HirExpr,
        then_block: &HirBlock,
        else_expr: Option<&HirExpr>,
    ) -> Result<Operand, MirError> {
        let cond = self.lower_expr(condition)?;
        let then_bb = self.new_block();
        let else_bb = self.new_block();
        let join_bb = self.new_block();
        let result = self.new_temp(expr.ty.clone(), expr.span);
        let result_place = Place::from_local(result);

        self.terminate(
            TerminatorKind::SwitchBool {
                cond,
                true_bb: then_bb,
                false_bb: else_bb,
            },
            condition.span,
        );

        self.switch_to(then_bb);
        let then_val = self.lower_block_expr(then_block)?;
        if !self.is_current_terminated() {
            self.emit_assign(result_place.clone(), Rvalue::Use(then_val), then_block.span);
            self.terminate(TerminatorKind::Goto(join_bb), then_block.span);
        }

        self.switch_to(else_bb);
        let else_val = if let Some(e) = else_expr {
            self.lower_expr(e)?
        } else {
            Operand::Constant(MirConstant::Unit)
        };
        if !self.is_current_terminated() {
            self.emit_assign(result_place.clone(), Rvalue::Use(else_val), expr.span);
            self.terminate(TerminatorKind::Goto(join_bb), expr.span);
        }

        self.switch_to(join_bb);
        Ok(Operand::Place(result_place))
    }

    fn lower_match_expr(
        &mut self,
        expr: &HirExpr,
        discr: &HirExpr,
        arms: &[nanachi_hir::HirMatchArm],
    ) -> Result<Operand, MirError> {
        let discr = self.lower_expr(discr)?;
        let arm_bbs = (0..arms.len())
            .map(|_| self.new_block())
            .collect::<Vec<_>>();
        let otherwise_bb = self.new_block();
        let join_bb = self.new_block();
        let result = self.new_temp(expr.ty.clone(), expr.span);
        let result_place = Place::from_local(result);

        let targets = arms
            .iter()
            .zip(arm_bbs.iter().copied())
            .map(|(arm, bb)| (SwitchTarget::Pattern(arm.pattern.clone()), bb))
            .collect::<Vec<_>>();

        self.terminate(
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise: otherwise_bb,
            },
            expr.span,
        );

        for (arm, bb) in arms.iter().zip(arm_bbs) {
            self.switch_to(bb);
            self.push_scope();
            self.bind_pattern_only(&arm.pattern, HirType::Unresolved);

            if let Some(guard) = &arm.guard {
                let guard_result = self.lower_expr(guard)?;
                let pass_bb = self.new_block();
                self.terminate(
                    TerminatorKind::SwitchBool {
                        cond: guard_result,
                        true_bb: pass_bb,
                        false_bb: otherwise_bb,
                    },
                    guard.span,
                );
                self.switch_to(pass_bb);
            }

            let body_val = self.lower_expr(&arm.body)?;
            if !self.is_current_terminated() {
                self.emit_assign(result_place.clone(), Rvalue::Use(body_val), arm.span);
                self.terminate(TerminatorKind::Goto(join_bb), arm.span);
            }
            self.pop_scope();
        }

        self.switch_to(otherwise_bb);
        if !self.is_current_terminated() {
            self.terminate(TerminatorKind::Unreachable, expr.span);
        }

        self.switch_to(join_bb);
        Ok(Operand::Place(result_place))
    }

    fn lower_block_expr(&mut self, block: &HirBlock) -> Result<Operand, MirError> {
        self.push_scope();
        for stmt in &block.stmts {
            self.lower_stmt(stmt)?;
        }
        let tail = if let Some(tail_expr) = &block.tail_expr {
            if self.is_current_terminated() {
                Operand::Constant(MirConstant::Unit)
            } else {
                self.lower_expr(tail_expr)?
            }
        } else {
            Operand::Constant(MirConstant::Unit)
        };
        self.pop_scope();
        Ok(tail)
    }

    fn lower_path(&self, path: &[String]) -> Operand {
        if path.len() == 1 {
            if let Some(local) = self.lookup_local(&path[0]) {
                return Operand::Place(Place::from_local(local));
            }
        }
        Operand::Constant(MirConstant::Path(path.to_vec()))
    }

    fn lower_expr_vec(&mut self, exprs: &[HirExpr]) -> Result<Vec<Operand>, MirError> {
        exprs.iter().map(|e| self.lower_expr(e)).collect()
    }

    fn lower_expr_place(&mut self, expr: &HirExpr) -> Result<Place, MirError> {
        let op = self.lower_expr(expr)?;
        Ok(self.operand_to_place(op, &expr.ty, expr.span))
    }

    fn compound_to_binop(&self, op: CompoundOp) -> BinOp {
        match op {
            CompoundOp::Add => BinOp::Add,
            CompoundOp::Sub => BinOp::Sub,
            CompoundOp::Mul => BinOp::Mul,
            CompoundOp::Div => BinOp::Div,
            CompoundOp::Rem => BinOp::Rem,
            CompoundOp::BitAnd => BinOp::BitAnd,
            CompoundOp::BitOr => BinOp::BitOr,
            CompoundOp::BitXor => BinOp::BitXor,
            CompoundOp::Shl => BinOp::Shl,
            CompoundOp::Shr => BinOp::Shr,
        }
    }

    fn literal_to_constant(&self, lit: &Literal) -> MirConstant {
        match lit {
            Literal::Int(v) => MirConstant::Int(v.clone()),
            Literal::Float(v) => MirConstant::Float(v.clone()),
            Literal::String(v) => MirConstant::String(v.clone()),
            Literal::Char(v) => MirConstant::Char(*v),
            Literal::Bool(v) => MirConstant::Bool(*v),
        }
    }

    fn bind_let_pattern(
        &mut self,
        pattern: &HirPattern,
        ty: &HirType,
        value: Option<Operand>,
        span: Span,
    ) {
        match &pattern.kind {
            HirPatternKind::Ident(name) => {
                let local = self.new_local_decl(name.clone(), ty.clone(), LocalKind::UserVar, span);
                self.define_local(name.clone(), local);
                if let Some(v) = value {
                    self.emit_assign(Place::from_local(local), Rvalue::Use(v), span);
                }
            }
            _ => {
                self.bind_pattern_only(pattern, ty.clone());
            }
        }
    }

    fn bind_pattern_only(&mut self, pattern: &HirPattern, ty: HirType) {
        match &pattern.kind {
            HirPatternKind::Ident(name) => {
                let local =
                    self.new_local_decl(name.clone(), ty.clone(), LocalKind::UserVar, pattern.span);
                self.define_local(name.clone(), local);
            }
            HirPatternKind::Tuple(fields) => {
                for field in fields {
                    self.bind_pattern_only(field, HirType::Unresolved);
                }
            }
            HirPatternKind::Struct { fields, .. } => {
                for field in fields {
                    if let Some(inner) = &field.pattern {
                        self.bind_pattern_only(inner, HirType::Unresolved);
                    } else {
                        let local = self.new_local_decl(
                            field.name.clone(),
                            HirType::Unresolved,
                            LocalKind::UserVar,
                            field.span,
                        );
                        self.define_local(field.name.clone(), local);
                    }
                }
            }
            HirPatternKind::TupleStruct { fields, .. } => {
                for field in fields {
                    self.bind_pattern_only(field, HirType::Unresolved);
                }
            }
            HirPatternKind::Wildcard
            | HirPatternKind::Literal(_)
            | HirPatternKind::Path(_)
            | HirPatternKind::Rest => {}
        }
    }

    fn some_pattern(&self, span: Span) -> HirPattern {
        HirPattern {
            kind: HirPatternKind::TupleStruct {
                path: vec!["Some".to_string()],
                fields: vec![HirPattern {
                    kind: HirPatternKind::Wildcard,
                    span,
                }],
            },
            span,
        }
    }

    fn new_local_decl(&mut self, name: String, ty: HirType, kind: LocalKind, span: Span) -> Local {
        let local = Local(self.next_local);
        self.next_local += 1;
        self.locals.push(LocalDecl {
            name,
            ty,
            kind,
            span,
        });
        local
    }

    fn new_temp(&mut self, ty: HirType, span: Span) -> Local {
        let idx = self.next_temp;
        self.next_temp += 1;
        self.new_local_decl(format!("_tmp{idx}"), ty, LocalKind::Temp, span)
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(WorkBlock::default());
        id
    }

    fn switch_to(&mut self, block: BlockId) {
        self.current_block = block;
    }

    fn emit_stmt(&mut self, kind: StatementKind, span: Span) {
        let block = &mut self.blocks[self.current_block.0 as usize];
        block.statements.push(Statement { kind, span });
    }

    fn emit_assign(&mut self, place: Place, rvalue: Rvalue, span: Span) {
        self.emit_stmt(StatementKind::Assign(place, rvalue), span);
    }

    fn terminate(&mut self, kind: TerminatorKind, span: Span) {
        let block = &mut self.blocks[self.current_block.0 as usize];
        block.terminator = Some(Terminator { kind, span });
    }

    fn is_current_terminated(&self) -> bool {
        self.blocks[self.current_block.0 as usize]
            .terminator
            .is_some()
    }

    fn operand_to_place(&mut self, operand: Operand, ty: &HirType, span: Span) -> Place {
        match operand {
            Operand::Place(p) => p,
            other => {
                let tmp = self.new_temp(ty.clone(), span);
                let place = Place::from_local(tmp);
                self.emit_assign(place.clone(), Rvalue::Use(other), span);
                place
            }
        }
    }

    fn operand_to_local(&mut self, operand: Operand, ty: &HirType, span: Span) -> Local {
        match operand {
            Operand::Place(place) if place.projection.is_empty() => place.local,
            other => {
                let tmp = self.new_temp(ty.clone(), span);
                self.emit_assign(Place::from_local(tmp), Rvalue::Use(other), span);
                tmp
            }
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_local(&mut self, name: String, local: Local) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, local);
        }
    }

    fn lookup_local(&self, name: &str) -> Option<Local> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    /// Extract references to in-scope locals from a macro token string.
    /// Scans for identifiers and returns `Operand::Place` for each match.
    fn extract_macro_var_refs(&self, tokens: &str) -> Vec<Operand> {
        let mut refs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let bytes = tokens.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let ident = &tokens[start..i];
                if !seen.contains(ident) {
                    if let Some(local) = self.lookup_local(ident) {
                        refs.push(Operand::Place(Place::from_local(local)));
                        seen.insert(ident);
                    }
                }
            } else {
                i += 1;
            }
        }
        refs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::StatementKind;

    fn build_mir_for(src: &str) -> MirProgram {
        let tokens = nanachi_lexer::lex(src).expect("lex");
        let ast = nanachi_parser::parse(&tokens).expect("parse");
        let hir = nanachi_hir::lower(&ast).expect("lower");
        build(&hir).expect("build mir")
    }

    fn find_body<'a>(mir: &'a MirProgram, name: &str) -> &'a MirBody {
        mir.bodies
            .iter()
            .find(|b| b.name == name)
            .unwrap_or_else(|| panic!("body '{name}' not found"))
    }

    fn assign_count(body: &MirBody, local_name: &str) -> usize {
        let local = body
            .locals
            .iter()
            .position(|l| l.name == local_name)
            .expect("local not found") as u32;
        body.blocks
            .iter()
            .flat_map(|bb| bb.statements.iter())
            .filter(|s| match &s.kind {
                StatementKind::Assign(place, _) => place.local.0 == local,
                _ => false,
            })
            .count()
    }

    #[test]
    fn simple_let() {
        let mir = build_mir_for("fn main() { let x: i32 = 5; }");
        let body = find_body(&mir, "main");
        assert_eq!(body.blocks.len(), 1);
        assert_eq!(assign_count(body, "x"), 1);
    }

    #[test]
    fn reassignment() {
        let mir = build_mir_for("fn main() { let x: i32 = 5; x = 10; }");
        let body = find_body(&mir, "main");
        assert_eq!(assign_count(body, "x"), 2);
    }

    #[test]
    fn if_else_blocks() {
        let mir = build_mir_for("fn f(x: bool) -> i32 { if x { 1 } else { 2 } }");
        let body = find_body(&mir, "f");
        assert!(body.blocks.len() >= 4);
        assert!(
            body.blocks
                .iter()
                .any(|bb| matches!(bb.terminator.kind, TerminatorKind::SwitchBool { .. }))
        );
    }

    #[test]
    fn while_loop_blocks() {
        let mir = build_mir_for("fn f(x: bool) { while x { x; } }");
        let body = find_body(&mir, "f");
        assert!(body.blocks.len() >= 4);
    }

    #[test]
    fn for_loop_blocks() {
        let mir = build_mir_for("fn f(items: Vec<i32>) { for i in items { i; } }");
        let body = find_body(&mir, "f");
        assert!(body.blocks.len() >= 5);
        assert!(
            body.blocks
                .iter()
                .any(|bb| matches!(bb.terminator.kind, TerminatorKind::MethodCall { .. }))
        );
    }

    #[test]
    fn fn_call_terminator() {
        let mir = build_mir_for("fn id(x: i32) -> i32 { x } fn f() { let y = id(1); }");
        let body = find_body(&mir, "f");
        assert!(
            body.blocks
                .iter()
                .any(|bb| matches!(bb.terminator.kind, TerminatorKind::Call { .. }))
        );
    }

    #[test]
    fn method_call_terminator() {
        let mir = build_mir_for("fn f(v: Vec<i32>) { v.push(1); }");
        let body = find_body(&mir, "f");
        assert!(
            body.blocks
                .iter()
                .any(|bb| matches!(bb.terminator.kind, TerminatorKind::MethodCall { .. }))
        );
    }

    #[test]
    fn optional_chain_and_null_coalesce() {
        let mir = build_mir_for("fn f(user: User?) -> i32 { user?.age ?? 0 }");
        let body = find_body(&mir, "f");
        let switch_count = body
            .blocks
            .iter()
            .filter(|bb| matches!(bb.terminator.kind, TerminatorKind::SwitchInt { .. }))
            .count();
        assert!(switch_count >= 2);
    }

    #[test]
    fn params_and_self_locals() {
        let mir = build_mir_for(
            "struct User { age: i32 } impl User { fn grow(self, d: i32) { self.age = self.age + d; } }",
        );
        let body = find_body(&mir, "grow");
        assert_eq!(body.owner.as_deref(), Some("User"));
        assert!(
            body.locals
                .iter()
                .any(|l| l.kind == LocalKind::SelfParam && l.name == "self")
        );
        assert!(
            body.locals
                .iter()
                .any(|l| l.kind == LocalKind::Param && l.name == "d")
        );
    }

    #[test]
    fn struct_literal_aggregate() {
        let mir = build_mir_for("struct User { age: i32 } fn f() -> User { User { age: 1 } }");
        let body = find_body(&mir, "f");
        assert!(
            body.blocks
                .iter()
                .flat_map(|bb| bb.statements.iter())
                .any(|s| match &s.kind {
                    StatementKind::Assign(_, Rvalue::Aggregate(AggregateKind::Struct(_), _)) =>
                        true,
                    _ => false,
                })
        );
    }

    #[test]
    fn macro_and_rust_block_statements() {
        let mir = build_mir_for(
            r#"fn f() {
                println!("x");
                rust { let v = 1; }
            }"#,
        );
        let body = find_body(&mir, "f");
        assert!(
            body.blocks
                .iter()
                .flat_map(|bb| bb.statements.iter())
                .any(|s| matches!(s.kind, StatementKind::MacroCall { .. }))
        );
        assert!(
            body.blocks
                .iter()
                .flat_map(|bb| bb.statements.iter())
                .any(|s| matches!(s.kind, StatementKind::RustBlock(_)))
        );
    }

    #[test]
    fn async_await_method_call() {
        let mir = build_mir_for("async fn f() { foo().await; }");
        let body = find_body(&mir, "f");
        assert!(body.is_async);
        assert!(body.blocks.iter().any(|bb| match &bb.terminator.kind {
            TerminatorKind::MethodCall { method, .. } => method == "await",
            _ => false,
        }));
    }

    #[test]
    fn loop_break_continue() {
        let mir = build_mir_for(
            "fn f() { let x: i32 = 0; loop { x = x + 1; if x > 10 { break; } continue; } }",
        );
        let body = find_body(&mir, "f");
        // loop: entry → head, body, exit (at least 3 blocks)
        assert!(body.blocks.len() >= 3);
        // break and continue each emit Goto terminators
        let goto_count = body
            .blocks
            .iter()
            .filter(|bb| matches!(bb.terminator.kind, TerminatorKind::Goto(_)))
            .count();
        assert!(
            goto_count >= 2,
            "expected at least 2 Goto terminators (break+continue), got {goto_count}"
        );
    }

    #[test]
    fn field_access_place() {
        let mir =
            build_mir_for("struct Pt { x: i32, y: i32 } fn f(p: Pt) -> i32 { p.x }");
        let body = find_body(&mir, "f");
        use crate::mir::PlaceElem;
        // tail expr `p.x` → Assign(_0, Use(Place(p, [Field("x")])))
        let has_field_proj = body
            .blocks
            .iter()
            .flat_map(|bb| bb.statements.iter())
            .any(|s| match &s.kind {
                StatementKind::Assign(_, Rvalue::Use(Operand::Place(place))) => place
                    .projection
                    .iter()
                    .any(|p| matches!(p, PlaceElem::Field(f) if f == "x")),
                _ => false,
            });
        assert!(has_field_proj, "expected field projection for p.x");
    }

    #[test]
    fn match_arms() {
        let mir = build_mir_for(
            r#"enum Color { Red, Blue }
            fn f(c: Color) -> i32 {
                match c {
                    Color::Red => 1,
                    Color::Blue => 2,
                }
            }"#,
        );
        let body = find_body(&mir, "f");
        assert!(
            body.blocks
                .iter()
                .any(|bb| matches!(bb.terminator.kind, TerminatorKind::SwitchInt { .. }))
        );
        // entry + 2 arms + otherwise + join = 5 blocks minimum
        assert!(
            body.blocks.len() >= 5,
            "expected >= 5 blocks for match, got {}",
            body.blocks.len()
        );
    }

    #[test]
    fn sketch_ownership() {
        let src = r#"
            fn greet(name: String) {
                println!("Hello, {}", name);
            }
            fn push_name(list: Vec<String>, name: String) {
                list.push(name);
            }
            fn ownership_example() {
                let a: String = "hello";
                let b: String = "world";
                let list: Vec<String> = Vec::new();
                greet(a);
                greet(b);
                push_name(list, a);
                push_name(list, b);
                println!("{}", b);
            }
        "#;
        let mir = build_mir_for(src);
        assert!(mir.bodies.iter().any(|b| b.name == "greet"));
        assert!(mir.bodies.iter().any(|b| b.name == "push_name"));
        let body = find_body(&mir, "ownership_example");
        assert!(body.locals.iter().any(|l| l.name == "a"));
        assert!(body.locals.iter().any(|l| l.name == "b"));
        assert!(body.locals.iter().any(|l| l.name == "list"));
        // Vec::new + greet x2 + push_name x2 = 5 Call terminators
        let call_count = body
            .blocks
            .iter()
            .filter(|bb| matches!(bb.terminator.kind, TerminatorKind::Call { .. }))
            .count();
        assert!(
            call_count >= 5,
            "expected at least 5 Call terminators, got {call_count}"
        );
    }

    #[test]
    fn sketch_struct_impl() {
        let src = r#"
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
        "#;
        let mir = build_mir_for(src);
        assert_eq!(mir.bodies.len(), 3);
        for body in &mir.bodies {
            assert_eq!(body.owner.as_deref(), Some("User"));
        }
        // `new` returns Aggregate(Struct)
        let new_body = find_body(&mir, "new");
        let has_struct_agg = new_body
            .blocks
            .iter()
            .flat_map(|bb| bb.statements.iter())
            .any(|s| {
                matches!(
                    &s.kind,
                    StatementKind::Assign(
                        _,
                        Rvalue::Aggregate(AggregateKind::Struct(path), _)
                    ) if path == &["User"]
                )
            });
        assert!(has_struct_agg, "User::new should produce Aggregate(Struct)");
        // `grow` has SelfParam and assigns to self.age
        let grow_body = find_body(&mir, "grow");
        assert!(grow_body
            .locals
            .iter()
            .any(|l| l.kind == LocalKind::SelfParam));
        assert!(assign_count(grow_body, "self") >= 1);
    }

    #[test]
    fn tuple_aggregate() {
        let mir = build_mir_for("fn f() -> (i32, i32) { (1, 2) }");
        let body = find_body(&mir, "f");
        let has_tuple_agg = body
            .blocks
            .iter()
            .flat_map(|bb| bb.statements.iter())
            .any(|s| {
                matches!(
                    &s.kind,
                    StatementKind::Assign(_, Rvalue::Aggregate(AggregateKind::Tuple, _))
                )
            });
        assert!(has_tuple_agg, "expected Aggregate(Tuple)");
    }

    #[test]
    fn return_in_middle() {
        let mir = build_mir_for("fn f(x: i32) -> i32 { if x > 0 { return x; } 0 }");
        let body = find_body(&mir, "f");
        let return_count = body
            .blocks
            .iter()
            .filter(|bb| matches!(bb.terminator.kind, TerminatorKind::Return))
            .count();
        // early return + final return
        assert!(
            return_count >= 2,
            "expected >= 2 Return terminators, got {return_count}"
        );
    }
}
