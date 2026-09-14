//! Lexer, parser, surface AST, and desugaring to Core AST —
//! `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` and `spec/ABSTRACT_GRAMMAR.md`.
//!
//! This is the frontend boundary future editor/LSP tooling is meant to
//! depend on directly, without pulling in `obfusku-cli` or
//! `obfusku-runtime`. See `spec/IMPLEMENTATION_ARCHITECTURE.md` §7.

pub mod ast;
pub mod desugar;
pub mod lexer;
pub mod parser;

use obfusku_diagnostics::{Diagnostic, SourceId};

/// Source text all the way to Core AST — `lexer::tokenize` →
/// `parser::parse` → `desugar::desugar`, in one call. Each step remains
/// independently callable (`obfusku-fmt` stops after `parser::parse`).
pub fn parse_and_desugar(
    source: &str,
    source_id: SourceId,
) -> Result<obfusku_core::ast::Module, Vec<Diagnostic>> {
    let tokens = lexer::tokenize(source, source_id).map_err(|d| vec![d])?;
    let surface = parser::parse(&tokens, source_id)?;
    desugar::desugar(&surface)
}
