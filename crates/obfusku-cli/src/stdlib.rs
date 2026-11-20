//! The standard library as an implicit prelude (`ADR-012`).
//!
//! **Why source, not Rust.** Most of the stdlib is ordinary `.obk` text
//! (`stdlib.obk`), run through the same parse → desugar → check → evaluate
//! pipeline as user code (`IMPLEMENTATION_ARCHITECTURE.md` §15). That keeps
//! the library honest — it can only use what the language itself offers —
//! and leaves the host for what Obfusku cannot express: effects and types
//! with no pattern form, which live in [`crate::natives`].
//!
//! Today the source declares the ambient ADTs `List`, `Optional`, `Result`
//! and the `⌑` directory-entry type; the combinators over them are natives
//! (`natives::collections`) because they call back into user functions.
//!
//! Its whole top-level surface is visible to every program without a `⟲`,
//! through the same prelude machinery imports use. The difference is *which*
//! bindings are exposed: every one, not only `⟳`-exported ones, since there
//! is no other module to keep anything private from.

use crate::project::exports::{export_types, export_values};
use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use obfusku_typecheck::Prelude;
use std::collections::HashMap;

const SOURCE: &str = include_str!("stdlib.obk");

/// Everything the stdlib contributes: type schemes, values, and the ADT
/// declarations behind them.
///
/// The declarations matter as much as the values: a constructor's variant
/// info must be in scope for a program to *pattern-match* on `⊘`/`⁝`, not
/// only call them as functions (see `check_with_prelude_and_types`).
pub type Stdlib = (
    Prelude,
    HashMap<String, Value>,
    Vec<obfusku_core::ast::TypeDecl>,
);

/// Parses, checks and evaluates the stdlib source.
///
/// The text is registered in `source_map` so that any diagnostic it could
/// produce still renders. None is expected from this fixed, verified
/// source, but the contract holds regardless.
pub fn load(source_map: &mut SourceMap) -> Result<Stdlib, Vec<Diagnostic>> {
    let source_id = source_map.add_file(SOURCE);
    let tokens = obfusku_syntax::lexer::tokenize(SOURCE, source_id).map_err(|d| vec![d])?;
    let surface = obfusku_syntax::parser::parse(&tokens, source_id)?;
    let core_module = obfusku_syntax::desugar::desugar(&surface)?;
    let typed = obfusku_typecheck::check(&core_module)?;
    let evaluated = obfusku_runtime::evaluate_with_prelude(&core_module, std::iter::empty())
        .map_err(|d| vec![d])?;

    Ok((
        export_types(&typed, false),
        export_values(evaluated, false),
        core_module.type_decls,
    ))
}
