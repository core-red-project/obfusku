//! Surface operators.

/// A binary operator, one per glyph.
///
/// Distinct from `obfusku_core::ast::BinOp` on purpose: the surface has `∧`
/// and `∨`, which the Core does not. They are *short-circuiting*, and Core
/// expresses that with `Match` rather than a strict operator
/// (`SEMANTIC_CORE.md` §16), so the desugarer lowers them away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `✚`
    Add,
    /// `☠︎` (a different glyph from unary `−`)
    Sub,
    /// `✱`
    Mul,
    /// `÷`
    Div,
    /// `⌗`
    Mod,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `≤`
    Le,
    /// `≥`
    Ge,
    /// `≡`
    Eq,
    /// `≠`
    NotEq,
    /// `∧`: short-circuiting.
    And,
    /// `∨`: short-circuiting.
    Or,
    /// `⊻`: eager, unlike `∧` and `∨`.
    Xor,
}

/// A prefix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// `¬`
    Not,
    /// `−`
    Neg,
}
