pub mod error_prop;
pub mod hints;
pub mod liveness;
pub mod mutability;
pub mod ownership;

use std::collections::{HashMap, HashSet};

use nanachi_hir::{HirProgram, HirType};
use nanachi_lexer::Span;
use nanachi_mir::MirProgram;

// ── Keys ─────────────────────────────────────────────────────

/// Identifies a function or method for analysis results.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct FnKey {
    /// Function name.
    pub name: String,
    /// Owner type name (e.g. `"User"` for `impl User`), `None` for free functions.
    pub owner: Option<String>,
}

// ── Parameter & Self Signatures ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParamSig {
    /// `&T` (or `&str` for String params).
    Ref,
    /// `&mut T`.
    RefMut,
    /// `T` (owned / consumed).
    Owned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SelfSig {
    /// `&self`.
    Ref,
    /// `&mut self`.
    RefMut,
    /// `self` (consumed).
    Owned,
}

// ── Call-site Actions ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct CallSiteInfo {
    pub arg_actions: Vec<ArgAction>,
    pub is_fallible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgAction {
    /// Argument not used after call → pass by move.
    Move,
    /// Argument used after call → insert `.clone()`.
    Clone,
    /// Callee takes `&T` → borrow.
    Borrow,
    /// Callee takes `&mut T` → mutable borrow.
    BorrowMut,
}

// ── Error Info ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorInfo {
    /// Distinct error types encountered in this function.
    pub error_types: Vec<ErrorType>,
    /// Whether the function needs automatic `Result` wrapping.
    pub needs_result_wrap: bool,
    /// Auto-generated error enum name (e.g. `ReadConfigError`), if 2+ error types.
    pub error_enum_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorType {
    pub ty: HirType,
    pub source_span: Span,
}

// ── Per-function Analysis ────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FnAnalysis {
    /// Variables that need `mut`.
    pub mutable_vars: HashSet<String>,
    /// Parameter signatures: name → Ref / RefMut / Owned.
    pub param_sigs: HashMap<String, ParamSig>,
    /// Self parameter signature, if present.
    pub self_sig: Option<SelfSig>,
    /// Call-site info keyed by span of the Call/MethodCall terminator.
    pub call_sites: HashMap<Span, CallSiteInfo>,
    /// Error propagation info, if this function is fallible.
    pub error_info: Option<ErrorInfo>,
}

// ── Warnings ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnalysisWarning {
    pub span: Span,
    pub message: String,
}

// ── Top-level Result ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub functions: HashMap<FnKey, FnAnalysis>,
    pub warnings: Vec<AnalysisWarning>,
}

// ── Entry Point ──────────────────────────────────────────────

