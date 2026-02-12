/// Parser state threaded through all parsing functions via `Stateful`.
#[derive(Debug, Clone)]
pub struct ParseContext {
    /// Set when the first half of `>>` (Shr) has been consumed as `>`.
    pub pending_gt: bool,
    /// Suppress struct literal parsing (e.g. in `if`/`while` conditions).
    pub no_struct_literal: bool,
}

impl ParseContext {
    pub fn new() -> Self {
        Self {
            pending_gt: false,
            no_struct_literal: false,
        }
    }
}
