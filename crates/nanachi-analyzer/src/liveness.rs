use std::collections::{HashMap, HashSet};

use nanachi_mir::{
    BlockId, Local, LocalKind, MirBody, Operand, Place, PlaceElem, Rvalue, StatementKind,
    TerminatorKind,
};

// ── Result Type ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LivenessInfo {
    pub live_in: HashMap<BlockId, HashSet<Local>>,
    pub live_out: HashMap<BlockId, HashSet<Local>>,
}

// ── Entry Point ──────────────────────────────────────────────

/// Classic backward dataflow liveness analysis on the MIR CFG.
pub fn analyze_liveness(body: &MirBody) -> LivenessInfo {
    let n = body.blocks.len();
    let successors = compute_successors(body);

    // Compute GEN/KILL per block
    let mut used = vec![HashSet::new(); n];
    let mut killed = vec![HashSet::new(); n];
    for (i, bb) in body.blocks.iter().enumerate() {
        compute_used_killed(body, bb, &mut used[i], &mut killed[i]);
    }

    // Initialize
    let mut live_in: Vec<HashSet<Local>> = vec![HashSet::new(); n];
    let mut live_out: Vec<HashSet<Local>> = vec![HashSet::new(); n];

    // Fixed-point iteration (backward)
    let mut changed = true;
    while changed {
        changed = false;
        for i in (0..n).rev() {
            // LiveOut[bb] = ∪ { LiveIn[succ] | succ ∈ successors(bb) }
            let mut new_out = HashSet::new();
            for &succ in &successors[i] {
                for local in &live_in[succ.0 as usize] {
                    new_out.insert(*local);
                }
            }

            // LiveIn[bb] = USED[bb] ∪ (LiveOut[bb] \ KILLED[bb])
            let mut new_in = used[i].clone();
            for local in &new_out {
                if !killed[i].contains(local) {
                    new_in.insert(*local);
                }
            }

            if new_in != live_in[i] || new_out != live_out[i] {
                live_in[i] = new_in;
                live_out[i] = new_out;
                changed = true;
            }
        }
    }

    let live_in_map = live_in
        .into_iter()
        .enumerate()
        .map(|(i, s)| (BlockId(i as u32), s))
        .collect();
    let live_out_map = live_out
        .into_iter()
        .enumerate()
        .map(|(i, s)| (BlockId(i as u32), s))
        .collect();

    LivenessInfo {
        live_in: live_in_map,
        live_out: live_out_map,
    }
}

/// Check if a local is live after a terminator in the given block.
pub fn is_live_after_terminator(
    block_id: BlockId,
    local: Local,
    liveness: &LivenessInfo,
) -> bool {
    liveness
        .live_out
        .get(&block_id)
        .map_or(false, |s| s.contains(&local))
}

// ── Internal Helpers ─────────────────────────────────────────

fn compute_successors(body: &MirBody) -> Vec<Vec<BlockId>> {
    body.blocks
        .iter()
        .map(|bb| terminator_successors(&bb.terminator.kind))
        .collect()
}

fn terminator_successors(term: &TerminatorKind) -> Vec<BlockId> {
    match term {
        TerminatorKind::Goto(bb) => vec![*bb],
        TerminatorKind::SwitchBool {
            true_bb, false_bb, ..
        } => vec![*true_bb, *false_bb],
        TerminatorKind::SwitchInt {
            targets, otherwise, ..
        } => {
            let mut succs: Vec<_> = targets.iter().map(|(_, bb)| *bb).collect();
            succs.push(*otherwise);
            succs
        }
        TerminatorKind::Call { target, .. } => vec![*target],
        TerminatorKind::MethodCall { target, .. } => vec![*target],
        TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
    }
}

