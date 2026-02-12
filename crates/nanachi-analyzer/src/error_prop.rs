use std::collections::HashMap;

use nanachi_hir::{HirItemKind, HirProgram, HirType};
use nanachi_mir::{LocalKind, MirBody, MirProgram, Operand, TerminatorKind};

use crate::hints;
use crate::{ErrorInfo, ErrorType, FnAnalysis, FnKey};

/// Analyze error propagation across the entire program.
/// Returns a map from function key to optional error info.
pub fn analyze_errors(
    mir: &MirProgram,
    hir: &HirProgram,
    _functions: &HashMap<FnKey, FnAnalysis>,
) -> HashMap<FnKey, Option<ErrorInfo>> {
    let mut result: HashMap<FnKey, Option<ErrorInfo>> = HashMap::new();

    // Collect which functions have explicit Result return types
    let explicit_results = collect_explicit_results(hir);

    // Phase 1: Direct fallible calls
    for body in &mir.bodies {
        let key = FnKey {
            name: body.name.clone(),
            owner: body.owner.clone(),
        };

        let is_explicit = explicit_results.contains(&key);
        let mut error_types: Vec<ErrorType> = Vec::new();

        for bb in &body.blocks {
            match &bb.terminator.kind {
                TerminatorKind::Call { func, .. } => {
                    if let Some(path) = operand_path(func) {
                        if let Some(hint) = hints::function_hint(&path) {
                            if hint.returns_result {
                                if let Some(err_ty) = &hint.error_type {
                                    add_error_type(
                                        &mut error_types,
                                        err_ty.clone(),
                                        bb.terminator.span,
                                    );
                                }
                            }
                        }
                    }
                }
                TerminatorKind::MethodCall {
                    receiver, method, ..
                } => {
                    let recv_type = operand_type(receiver, body);
                    if let Some(hint) = hints::method_hint(&recv_type, method) {
                        if hint.returns_result {
                            // Generic parse error
                            add_error_type(
                                &mut error_types,
                                HirType::Named {
                                    path: vec!["ParseError".to_string()],
                                    generics: Vec::new(),
                                },
                                bb.terminator.span,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        if error_types.is_empty() {
            result.insert(key, None);
        } else {
            let needs_result_wrap = !is_explicit && key.name != "main";
            let error_enum_name = if error_types.len() >= 2 && needs_result_wrap {
                Some(make_error_enum_name(&body.name))
            } else {
                None
            };
            result.insert(
                key,
                Some(ErrorInfo {
                    error_types,
                    needs_result_wrap,
                    error_enum_name,
                }),
            );
        }
    }

    // Phase 2: Transitive propagation
    // If function F calls function G which is fallible, F is also fallible.
    loop {
        let mut changed = false;
        for body in &mir.bodies {
            let key = FnKey {
                name: body.name.clone(),
                owner: body.owner.clone(),
            };

            // Skip if already has error info or is main (stop condition)
            if key.name == "main" {
                continue;
            }

            let is_explicit = explicit_results.contains(&key);
            let mut new_errors: Vec<ErrorType> = Vec::new();

            // Collect existing error types
            if let Some(Some(existing)) = result.get(&key) {
                new_errors.extend(existing.error_types.clone());
            }

            // Check callee functions
            for bb in &body.blocks {
                match &bb.terminator.kind {
                    TerminatorKind::Call { func, .. } => {
                        if let Some(callee_key) = resolve_callee_key(func) {
                            if let Some(Some(callee_info)) = result.get(&callee_key) {
                                for et in &callee_info.error_types {
                                    add_error_type(
                                        &mut new_errors,
                                        et.ty.clone(),
                                        bb.terminator.span,
                                    );
                                }
                            }
                        }
                    }
                    TerminatorKind::MethodCall {
                        receiver, method, ..
                    } => {
                        if let Some(callee_key) = resolve_method_callee_key(receiver, method, body)
                        {
                            if let Some(Some(callee_info)) = result.get(&callee_key) {
                                for et in &callee_info.error_types {
                                    add_error_type(
                                        &mut new_errors,
                                        et.ty.clone(),
                                        bb.terminator.span,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            if !new_errors.is_empty() {
                let old = result.get(&key).cloned().flatten();
                let old_count = old.as_ref().map_or(0, |e| e.error_types.len());

                if new_errors.len() != old_count {
                    let needs_result_wrap = !is_explicit && key.name != "main";
                    let error_enum_name = if new_errors.len() >= 2 && needs_result_wrap {
                        Some(make_error_enum_name(&body.name))
                    } else {
                        None
                    };
                    result.insert(
                        key,
                        Some(ErrorInfo {
                            error_types: new_errors,
                            needs_result_wrap,
                            error_enum_name,
                        }),
                    );
                    changed = true;
                }
            }
        }

        if !changed {
            break;
        }
    }

    result
}

fn add_error_type(types: &mut Vec<ErrorType>, ty: HirType, span: nanachi_lexer::Span) {
    // Deduplicate by type
    if !types.iter().any(|et| et.ty == ty) {
        types.push(ErrorType {
            ty,
            source_span: span,
        });
    }
}

fn make_error_enum_name(fn_name: &str) -> String {
    // Convert snake_case to PascalCase + "Error"
    let pascal: String = fn_name
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect();
    format!("{pascal}Error")
}

fn operand_path(op: &Operand) -> Option<Vec<String>> {
    match op {
        Operand::Constant(nanachi_mir::MirConstant::Path(path)) => Some(path.clone()),
        _ => None,
    }
}

fn operand_type(op: &Operand, body: &nanachi_mir::MirBody) -> HirType {
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

fn resolve_callee_key(func: &Operand) -> Option<FnKey> {
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

fn operand_root_local(op: &Operand) -> Option<nanachi_mir::Local> {
    match op {
        Operand::Place(place) => Some(place.local),
        Operand::Constant(_) => None,
    }
}

fn type_to_owner(ty: &HirType) -> Option<String> {
    match ty {
        HirType::Named { path, .. } if !path.is_empty() => Some(path.join("::")),
        _ => None,
    }
}

fn collect_explicit_results(hir: &HirProgram) -> std::collections::HashSet<FnKey> {
    let mut explicit = std::collections::HashSet::new();

    for item in &hir.items {
        match &item.kind {
            HirItemKind::Function(func) => {
                if is_result_type(&func.return_ty) {
                    explicit.insert(FnKey {
                        name: func.name.clone(),
                        owner: None,
                    });
                }
            }
            HirItemKind::Impl(imp) => {
                let owner = match &imp.target {
                    HirType::Named { path, .. } if !path.is_empty() => Some(path.join("::")),
                    _ => None,
                };
                for method in &imp.methods {
                    if is_result_type(&method.return_ty) {
                        explicit.insert(FnKey {
                            name: method.name.clone(),
                            owner: owner.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    explicit
}

fn is_result_type(ty: &HirType) -> bool {
    matches!(ty, HirType::Named { path, .. } if path.last().map_or(false, |s| s == "Result"))
}
