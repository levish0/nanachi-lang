use nanachi_ast::expr::{BinOp, UnOp};
use nanachi_hir::{HirPattern, HirType};
use nanachi_lexer::Span;

// ── Index types ────────────────────────────────────────────

/// Index into `MirBody::locals`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local(pub u32);

/// Index into `MirBody::blocks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

// ── LocalDecl ──────────────────────────────────────────────

/// Declaration of a local variable (parameter, user var, temp, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalDecl {
    pub name: String,
    pub ty: HirType,
    pub kind: LocalKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    /// `_0` — function return value.
    ReturnPlace,
    /// Function parameter.
    Param,
    /// `self` parameter.
    SelfParam,
    /// User-declared variable (`let x = ...`).
    UserVar,
    /// Compiler-generated temporary.
    Temp,
}

// ── Place ──────────────────────────────────────────────────

/// An addressable location (rustc-inspired).
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<PlaceElem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaceElem {
    /// `.field_name`
    Field(String),
    /// `[index]`
    Index(Local),
}

impl Place {
    pub fn from_local(local: Local) -> Self {
        Place {
            local,
            projection: Vec::new(),
        }
    }

    pub fn field(mut self, name: &str) -> Self {
        self.projection.push(PlaceElem::Field(name.to_string()));
        self
    }

    pub fn index(mut self, idx: Local) -> Self {
        self.projection.push(PlaceElem::Index(idx));
        self
    }
}

// ── Operand & Constant ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Place(Place),
    Constant(MirConstant),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MirConstant {
    Int(String),
    Float(String),
    String(String),
    Char(char),
    Bool(bool),
    /// Symbolic path like `foo` / `std::io::Error` / `None`.
    Path(Vec<String>),
    Unit,
}

// ── Rvalue ─────────────────────────────────────────────────

/// Right-hand side of an assignment.
#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp {
        op: BinOp,
        left: Operand,
        right: Operand,
    },
    UnaryOp {
        op: UnOp,
        operand: Operand,
    },
    /// Struct/tuple literal construction.
    Aggregate(AggregateKind, Vec<(String, Operand)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AggregateKind {
    Tuple,
    Struct(Vec<String>),
}

// ── Statement ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    /// `place = rvalue`
    Assign(Place, Rvalue),
    /// Macro call — opaque, but tracks argument reads.
    MacroCall {
        path: Vec<String>,
        args: Vec<Operand>,
    },
    /// `rust { ... }` block — passed through verbatim.
    RustBlock(String),
    /// No operation.
    Nop,
}

// ── Terminator ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminatorKind {
    Goto(BlockId),
    SwitchBool {
        cond: Operand,
        true_bb: BlockId,
        false_bb: BlockId,
    },
    SwitchInt {
        discr: Operand,
        targets: Vec<(SwitchTarget, BlockId)>,
        otherwise: BlockId,
    },
    Call {
        func: Operand,
        args: Vec<Operand>,
        dest: Place,
        target: BlockId,
    },
    MethodCall {
        receiver: Operand,
        method: String,
        args: Vec<Operand>,
        dest: Place,
        target: BlockId,
    },
    Return,
    Unreachable,
}

/// Match arm target — keeps HirPattern for analyzer to extract bindings.
#[derive(Debug, Clone, PartialEq)]
pub enum SwitchTarget {
    Pattern(HirPattern),
}

// ── BasicBlock & MirBody ───────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

/// MIR for a single function or method.
#[derive(Debug, Clone, PartialEq)]
pub struct MirBody {
    pub name: String,
    pub owner: Option<String>,
    pub is_async: bool,
    pub locals: Vec<LocalDecl>,
    pub blocks: Vec<BasicBlock>,
    pub arg_count: usize,
    pub return_ty: HirType,
    pub span: Span,
}

/// MIR for the entire program.
#[derive(Debug, Clone, PartialEq)]
pub struct MirProgram {
    pub bodies: Vec<MirBody>,
}
