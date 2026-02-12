use std::collections::HashMap;

use nanachi_hir::{HirItemKind, HirProgram, HirType};
use nanachi_lexer::Span;
use nanachi_mir::{
    Local, LocalKind, MirBody, MirProgram, Operand, Rvalue, StatementKind, TerminatorKind,
};

use crate::hints;
use crate::liveness::{self, LivenessInfo};
use crate::{AnalysisWarning, ArgAction, CallSiteInfo, FnAnalysis, FnKey, ParamSig, SelfSig};

// ── Local Analysis (Level 1 + Level 2) ──────────────────────

/// Analyze a single function body for parameter/self signatures and call-site info.
/// Returns (param_sigs, self_sig, call_sites, warnings).
pub fn analyze_ownership_local(
    body: &MirBody,
    liveness: &LivenessInfo,
) -> (
    HashMap<String, ParamSig>,
    Option<SelfSig>,
    HashMap<Span, CallSiteInfo>,
    Vec<AnalysisWarning>,
) {
    let mut param_sigs = HashMap::new();
    let mut self_sig = None;
    let mut warnings = Vec::new();

    // Initialize all params/self to Ref (least privilege)
    for decl in &body.locals {
        match decl.kind {
            LocalKind::Param => {
                param_sigs.insert(decl.name.clone(), ParamSig::Ref);
            }
            LocalKind::SelfParam => {
                self_sig = Some(SelfSig::Ref);
            }
            _ => {}
        }
    }

    // Level 1: Direct MIR evidence
    analyze_level1(body, &mut param_sigs, &mut self_sig);

    // Level 2: Hint-based evidence
    analyze_level2(body, &mut param_sigs, &mut self_sig, &mut warnings);

    // Call-sites: partial (without cross-function info yet)
    let call_sites = compute_initial_call_sites(body, liveness);

    (param_sigs, self_sig, call_sites, warnings)
}

