use std::collections::HashMap;

use crate::hir::{HirType, HirVariantFields};

// ── Symbol ──────────────────────────────────────────────────

/// Information about a symbol in the current scope.
#[derive(Debug, Clone)]
pub enum Symbol {
    /// A local variable or parameter.
    Variable { ty: HirType },
    /// A function (top-level or method).
    Function {
        params: Vec<(String, HirType)>,
        return_ty: HirType,
    },
    /// A struct definition.
    Struct {
        fields: Vec<(String, HirType)>,
    },
    /// An enum definition.
    Enum {
        variants: Vec<(String, HirVariantFields)>,
    },
    /// A trait definition.
    Trait {
        methods: Vec<TraitMethodSig>,
    },
}

/// Signature of a trait method.
#[derive(Debug, Clone)]
pub struct TraitMethodSig {
    pub name: String,
    pub params: Vec<(String, HirType)>,
    pub return_ty: HirType,
}

// ── Scope ───────────────────────────────────────────────────

/// A stack of scopes for name resolution.
pub struct Scope {
    /// Stack of symbol tables. Last = innermost scope.
    frames: Vec<HashMap<String, Symbol>>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
        }
    }

    /// Push a new scope frame.
    pub fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    /// Pop the innermost scope frame.
    pub fn pop(&mut self) {
        self.frames.pop();
    }

    /// Define a symbol in the current (innermost) scope.
    pub fn define(&mut self, name: String, symbol: Symbol) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name, symbol);
        }
    }

    /// Look up a symbol by name, searching from innermost to outermost scope.
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for frame in self.frames.iter().rev() {
            if let Some(sym) = frame.get(name) {
                return Some(sym);
            }
        }
        None
    }
}