fn compute_used_killed(
    body: &MirBody,
    bb: &nanachi_mir::BasicBlock,
    used: &mut HashSet<Local>,
    killed: &mut HashSet<Local>,
) {
    // Process statements in order: a use before a def → USED, a def before use → KILLED.
    for stmt in &bb.statements {
        match &stmt.kind {
            StatementKind::Assign(place, rvalue) => {
                collect_rvalue_reads(rvalue, used, killed);
                collect_place_projection_reads(place, used, killed);
                if place.projection.is_empty() {
                    killed.insert(place.local);
                }
            }
            StatementKind::MacroCall { args, .. } => {
                for arg in args {
                    add_operand_locals(arg, used, killed);
                }
            }
            StatementKind::RustBlock(_) => {
                // Conservative: mark all UserVar/Param/SelfParam as used
                for (i, decl) in body.locals.iter().enumerate() {
                    match decl.kind {
                        LocalKind::UserVar | LocalKind::Param | LocalKind::SelfParam => {
                            let local = Local(i as u32);
                            if !killed.contains(&local) {
                                used.insert(local);
                            }
                        }
                        _ => {}
                    }
                }
            }
            StatementKind::Nop => {}
        }
    }

    // Process terminator
    match &bb.terminator.kind {
        TerminatorKind::Call { func, args, .. } => {
            add_operand_locals(func, used, killed);
            for arg in args {
                add_operand_locals(arg, used, killed);
            }
        }
        TerminatorKind::MethodCall {
            receiver, args, ..
        } => {
            add_operand_locals(receiver, used, killed);
            for arg in args {
                add_operand_locals(arg, used, killed);
            }
        }
        TerminatorKind::SwitchBool { cond, .. } => {
            add_operand_locals(cond, used, killed);
        }
        TerminatorKind::SwitchInt { discr, .. } => {
            add_operand_locals(discr, used, killed);
        }
        TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

fn collect_rvalue_reads(
    rvalue: &Rvalue,
    used: &mut HashSet<Local>,
    killed: &HashSet<Local>,
) {
    match rvalue {
        Rvalue::Use(op) => add_operand_locals(op, used, killed),
        Rvalue::BinaryOp { left, right, .. } => {
            add_operand_locals(left, used, killed);
            add_operand_locals(right, used, killed);
        }
        Rvalue::UnaryOp { operand, .. } => add_operand_locals(operand, used, killed),
        Rvalue::Aggregate(_, fields) => {
            for (_, op) in fields {
                add_operand_locals(op, used, killed);
            }
        }
    }
}

fn collect_place_projection_reads(
    place: &Place,
    used: &mut HashSet<Local>,
    killed: &HashSet<Local>,
) {
    for elem in &place.projection {
        if let PlaceElem::Index(idx_local) = elem {
            if !killed.contains(idx_local) {
                used.insert(*idx_local);
            }
        }
    }
}

fn add_operand_locals(op: &Operand, used: &mut HashSet<Local>, killed: &HashSet<Local>) {
    if let Operand::Place(place) = op {
        if !killed.contains(&place.local) {
            used.insert(place.local);
        }
        collect_place_projection_reads(place, used, killed);
    }
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

    fn local_by_name(body: &MirBody, name: &str) -> Local {
        body.locals
            .iter()
            .position(|l| l.name == name)
            .map(|i| Local(i as u32))
            .unwrap_or_else(|| panic!("local '{name}' not found"))
    }

    #[test]
    fn param_live_at_entry() {
        // Param used in body → live at block entry
        let mir = build_mir("fn f(x: i32) -> i32 { x }");
        let body = find_body(&mir, "f");
        let info = analyze_liveness(body);
        let x = local_by_name(body, "x");
        let any_live = info.live_in.values().any(|s| s.contains(&x));
        assert!(any_live, "param x should be live at entry");
    }

    #[test]
    fn local_def_before_use_not_live_at_entry() {
        // x is defined then used in same block → not live at entry
        let mir = build_mir("fn main() { let x: i32 = 5; let y: i32 = x; }");
        let body = find_body(&mir, "main");
        let info = analyze_liveness(body);
        let x = local_by_name(body, "x");
        let entry = BlockId(0);
        let live_in = info.live_in.get(&entry).unwrap();
        assert!(!live_in.contains(&x), "x defined before use: not live at entry");
    }

    #[test]
    fn dead_after_last_use() {
        let mir = build_mir(
            "fn f() { let x: i32 = 5; let y: i32 = x; let z: i32 = 1; }",
        );
        let body = find_body(&mir, "f");
        let info = analyze_liveness(body);
        let x = local_by_name(body, "x");
        let exit_block = BlockId(0);
        let live_out = info.live_out.get(&exit_block).unwrap();
        assert!(!live_out.contains(&x), "x should be dead at exit");
    }
}
