//! The surface AST: what the parser builds, exactly as the author wrote it
//! (`spec/ABSTRACT_GRAMMAR.md`, `spec/CONCRETE_SYMBOLIC_GRAMMAR.md`).
//!
//! **Why a second tree.** The Core AST is deliberately tiny; the surface
//! language is not. This tree keeps every form the author can write (`∧`,
//! `▷` pipes, partial-application holes, named records, local function
//! runs) and a [`Span`](obfusku_diagnostics::Span) on each, so that tools
//! working on the *source* (the formatter, error messages) can reproduce what
//! was written. Lowering to Core is the desugarer's job; losing information
//! there is its purpose, not this tree's.
//!
//! | Module | Contents |
//! |---|---|
//! | `declaration` | [`Module`], [`Declaration`] and the declaration forms |
//! | `type_ref` | type annotations: [`TypeRef`], [`BaseType`] |
//! | `expression` | [`Expression`], [`Argument`], [`Stage`], [`Parameter`] |
//! | `operator` | [`BinOp`], [`UnOp`] |
//! | `pattern` | [`Pattern`], [`MatchArm`] |

mod declaration;
mod expression;
mod operator;
mod pattern;
mod type_ref;

pub use declaration::{
    Declaration, FnParameter, FunctionDeclaration, ImportDeclaration, Module, TypeDeclaration,
    ValueDeclaration, VariantDecl,
};
pub use expression::{Argument, Expression, Parameter, Stage};
pub use operator::{BinOp, UnOp};
pub use pattern::{MatchArm, Pattern};
pub use type_ref::{BaseType, TypeRef};