/// Level 1: Scan MIR for direct assignment / consumption evidence.
fn analyze_level1(
    body: &MirBody,
    param_sigs: &mut HashMap<String, ParamSig>,
    self_sig: &mut Option<SelfSig>,
) {
    for bb in &body.blocks {
        for stmt in &bb.statements {
            if let StatementKind::Assign(place, rvalue) = &stmt.kind {
                // Check if a param/self is the target of assignment
                let local_idx = place.local.0 as usize;
                if local_idx < body.locals.len() {
                    let decl = &body.locals[local_idx];
                    match decl.kind {
                        LocalKind::Param if !place.projection.is_empty() => {
                            // Assign to param.field → RefMut
                            widen_param(param_sigs, &decl.name, ParamSig::RefMut);
                        }
                        LocalKind::Param if place.projection.is_empty() => {
                            // Direct reassignment (`x = ...`) mutates only local binding.
                            // Mutability pass will mark this as `mut`, but caller does not
                            // need `&mut` here.
                        }
                        LocalKind::SelfParam if !place.projection.is_empty() => {
                            // Assign to self.field → RefMut
                            widen_self(self_sig, SelfSig::RefMut);
                        }
                        LocalKind::SelfParam if place.projection.is_empty() => {
                            // Direct reassignment of self binding is local mutability only.
                        }
                        _ => {}
                    }
                }

                // Check if a param/self is consumed (used in Aggregate or Return)
                match rvalue {
                    Rvalue::Aggregate(_, fields) => {
                        for (_, op) in fields {
                            check_consumed(op, body, param_sigs, self_sig);
                        }
                    }
                    Rvalue::Use(op) => {
                        // Check if this is _0 assignment (return value)
                        if place.local.0 == 0 && place.projection.is_empty() {
                            check_consumed(op, body, param_sigs, self_sig);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Level 2: Check call targets for hint-based evidence.
fn analyze_level2(
    body: &MirBody,
    param_sigs: &mut HashMap<String, ParamSig>,
    self_sig: &mut Option<SelfSig>,
    _warnings: &mut Vec<AnalysisWarning>,
) {
    for bb in &body.blocks {
        match &bb.terminator.kind {
            TerminatorKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                // Determine receiver type
                let recv_type = operand_type(receiver, body);
                if let Some(hint) = hints::method_hint(&recv_type, method) {
                    // Receiver → check param/self
                    let recv_sig = match hint.receiver {
                        SelfSig::Ref => ParamSig::Ref,
                        SelfSig::RefMut => ParamSig::RefMut,
                        SelfSig::Owned => ParamSig::Owned,
                    };
                    apply_operand_sig(receiver, body, recv_sig, param_sigs, self_sig);

                    // Args → check params
                    for (i, arg) in args.iter().enumerate() {
                        if let Some(&arg_sig) = hint.args.get(i) {
                            apply_operand_sig(arg, body, arg_sig, param_sigs, self_sig);
                        }
                    }
                }
            }
            TerminatorKind::Call { func, args, .. } => {
                if let Some(path) = operand_path(func) {
                    if let Some(hint) = hints::function_hint(&path) {
                        for (i, arg) in args.iter().enumerate() {
                            if let Some(&arg_sig) = hint.args.get(i) {
                                apply_operand_sig(arg, body, arg_sig, param_sigs, self_sig);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Apply an inferred signature to whatever param/self the operand refers to.
fn apply_operand_sig(
    op: &Operand,
    body: &MirBody,
    sig: ParamSig,
    param_sigs: &mut HashMap<String, ParamSig>,
    self_sig: &mut Option<SelfSig>,
) {
    if let Some(local) = operand_root_local(op) {
        let idx = local.0 as usize;
        if idx < body.locals.len() {
            let decl = &body.locals[idx];
            match decl.kind {
                LocalKind::Param => {
                    widen_param(param_sigs, &decl.name, sig);
                }
                LocalKind::SelfParam => {
                    let self_equivalent = match sig {
                        ParamSig::Ref => SelfSig::Ref,
                        ParamSig::RefMut => SelfSig::RefMut,
                        ParamSig::Owned => SelfSig::Owned,
                    };
                    widen_self(self_sig, self_equivalent);
                }
                _ => {}
            }
        }
    }
}

fn check_consumed(
    op: &Operand,
    body: &MirBody,
    param_sigs: &mut HashMap<String, ParamSig>,
    self_sig: &mut Option<SelfSig>,
) {
    if let Some(local) = operand_root_local(op) {
        let idx = local.0 as usize;
        if idx < body.locals.len() {
            let decl = &body.locals[idx];
            match decl.kind {
                LocalKind::Param => {
                    widen_param(param_sigs, &decl.name, ParamSig::Owned);
                }
                LocalKind::SelfParam => {
                    widen_self(self_sig, SelfSig::Owned);
                }
                _ => {}
            }
        }
    }
}

fn widen_param(sigs: &mut HashMap<String, ParamSig>, name: &str, new: ParamSig) {
    let entry = sigs.entry(name.to_string()).or_insert(ParamSig::Ref);
    if new > *entry {
        *entry = new;
    }
}

fn widen_self(sig: &mut Option<SelfSig>, new: SelfSig) {
    if let Some(s) = sig {
        if new > *s {
            *s = new;
        }
    }
}

// ── Cross-function Convergence (Level 3) ─────────────────────

/// Iterate to convergence: when a callee's param sig changes, re-analyze callers.
pub fn converge_cross_function(mir: &MirProgram, functions: &mut HashMap<FnKey, FnAnalysis>) {
    loop {
        let mut changed = false;
        for body in &mir.bodies {
            let key = FnKey {
                name: body.name.clone(),
                owner: body.owner.clone(),
            };

            // Check each call terminator: if callee is a user-defined function,
            // use its resolved param sigs to widen caller's param sigs.
            for bb in &body.blocks {
                match &bb.terminator.kind {
                    TerminatorKind::Call { func, args, .. } => {
                        if let Some(callee_key) = resolve_callee_key(func, body) {
                            if let Some(callee_analysis) = functions.get(&callee_key).cloned() {
                                // Check each arg against callee's param sigs
                                if widen_caller_from_callee(
                                    args,
                                    body,
                                    &callee_analysis,
                                    &key,
                                    functions,
                                ) {
                                    changed = true;
                                }
                            }
                        }
                    }
                    TerminatorKind::MethodCall {
                        receiver,
                        method,
                        args,
                        ..
                    } => {
                        if let Some(callee_key) = resolve_method_callee_key(receiver, method, body)
                        {
                            if let Some(callee_analysis) = functions.get(&callee_key).cloned() {
                                if widen_caller_from_method_receiver(
                                    receiver,
                                    body,
                                    callee_analysis.self_sig,
                                    &key,
                                    functions,
                                ) {
                                    changed = true;
                                }

                                if widen_caller_from_callee(
                                    args,
                                    body,
                                    &callee_analysis,
                                    &key,
                                    functions,
                                ) {
                                    changed = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn widen_caller_from_callee(
    args: &[Operand],
    body: &MirBody,
    callee_analysis: &FnAnalysis,
    caller_key: &FnKey,
    functions: &mut HashMap<FnKey, FnAnalysis>,
) -> bool {
    let mut changed = false;

    for (i, arg) in args.iter().enumerate() {
        let callee_sig = callee_analysis
            .param_order
            .get(i)
            .and_then(|name| callee_analysis.param_sigs.get(name).copied());

        if let Some(callee_sig) = callee_sig {
            if let Some(local) = operand_root_local(arg) {
                let idx = local.0 as usize;
                if idx < body.locals.len() {
                    let decl = &body.locals[idx];
                    if let Some(caller_analysis) = functions.get_mut(caller_key) {
                        match decl.kind {
                            LocalKind::Param => {
                                let entry = caller_analysis
                                    .param_sigs
                                    .entry(decl.name.clone())
                                    .or_insert(ParamSig::Ref);
                                if callee_sig > *entry {
                                    *entry = callee_sig;
                                    changed = true;
                                }
                            }
                            LocalKind::SelfParam => {
                                let callee_self = match callee_sig {
                                    ParamSig::Ref => SelfSig::Ref,
                                    ParamSig::RefMut => SelfSig::RefMut,
                                    ParamSig::Owned => SelfSig::Owned,
                                };
                                if let Some(ref mut s) = caller_analysis.self_sig {
                                    if callee_self > *s {
                                        *s = callee_self;
                                        changed = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    changed
}

fn widen_caller_from_method_receiver(
    receiver: &Operand,
    body: &MirBody,
    callee_self_sig: Option<SelfSig>,
    caller_key: &FnKey,
    functions: &mut HashMap<FnKey, FnAnalysis>,
) -> bool {
    let Some(callee_self_sig) = callee_self_sig else {
        return false;
    };
    let Some(local) = operand_root_local(receiver) else {
        return false;
    };

    let idx = local.0 as usize;
    if idx >= body.locals.len() {
        return false;
    }
    let decl = &body.locals[idx];

    let Some(caller_analysis) = functions.get_mut(caller_key) else {
        return false;
    };

    match decl.kind {
        LocalKind::Param => {
            let needed = match callee_self_sig {
                SelfSig::Ref => ParamSig::Ref,
                SelfSig::RefMut => ParamSig::RefMut,
                SelfSig::Owned => ParamSig::Owned,
            };
            let entry = caller_analysis
                .param_sigs
                .entry(decl.name.clone())
                .or_insert(ParamSig::Ref);
            if needed > *entry {
                *entry = needed;
                true
            } else {
                false
            }
        }
        LocalKind::SelfParam => {
            if let Some(ref mut self_sig) = caller_analysis.self_sig {
                if callee_self_sig > *self_sig {
                    *self_sig = callee_self_sig;
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

// ── Trait Signature Unification ──────────────────────────────

/// Unify trait method signatures: for each trait method, take the widest sig across all impls.
pub fn unify_trait_sigs(hir: &HirProgram, functions: &mut HashMap<FnKey, FnAnalysis>) {
    // Collect trait → method names
    let mut trait_methods: HashMap<String, Vec<String>> = HashMap::new();
    for item in &hir.items {
        if let HirItemKind::Trait(tr) = &item.kind {
            let methods: Vec<_> = tr.methods.iter().map(|m| m.name.clone()).collect();
            trait_methods.insert(tr.name.clone(), methods);
        }
    }

    // Collect impl trait → owner type
    let mut trait_impls: HashMap<String, Vec<String>> = HashMap::new(); // trait_name → [owner_type]
    for item in &hir.items {
        if let HirItemKind::Impl(imp) = &item.kind {
            if let Some(trait_path) = &imp.trait_name {
                let trait_name = trait_path.last().cloned().unwrap_or_default();
                let owner = type_to_owner(&imp.target);
                if let Some(owner) = owner {
                    trait_impls.entry(trait_name).or_default().push(owner);
                }
            }
        }
    }

    // For each trait, for each method, find widest sig across all impls
    for (trait_name, method_names) in &trait_methods {
        if let Some(owners) = trait_impls.get(trait_name) {
            for method_name in method_names {
                let mut widest_self = SelfSig::Ref;
                let mut widest_params: HashMap<String, ParamSig> = HashMap::new();

                // Collect current sigs from all impls
                for owner in owners {
                    let key = FnKey {
                        name: method_name.clone(),
                        owner: Some(owner.clone()),
                    };
                    if let Some(analysis) = functions.get(&key) {
                        if let Some(s) = analysis.self_sig {
                            if s > widest_self {
                                widest_self = s;
                            }
                        }
                        for (name, &sig) in &analysis.param_sigs {
                            let entry = widest_params.entry(name.clone()).or_insert(ParamSig::Ref);
                            if sig > *entry {
                                *entry = sig;
                            }
                        }
                    }
                }

                // Apply widest back to all impls
                for owner in owners {
                    let key = FnKey {
                        name: method_name.clone(),
                        owner: Some(owner.clone()),
                    };
                    if let Some(analysis) = functions.get_mut(&key) {
                        if analysis.self_sig.is_some() {
                            analysis.self_sig = Some(widest_self);
                        }
                        for (name, &sig) in &widest_params {
                            analysis.param_sigs.entry(name.clone()).and_modify(|s| {
                                if sig > *s {
                                    *s = sig;
                                }
                            });
                        }
                    }
                }
            }
        }
    }
}

// ── Finalize Call-site Actions ────────────────────────────────

/// After all sigs are resolved, compute final call-site actions for each call.
pub fn finalize_call_sites(
    body: &MirBody,
    liveness: &LivenessInfo,
    functions: &mut HashMap<FnKey, FnAnalysis>,
    key: &FnKey,
) {
    let mut new_call_sites = HashMap::new();

    for (block_idx, bb) in body.blocks.iter().enumerate() {
        let block_id = nanachi_mir::BlockId(block_idx as u32);
        let span = bb.terminator.span;

        match &bb.terminator.kind {
            TerminatorKind::Call { func, args, .. } => {
                let callee_key = resolve_callee_key(func, body);
                let is_fallible = check_fallibility_for_call(func, &callee_key, functions);
                let arg_actions =
                    compute_arg_actions(args, body, &callee_key, functions, block_id, liveness);
                new_call_sites.insert(
                    span,
                    CallSiteInfo {
                        arg_actions,
                        is_fallible,
                    },
                );
            }
            TerminatorKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let recv_type = operand_type(receiver, body);
                let hint = hints::method_hint(&recv_type, method);
                let callee_key = resolve_method_callee_key(receiver, method, body);
                let callee_analysis = callee_key
                    .as_ref()
                    .and_then(|method_key| functions.get(method_key));
                let hint_fallible = hint.as_ref().map_or(false, |h| h.returns_result);
                let user_fallible = callee_analysis
                    .and_then(|analysis| analysis.error_info.as_ref())
                    .map_or(false, |info| {
                        info.needs_result_wrap || !info.error_types.is_empty()
                    });
                let is_fallible = hint_fallible || user_fallible;

                let mut arg_actions = Vec::new();
                // Receiver action: std hint > user-defined method sig > default borrow.
                let receiver_sig = hint
                    .as_ref()
                    .map(|h| h.receiver)
                    .or_else(|| callee_analysis.and_then(|analysis| analysis.self_sig));

                if let Some(receiver_sig) = receiver_sig {
                    match receiver_sig {
                        SelfSig::Ref => arg_actions.push(ArgAction::Borrow),
                        SelfSig::RefMut => arg_actions.push(ArgAction::BorrowMut),
                        SelfSig::Owned => {
                            if is_live_after_term(receiver, body, block_id, liveness) {
                                arg_actions.push(ArgAction::Clone);
                            } else {
                                arg_actions.push(ArgAction::Move);
                            }
                        }
                    }
                } else {
                    // Unknown method target/signature.
                    arg_actions.push(ArgAction::Borrow);
                }

                // Regular args
                for (i, arg) in args.iter().enumerate() {
                    let callee_arg_sig = if let Some(ref hint) = hint {
                        hint.args.get(i).copied().unwrap_or(ParamSig::Ref)
                    } else if let Some(analysis) = callee_analysis {
                        param_sig_at(analysis, i)
                    } else {
                        ParamSig::Ref
                    };

                    let action = match callee_arg_sig {
                        ParamSig::Ref => ArgAction::Borrow,
                        ParamSig::RefMut => ArgAction::BorrowMut,
                        ParamSig::Owned => {
                            if is_live_after_term(arg, body, block_id, liveness) {
                                ArgAction::Clone
                            } else {
                                ArgAction::Move
                            }
                        }
                    };
                    arg_actions.push(action);
                }

                new_call_sites.insert(
                    span,
                    CallSiteInfo {
                        arg_actions,
                        is_fallible,
                    },
                );
            }
            _ => {}
        }
    }

    if let Some(analysis) = functions.get_mut(key) {
        analysis.call_sites = new_call_sites;
    }
}

fn compute_arg_actions(
    args: &[Operand],
    body: &MirBody,
    callee_key: &Option<FnKey>,
    functions: &HashMap<FnKey, FnAnalysis>,
    block_id: nanachi_mir::BlockId,
    liveness: &LivenessInfo,
) -> Vec<ArgAction> {
    let callee_analysis = callee_key.as_ref().and_then(|k| functions.get(k));

    args.iter()
        .enumerate()
        .map(|(i, arg)| {
            let callee_sig = callee_analysis
                .map(|analysis| param_sig_at(analysis, i))
                .unwrap_or(ParamSig::Ref); // default: Ref for unknown

            match callee_sig {
                ParamSig::Ref => ArgAction::Borrow,
                ParamSig::RefMut => ArgAction::BorrowMut,
                ParamSig::Owned => {
                    if is_live_after_term(arg, body, block_id, liveness) {
                        ArgAction::Clone
                    } else {
                        ArgAction::Move
                    }
                }
            }
        })
        .collect()
}

fn check_fallibility_for_call(
    func: &Operand,
    callee_key: &Option<FnKey>,
    functions: &HashMap<FnKey, FnAnalysis>,
) -> bool {
    // Check function hints
    if let Some(path) = operand_path(func) {
        if let Some(hint) = hints::function_hint(&path) {
            if hint.returns_result {
                return true;
            }
        }
    }
    // Check cross-function
    if let Some(key) = callee_key {
        if let Some(analysis) = functions.get(key) {
            if let Some(ref info) = analysis.error_info {
                return info.needs_result_wrap || !info.error_types.is_empty();
            }
        }
    }
    false
}

fn is_live_after_term(
    op: &Operand,
    body: &MirBody,
    block_id: nanachi_mir::BlockId,
    liveness_info: &LivenessInfo,
) -> bool {
    if let Some(local) = operand_root_local(op) {
        let idx = local.0 as usize;
        if idx < body.locals.len() {
            let decl = &body.locals[idx];
            // Only track user vars and params — temps are always consumed
            match decl.kind {
                LocalKind::UserVar | LocalKind::Param | LocalKind::SelfParam => {
                    return liveness::is_live_after_terminator(block_id, local, liveness_info);
                }
                _ => return false,
            }
        }
    }
    false
}

// ── Initial Call-sites (before cross-function) ───────────────

fn compute_initial_call_sites(
    body: &MirBody,
    liveness: &LivenessInfo,
) -> HashMap<Span, CallSiteInfo> {
    let mut sites = HashMap::new();

    for (block_idx, bb) in body.blocks.iter().enumerate() {
        let block_id = nanachi_mir::BlockId(block_idx as u32);
        let span = bb.terminator.span;

        match &bb.terminator.kind {
            TerminatorKind::Call { func, args, .. } => {
                let is_fallible = if let Some(path) = operand_path(func) {
                    hints::function_hint(&path).map_or(false, |h| h.returns_result)
                } else {
                    false
                };

                let arg_actions: Vec<_> = args
                    .iter()
                    .map(|arg| {
                        if is_live_after_term(arg, body, block_id, liveness) {
                            ArgAction::Clone
                        } else {
                            ArgAction::Move
                        }
                    })
                    .collect();

                sites.insert(
                    span,
                    CallSiteInfo {
                        arg_actions,
                        is_fallible,
                    },
                );
            }
            TerminatorKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let recv_type = operand_type(receiver, body);
                let hint = hints::method_hint(&recv_type, method);
                let is_fallible = hint.as_ref().map_or(false, |h| h.returns_result);

                let mut arg_actions = Vec::new();
                // Receiver action (use hint)
                if let Some(ref h) = hint {
                    match h.receiver {
                        SelfSig::Ref => arg_actions.push(ArgAction::Borrow),
                        SelfSig::RefMut => arg_actions.push(ArgAction::BorrowMut),
                        SelfSig::Owned => {
                            if is_live_after_term(receiver, body, block_id, liveness) {
                                arg_actions.push(ArgAction::Clone);
                            } else {
                                arg_actions.push(ArgAction::Move);
                            }
                        }
                    }
                } else {
                    arg_actions.push(ArgAction::Borrow);
                }

                for (i, arg) in args.iter().enumerate() {
                    let sig = hint
                        .as_ref()
                        .and_then(|h| h.args.get(i).copied())
                        .unwrap_or(ParamSig::Ref);
                    let action = match sig {
                        ParamSig::Ref => ArgAction::Borrow,
                        ParamSig::RefMut => ArgAction::BorrowMut,
                        ParamSig::Owned => {
                            if is_live_after_term(arg, body, block_id, liveness) {
                                ArgAction::Clone
                            } else {
                                ArgAction::Move
                            }
                        }
                    };
                    arg_actions.push(action);
                }

                sites.insert(
                    span,
                    CallSiteInfo {
                        arg_actions,
                        is_fallible,
                    },
                );
            }
            _ => {}
        }
    }

    sites
}

// ── Helpers ──────────────────────────────────────────────────

fn operand_root_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Place(place) => Some(place.local),
        Operand::Constant(_) => None,
    }
}

fn operand_type(op: &Operand, body: &MirBody) -> HirType {
    match op {
        Operand::Place(place) => {
            let idx = place.local.0 as usize;
            if idx < body.locals.len() {
                body.locals[idx].ty.clone()
            } else {
                HirType::Unresolved
            }
        }
        Operand::Constant(_) => HirType::Unresolved,
    }
}

fn operand_path(op: &Operand) -> Option<Vec<String>> {
    match op {
        Operand::Constant(nanachi_mir::MirConstant::Path(path)) => Some(path.clone()),
        _ => None,
    }
}

fn resolve_callee_key(func: &Operand, _body: &MirBody) -> Option<FnKey> {
    let path = operand_path(func)?;
    if path.len() == 1 {
        Some(FnKey {
            name: path[0].clone(),
            owner: None,
        })
    } else if path.len() == 2 {
        Some(FnKey {
            name: path[1].clone(),
            owner: Some(path[0].clone()),
        })
    } else {
        None
    }
}

fn resolve_method_callee_key(receiver: &Operand, method: &str, body: &MirBody) -> Option<FnKey> {
    if let Some(local) = operand_root_local(receiver) {
        let idx = local.0 as usize;
        if idx < body.locals.len()
            && body.locals[idx].kind == LocalKind::SelfParam
            && body.owner.is_some()
        {
            return Some(FnKey {
                name: method.to_string(),
                owner: body.owner.clone(),
            });
        }
    }

    let owner = type_to_owner(&operand_type(receiver, body))?;
    Some(FnKey {
        name: method.to_string(),
        owner: Some(owner),
    })
}

fn param_sig_at(analysis: &FnAnalysis, idx: usize) -> ParamSig {
    analysis
        .param_order
        .get(idx)
        .and_then(|name| analysis.param_sigs.get(name).copied())
        .unwrap_or(ParamSig::Ref)
}

fn type_to_owner(ty: &HirType) -> Option<String> {
    match ty {
        HirType::Named { path, .. } if !path.is_empty() => Some(path.join("::")),
        _ => None,
    }
}
