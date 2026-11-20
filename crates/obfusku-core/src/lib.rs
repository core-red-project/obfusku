//! The Core: the frozen vocabulary every other stage speaks.
//!
//! `obfusku-syntax` produces it, `obfusku-typecheck` and `obfusku-runtime`
//! consume it, and nothing here knows about any of them. That one-way
//! dependency is the point of a core: the surface language can grow sugar
//! freely (it desugars *into* this), while the checker and the evaluator —
//! the parts that must be correct — only ever handle the small set of forms
//! defined here (`spec/SEMANTIC_CORE.md`, `spec/IMPLEMENTATION_ARCHITECTURE.md`
//! §9).
//!
//! # Contents
//!
//! - [`ast`] — expressions, patterns, bindings, declarations and the
//!   [`ast::Module`] that holds them (`SEMANTIC_CORE.md` §1, §9–§11);
//! - [`types`] — the concrete, variable-free type vocabulary (§3, §12.1);
//! - [`exception`] — the built-in `Exception` variants every program has (§15.2).
//!
//! Besides the data, the crate offers the operations that are *about* the
//! data and so have exactly one right implementation: [`ast::Expr::children`]
//! and [`ast::Expr::walk`] for passes that only visit, [`ast::Pattern::binders`]
//! for the order binders are bound in, [`ast::Literal::ty`], the operator
//! typing families ([`ast::OpClass`]), and the record helpers on
//! [`ast::TypeDecl`].
//!
//! No parsing, no execution and no glyphs: source spelling belongs to the
//! surface, meaning belongs here.

#![warn(missing_docs)]

pub mod ast;
pub mod exception;
pub mod types;
