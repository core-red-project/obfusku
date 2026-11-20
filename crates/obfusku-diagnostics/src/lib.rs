//! Source locations and diagnostics: the vocabulary every stage uses to say
//! *where* and *what went wrong*.
//!
//! **Why a leaf crate.** Nearly every other crate depends on this one, and it
//! depends on none (`IMPLEMENTATION_ARCHITECTURE.md` §14). Position and error
//! types shared by the lexer, parser, checker, evaluator and tools would
//! otherwise force a dependency cycle or a duplicate definition. Keeping it
//! small, std-only and stable is the point; nothing language-specific belongs
//! here.
//!
//! # Map
//!
//! | Module | Types | Role |
//! |---|---|---|
//! | `span` | [`SourceId`], [`Span`] | a byte range in one file, cheap to store on every AST node |
//! | `diagnostic` | [`Severity`], [`Diagnostic`] | the one error shape that crosses crate boundaries |
//! | `source_map` | [`SourceMap`], [`LineCol`] | owns the texts and converts bytes to line/column |
//!
//! # Flow
//!
//! Stages record only a [`Span`] (bytes, no text) while they run. A
//! [`Diagnostic`] carries that span plus a message. Only at the very end does
//! a consumer ask the [`SourceMap`] to turn it into a line and column, so the
//! expensive, glyph-aware conversion happens once per reported message rather
//! than once per token.

#![warn(missing_docs)]

mod diagnostic;
mod source_map;
mod span;

pub use diagnostic::{Diagnostic, Severity};
pub use source_map::{LineCol, SourceMap};
pub use span::{SourceId, Span};
