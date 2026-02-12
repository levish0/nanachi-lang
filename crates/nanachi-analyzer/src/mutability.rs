use std::collections::HashSet;

use nanachi_mir::{LocalKind, MirBody, StatementKind};

/// Analyze which user variables need `mut` in the given function body.
///
/// A variable is mutable if:
/// - For `let` bindings (which emit an initial Assign): assign_count > 1
/// - For bindings without initial assign (for-loop, match): assign_count > 0
/// - Params that are directly reassigned also need `mut` on the local binding
pub fn analyze_mutability(body: &MirBody) -> HashSet<String> {
    let mut assign_counts = vec![0usize; body.locals.len()];

    for bb in &body.blocks {
        for stmt in &bb.statements {
            if let StatementKind::Assign(place, _) = &stmt.kind {
                let idx = place.local.0 as usize;
                if idx < assign_counts.len() {
                    assign_counts[idx] += 1;
                }
            }
        }
    }

    let mut mutable = HashSet::new();
    for (i, decl) in body.locals.iter().enumerate() {
        match decl.kind {
            LocalKind::UserVar => {
                if assign_counts[i] > 1 {
                    mutable.insert(decl.name.clone());
                }
            }
            LocalKind::Param | LocalKind::SelfParam => {
                // Direct reassignment of a param needs `mut` on the local binding
                // (but does NOT change the param's passing convention)
                if assign_counts[i] > 0 {
                    mutable.insert(decl.name.clone());
                }
            }
            _ => {}
        }
    }
    mutable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_mir(src: &str) -> nanachi_mir::MirProgram {
        let tokens = nanachi_lexer::lex(src).expect("lex");
        let ast = nanachi_parser::parse(&tokens).expect("parse");
        let hir = nanachi_hir::lower(&ast).expect("lower");
        nanachi_mir::build(&hir).expect("build mir")
    }

    fn find_body<'a>(mir: &'a nanachi_mir::MirProgram, name: &str) -> &'a MirBody {
        mir.bodies.iter().find(|b| b.name == name).unwrap()
    }

    #[test]
    fn reassigned_var_is_mutable() {
        let mir = build_mir("fn main() { let x: i32 = 5; x = 10; }");
        let body = find_body(&mir, "main");
        let muts = analyze_mutability(body);
        assert!(muts.contains("x"));
    }

    #[test]
    fn single_assign_is_not_mutable() {
        let mir = build_mir("fn main() { let x: i32 = 5; }");
        let body = find_body(&mir, "main");
        let muts = analyze_mutability(body);
        assert!(!muts.contains("x"));
    }

    #[test]
    fn compound_assign_is_mutable() {
        let mir = build_mir("fn main() { let x: i32 = 5; x = x + 1; }");
        let body = find_body(&mir, "main");
        let muts = analyze_mutability(body);
        assert!(muts.contains("x"));
    }

    #[test]
    fn param_reassigned_is_mutable() {
        let mir = build_mir("fn f(x: i32) { x = 5; }");
        let body = find_body(&mir, "f");
        let muts = analyze_mutability(body);
        assert!(muts.contains("x"), "reassigned param should be mutable");
    }

    #[test]
    fn param_not_reassigned_is_not_mutable() {
        let mir = build_mir("fn f(x: i32) { let y: i32 = x; }");
        let body = find_body(&mir, "f");
        let muts = analyze_mutability(body);
        assert!(!muts.contains("x"));
    }
}
