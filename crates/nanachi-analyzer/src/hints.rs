use nanachi_hir::HirType;

use crate::{ParamSig, SelfSig};

// ── Method Hint ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MethodHint {
    pub receiver: SelfSig,
    pub args: Vec<ParamSig>,
    pub returns_result: bool,
    pub returns_future_result: bool,
}

// ── Function Hint ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FunctionHint {
    pub args: Vec<ParamSig>,
    pub returns_result: bool,
    pub returns_future_result: bool,
    pub error_type: Option<HirType>,
}

// ── Lookup Functions ─────────────────────────────────────────

/// Look up a method hint by receiver type's last path segment and method name.
pub fn method_hint(receiver_type: &HirType, method: &str) -> Option<MethodHint> {
    last_path_segment(receiver_type)
        .and_then(|type_name| lookup_method(type_name, method))
        .or_else(|| lookup_method_by_name(method))
}

/// Look up a function hint by its path segments.
pub fn function_hint(path: &[String]) -> Option<FunctionHint> {
    let key: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    lookup_function(&key)
}

fn last_path_segment(ty: &HirType) -> Option<&str> {
    match ty {
        HirType::Named { path, .. } => path.last().map(|s| s.as_str()),
        _ => None,
    }
}

// ── Method Hint Table ────────────────────────────────────────

fn lookup_method(type_name: &str, method: &str) -> Option<MethodHint> {
    let s_ref = SelfSig::Ref;
    let s_mut = SelfSig::RefMut;
    let s_own = SelfSig::Owned;
    let p_ref = ParamSig::Ref;
    let p_own = ParamSig::Owned;

    let (recv, args, fallible, future_fallible) = match (type_name, method) {
        // Vec
        ("Vec", "push") => (s_mut, vec![p_own], false, false),
        ("Vec", "pop") => (s_mut, vec![], false, false),
        ("Vec", "insert") => (s_mut, vec![p_own, p_own], false, false),
        ("Vec", "remove") => (s_mut, vec![p_own], false, false),
        ("Vec", "extend") => (s_mut, vec![p_own], false, false),
        ("Vec", "clear") => (s_mut, vec![], false, false),
        ("Vec", "sort") => (s_mut, vec![], false, false),
        ("Vec", "retain") => (s_mut, vec![p_own], false, false),
        ("Vec", "len") => (s_ref, vec![], false, false),
        ("Vec", "is_empty") => (s_ref, vec![], false, false),
        ("Vec", "iter") => (s_ref, vec![], false, false),
        ("Vec", "contains") => (s_ref, vec![p_ref], false, false),
        ("Vec", "get") => (s_ref, vec![p_ref], false, false),
        // HashMap
        ("HashMap", "insert") => (s_mut, vec![p_own, p_own], false, false),
        ("HashMap", "remove") => (s_mut, vec![p_ref], false, false),
        ("HashMap", "get") => (s_ref, vec![p_ref], false, false),
        ("HashMap", "contains_key") => (s_ref, vec![p_ref], false, false),
        ("HashMap", "keys") => (s_ref, vec![], false, false),
        ("HashMap", "values") => (s_ref, vec![], false, false),
        ("HashMap", "len") => (s_ref, vec![], false, false),
        // String
        ("String", "push_str") => (s_mut, vec![p_ref], false, false),
        ("String", "push") => (s_mut, vec![p_own], false, false),
        ("String", "truncate") => (s_mut, vec![p_own], false, false),
        ("String", "len") => (s_ref, vec![], false, false),
        ("String", "is_empty") => (s_ref, vec![], false, false),
        ("String", "as_str") => (s_ref, vec![], false, false),
        ("String", "to_uppercase") => (s_ref, vec![], false, false),
        ("String", "to_lowercase") => (s_ref, vec![], false, false),
        ("String", "trim") => (s_ref, vec![], false, false),
        ("String", "contains") => (s_ref, vec![p_ref], false, false),
        ("String", "starts_with") => (s_ref, vec![p_ref], false, false),
        ("String", "ends_with") => (s_ref, vec![p_ref], false, false),
        ("String", "split") => (s_ref, vec![p_ref], false, false),
        ("String", "lines") => (s_ref, vec![], false, false),
        // str
        ("str", "parse") => (s_ref, vec![], true, false),
        ("str", "to_string") => (s_ref, vec![], false, false),
        ("str", "split") => (s_ref, vec![p_ref], false, false),
        ("str", "trim") => (s_ref, vec![], false, false),
        ("str", "contains") => (s_ref, vec![p_ref], false, false),
        // Iterator
        ("Iterator", "collect") => (s_own, vec![], false, false),
        ("Iterator", "map") => (s_own, vec![p_own], false, false),
        ("Iterator", "filter") => (s_own, vec![p_own], false, false),
        ("Iterator", "for_each") => (s_own, vec![p_own], false, false),
        // reqwest
        ("Response", "text") => (s_ref, vec![], false, true),
        _ => return None,
    };

    Some(MethodHint {
        receiver: recv,
        args,
        returns_result: fallible,
        returns_future_result: future_fallible,
    })
}

fn lookup_method_by_name(method: &str) -> Option<MethodHint> {
    // Fallbacks for unresolved receiver types.
    match method {
        "text" => Some(MethodHint {
            receiver: SelfSig::Ref,
            args: Vec::new(),
            returns_result: false,
            returns_future_result: true,
        }),
        _ => None,
    }
}

// ── Function Hint Table ──────────────────────────────────────

fn io_error_type() -> HirType {
    HirType::Named {
        path: vec!["std".to_string(), "io".to_string(), "Error".to_string()],
        generics: Vec::new(),
    }
}

fn lookup_function(path: &[&str]) -> Option<FunctionHint> {
    use ParamSig::*;

    match path {
        ["fs", "read_to_string"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["fs", "write"] => Some(FunctionHint {
            args: vec![Ref, Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["fs", "read"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["fs", "read_dir"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["fs", "create_dir"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["File", "open"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["File", "create"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: true,
            returns_future_result: false,
            error_type: Some(io_error_type()),
        }),
        ["reqwest", "get"] => Some(FunctionHint {
            args: vec![Ref],
            returns_result: false,
            returns_future_result: true,
            error_type: None,
        }),
        _ => None,
    }
}
