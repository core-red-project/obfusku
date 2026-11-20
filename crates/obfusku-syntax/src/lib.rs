//! The language front end: source text to Core AST.
//!
//! ```text
//! source ──lexer──▶ tokens ──parser──▶ surface AST ──desugar──▶ Core AST
//!                                           │
//!                                           └──▶ obfusku-fmt (stops here)
//! ```
//!
//! (`spec/CONCRETE_SYMBOLIC_GRAMMAR.md`, `spec/ABSTRACT_GRAMMAR.md`.)
//!
//! **Why the stages stay separate.** Each is callable on its own. The
//! formatter needs the surface AST and must not lose sugar, so it stops after
//! parsing; tools that only want diagnostics for a fragment can stop earlier
//! still. Merging the stages would force every consumer through the lossy
//! last one.
//!
//! **A boundary for tooling.** This crate depends only on `obfusku-core` and
//! `obfusku-diagnostics`, so an editor or language server can parse without
//! pulling in the checker, the runtime or the CLI
//! (`IMPLEMENTATION_ARCHITECTURE.md` §7). It also does no file I/O: resolving
//! `⟲` imports is the CLI's job.
//!
//! | Module | Role |
//! |---|---|
//! | [`lexer`] | text to tokens, plus the comments the parser never sees |
//! | [`ast`] | the surface tree, with every sugar form and every span |
//! | [`parser`] | tokens to surface AST, failing at the first syntax error |
//! | [`desugar`] | surface AST to Core, collecting independent errors |

#![warn(missing_docs)]

pub mod ast;
pub mod desugar;
pub mod lexer;
pub mod parser;

use obfusku_diagnostics::{Diagnostic, SourceId};

/// Source text all the way to Core AST: [`lexer::tokenize`], then
/// [`parser::parse`], then [`desugar::desugar`], in one call.
///
/// A lexical error is the only diagnostic returned in that case (the lexer
/// stops at its first); a syntax error likewise; desugaring may return
/// several.
pub fn parse_and_desugar(
    source: &str,
    source_id: SourceId,
) -> Result<obfusku_core::ast::Module, Vec<Diagnostic>> {
    let tokens = lexer::tokenize(source, source_id).map_err(|d| vec![d])?;
    let surface = parser::parse(&tokens, source_id)?;
    desugar::desugar(&surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Result<obfusku_core::ast::Module, Vec<Diagnostic>> {
        parse_and_desugar(source, SourceId(0))
    }

    #[test]
    fn source_goes_all_the_way_to_core() {
        let module = lower("x ≔ 1 ✚ 2\n❧").unwrap();
        assert!(module.bound_names().any(|n| n == "x"));
    }

    #[test]
    fn a_lexical_error_is_one_diagnostic() {
        assert_eq!(lower("x ≔ @\n❧").unwrap_err().len(), 1);
    }

    #[test]
    fn a_syntax_error_stops_before_desugaring() {
        let errs = lower("x ≔ ◈ )\n❧").unwrap_err();
        assert!(errs[0].message.contains("expected '❧'") || errs[0].message.contains("expected"));
    }

    #[test]
    fn a_desugaring_error_can_report_several() {
        assert_eq!(lower("a ≔ ◈\nb ≔ ◈\n❧").unwrap_err().len(), 2);
    }
}