pub fn analyze(mir: &MirProgram, hir: &HirProgram) -> AnalysisResult {
    let mut functions = HashMap::new();
    let mut warnings = Vec::new();

    // Phase 1: Per-function independent analysis (mutability + liveness + ownership L1/L2)
    for body in &mir.bodies {
        let key = FnKey {
            name: body.name.clone(),
            owner: body.owner.clone(),
        };

        let mutable_vars = mutability::analyze_mutability(body);
        let liveness_info = liveness::analyze_liveness(body);
        let (param_sigs, self_sig, call_sites, mut fn_warnings) =
            ownership::analyze_ownership_local(body, &liveness_info);

        warnings.append(&mut fn_warnings);

        functions.insert(
            key,
            FnAnalysis {
                mutable_vars,
                param_sigs,
                self_sig,
                call_sites,
                error_info: None,
            },
        );
    }

    // Phase 2: Cross-function ownership convergence (Level 3)
    ownership::converge_cross_function(mir, &mut functions);

    // Phase 3: Trait signature unification
    ownership::unify_trait_sigs(hir, &mut functions);

    // Phase 4: Error propagation
    let error_map = error_prop::analyze_errors(mir, hir, &functions);
    for (key, error_info) in error_map {
        if let Some(analysis) = functions.get_mut(&key) {
            analysis.error_info = error_info;
        }
    }

    // Phase 5: Finalize call-site actions with resolved sigs
    for body in &mir.bodies {
        let key = FnKey {
            name: body.name.clone(),
            owner: body.owner.clone(),
        };
        let liveness_info = liveness::analyze_liveness(body);
        ownership::finalize_call_sites(body, &liveness_info, &mut functions, &key);
    }

    AnalysisResult {
        functions,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_src(src: &str) -> AnalysisResult {
        let tokens = nanachi_lexer::lex(src).expect("lex");
        let ast = nanachi_parser::parse(&tokens).expect("parse");
        let hir = nanachi_hir::lower(&ast).expect("lower");
        let mir = nanachi_mir::build(&hir).expect("build mir");
        analyze(&mir, &hir)
    }

    fn get_fn<'a>(result: &'a AnalysisResult, name: &str) -> &'a FnAnalysis {
        result
            .functions
            .iter()
            .find(|(k, _)| k.name == name && k.owner.is_none())
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("function '{name}' not found in analysis"))
    }

    fn get_method<'a>(result: &'a AnalysisResult, owner: &str, name: &str) -> &'a FnAnalysis {
        result
            .functions
            .iter()
            .find(|(k, _)| k.name == name && k.owner.as_deref() == Some(owner))
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("method '{owner}::{name}' not found in analysis"))
    }

    // ── Mutability tests ─────────────────────────────────────

    #[test]
    fn mut_reassign() {
        let r = analyze_src("fn main() { let x: i32 = 5; x = 10; }");
        let f = get_fn(&r, "main");
        assert!(f.mutable_vars.contains("x"));
    }

    #[test]
    fn no_mut() {
        let r = analyze_src("fn main() { let x: i32 = 5; println!(\"{}\", x); }");
        let f = get_fn(&r, "main");
        assert!(!f.mutable_vars.contains("x"));
    }

    // ── Self signature tests ─────────────────────────────────

    #[test]
    fn self_ref() {
        let r = analyze_src(
            r#"struct User { name: String }
            impl User {
                fn greet(self) { println!("{}", self.name); }
            }"#,
        );
        let f = get_method(&r, "User", "greet");
        assert_eq!(f.self_sig, Some(SelfSig::Ref));
    }

    #[test]
    fn self_ref_mut() {
        let r = analyze_src(
            r#"struct User { age: i32 }
            impl User {
                fn grow(self) { self.age = self.age + 1; }
            }"#,
        );
        let f = get_method(&r, "User", "grow");
        assert_eq!(f.self_sig, Some(SelfSig::RefMut));
    }

    // ── Parameter signature tests ────────────────────────────

    #[test]
    fn param_ref_string() {
        let r = analyze_src(r#"fn greet(name: String) { println!("{}", name); }"#);
        let f = get_fn(&r, "greet");
        assert_eq!(f.param_sigs.get("name"), Some(&ParamSig::Ref));
    }

    #[test]
    fn param_ref_mut_via_hint() {
        let r = analyze_src("fn add(list: Vec<i32>) { list.push(1); }");
        let f = get_fn(&r, "add");
        assert_eq!(f.param_sigs.get("list"), Some(&ParamSig::RefMut));
    }

    #[test]
    fn param_consumed() {
        let r = analyze_src(
            r#"struct Wrapper { s: String }
            fn wrap(s: String) -> Wrapper { Wrapper { s } }"#,
        );
        let f = get_fn(&r, "wrap");
        assert_eq!(f.param_sigs.get("s"), Some(&ParamSig::Owned));
    }

    // ── Call-site action tests ───────────────────────────────

    #[test]
    fn call_site_move_and_clone() {
        let r = analyze_src(
            r#"fn greet(name: String) { println!("{}", name); }
            fn main() {
                let a: String = "hello";
                greet(a);
                greet(a);
                println!("{}", a);
            }"#,
        );
        let f = get_fn(&r, "main");
        // greet takes &str (Ref) → all calls should Borrow
        // Actually greet's param is Ref, so call-site action is Borrow
        let borrow_count = f
            .call_sites
            .values()
            .flat_map(|cs| cs.arg_actions.iter())
            .filter(|a| **a == ArgAction::Borrow)
            .count();
        assert!(borrow_count >= 2, "expected at least 2 Borrow actions");
    }

    // ── Error propagation tests ──────────────────────────────

    #[test]
    fn error_propagation_basic() {
        let r = analyze_src(
            r#"use std::fs;
            fn read_config(path: String) -> String {
                fs::read_to_string(path)
            }"#,
        );
        let f = get_fn(&r, "read_config");
        assert!(f.error_info.is_some());
        let info = f.error_info.as_ref().unwrap();
        assert!(info.needs_result_wrap);
        assert_eq!(info.error_types.len(), 1);
    }

    #[test]
    fn no_error_wrap_explicit_result() {
        let r = analyze_src(
            r#"use std::fs;
            fn read_config(path: String) -> Result<String, std::io::Error> {
                fs::read_to_string(path)
            }"#,
        );
        let f = get_fn(&r, "read_config");
        // User explicitly wrote Result → no auto-wrap
        if let Some(info) = &f.error_info {
            assert!(!info.needs_result_wrap);
        }
    }

    // ── Integration tests ────────────────────────────────────

    #[test]
    fn sketch_ownership_integration() {
        let r = analyze_src(
            r#"fn greet(name: String) {
                println!("Hello, {}", name);
            }
            fn push_name(list: Vec<String>, name: String) {
                list.push(name);
            }"#,
        );
        let greet = get_fn(&r, "greet");
        assert_eq!(greet.param_sigs.get("name"), Some(&ParamSig::Ref));

        let push = get_fn(&r, "push_name");
        assert_eq!(push.param_sigs.get("list"), Some(&ParamSig::RefMut));
        assert_eq!(push.param_sigs.get("name"), Some(&ParamSig::Owned));
    }

    #[test]
    fn sketch_struct_impl_integration() {
        let r = analyze_src(
            r#"struct User {
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
            }"#,
        );
        let new_fn = get_method(&r, "User", "new");
        assert_eq!(new_fn.param_sigs.get("name"), Some(&ParamSig::Owned));
        assert_eq!(new_fn.param_sigs.get("age"), Some(&ParamSig::Owned));

        let greet = get_method(&r, "User", "greet");
        assert_eq!(greet.self_sig, Some(SelfSig::Ref));

        let grow = get_method(&r, "User", "grow");
        assert_eq!(grow.self_sig, Some(SelfSig::RefMut));
    }
}
