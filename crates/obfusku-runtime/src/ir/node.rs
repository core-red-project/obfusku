//! The compiled tree the evaluator walks.

use crate::value::Value;
use obfusku_core::ast::{BinOp, UnOp};
use obfusku_diagnostics::Span;
use std::rc::Rc;

use super::GlobalCell;

/// One compiled expression. Mirrors `obfusku_core::ast::Expr` with names
/// resolved away; each variant keeps the [`Span`] of the places that can
/// fail at run time, so a runtime error can point at source.
///
/// Subtrees that a closure or a branch re-enters (`body`, `handler`, arm
/// results) are `Rc<Node>`, so entering them is a pointer clone and the
/// trampoline can hold onto one without borrowing the original tree.
pub enum Node {
    /// A local variable: `depth` frames up, `slot` within that frame.
    Local {
        /// Frames to walk up the environment chain.
        depth: u32,
        /// Index within the frame.
        slot: u32,
    },
    /// A top-level or prelude name.
    Global(Rc<GlobalCell>, Span),
    /// A name that resolved to nothing. Kept (instead of failing compile) so
    /// it errors only if evaluated, as the checker would normally have
    /// rejected it earlier.
    Unbound(Rc<str>, Span),
    /// A literal value.
    Lit(Value),
    /// `λ`: builds a closure over the current environment.
    Lambda {
        /// The body, run with its parameter pushed on the captured chain.
        body: Rc<Node>,
        /// The name it was bound to, for call traces; `λ` if anonymous.
        name: Rc<str>,
    },
    /// Function application.
    Apply {
        /// The callee.
        func: Box<Node>,
        /// The single argument (application is curried).
        arg: Box<Node>,
        /// The call site, recorded in call traces.
        span: Span,
    },
    /// `˚ initial`: allocates a mutable cell.
    MutCell(Box<Node>),
    /// Reads a cell's current value.
    MutRead(Box<Node>, Span),
    /// `cell ⚙︎ value`: overwrites a cell.
    MutRebind {
        /// Evaluates to the cell.
        cell: Box<Node>,
        /// Evaluates to the new contents.
        value: Box<Node>,
        /// Where a non-cell target is reported.
        span: Span,
    },
    /// Builds an ADT value.
    Constructor {
        /// The interned constructor tag.
        tag: Rc<str>,
        /// The field expressions, in order.
        args: Vec<Node>,
    },
    /// `⟡`: pattern match, first matching arm wins.
    Match {
        /// The value matched on.
        scrutinee: Box<Node>,
        /// The arms, tried in order.
        arms: Vec<Arm>,
    },
    /// `☄`: raises an exception.
    Raise(Box<Node>, Span),
    /// `☊`: runs `body`, and on a raise runs `handler` with the payload.
    Catch {
        /// The protected expression.
        body: Box<Node>,
        /// Run with the raised value pushed on the environment.
        handler: Rc<Node>,
    },
    /// A binary operator.
    Binary {
        /// The operator.
        op: BinOp,
        /// Left operand, evaluated first.
        lhs: Box<Node>,
        /// Right operand, always evaluated (no short circuit).
        rhs: Box<Node>,
        /// Where the operation's failure (overflow, division) is reported.
        span: Span,
    },
    /// A unary operator.
    Unary {
        /// The operator.
        op: UnOp,
        /// The operand.
        operand: Box<Node>,
        /// Where a failure (negating `i64::MIN`) is reported.
        span: Span,
    },
    /// A single local binding scoping over `body`.
    Let {
        /// The bound expression.
        value: Box<Node>,
        /// Run with the value pushed on the environment.
        body: Rc<Node>,
    },
    /// A mutually recursive local group.
    LetRec {
        /// The members' expressions; member `i` fills slot `i`.
        values: Vec<Node>,
        /// Run with the filled frame on the environment.
        body: Rc<Node>,
    },
    /// An array literal.
    Array(Vec<Node>),
    /// `array[index]`.
    Index {
        /// The array.
        array: Box<Node>,
        /// The position.
        index: Box<Node>,
        /// Where an out-of-range index is reported.
        span: Span,
    },
    /// `base.name`.
    Field {
        /// The record.
        base: Box<Node>,
        /// The field name.
        name: String,
        /// Where a missing field is reported.
        span: Span,
    },
    /// `base.{ name: value, … }`.
    Update {
        /// The record being copied.
        base: Box<Node>,
        /// The replaced fields, in written order.
        fields: Vec<(String, Node)>,
        /// Where a missing field is reported.
        span: Span,
    },
}

/// One compiled match arm.
pub struct Arm {
    /// What the arm matches.
    pub pattern: Pat,
    /// How many variables the pattern binds. A frame is pushed only when
    /// this is non-zero.
    pub vars: u32,
    /// The arm's result, run in tail position.
    pub result: Rc<Node>,
}

/// A compiled pattern. Variable names are gone: the `n`-th binder (left to
/// right) fills slot `n` of the arm's frame.
pub enum Pat {
    /// `_`: matches anything, binds nothing.
    Wild,
    /// A variable: matches anything and binds it.
    Var,
    /// A literal, compared with structural equality.
    Lit(Value),
    /// A constructor with sub-patterns.
    Con {
        /// The interned constructor tag.
        tag: Rc<str>,
        /// One sub-pattern per field.
        args: Vec<Pat>,
    },
}
