//! What the embedding host contributes to a run, and what a check yields.

use crate::project::deps::Packages;
use crate::project::manifest::Capabilities;
use crate::project::modules::ModuleSource;
use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use std::collections::HashMap;
use std::path::Path;

/// What the embedding host grants the program.
///
/// The defaults describe an ordinary `obfusku run` of files on disk. A
/// bundled program (`ADR-022`) overrides them: its sources live in memory,
/// its filesystem root is the caller's directory, and its manifest and
/// package graph were captured when it was built.
#[derive(Default)]
pub(super) struct Host<'a> {
    /// Program arguments behind `⌁↓⌥` (`ADR-022`).
    pub args: &'a [String],
    /// Filesystem root for the `⌁` natives; `None` means the Project root.
    pub fs_root: Option<&'a Path>,
    /// Where module text comes from.
    pub sources: ModuleSource<'a>,
    /// Declared by a bundled manifest; for a disk project the manifest at
    /// the project root is read instead (`ADR-029`).
    pub capabilities: Option<Capabilities>,
    /// A bundle's embedded package graph; read from disk otherwise.
    pub packages: Option<Packages>,
}

/// An entry module taken through resolve → parse → import resolution →
/// desugar → type-check, ready to evaluate.
pub(super) struct Checked {
    /// Every file the run touched, so any diagnostic can be rendered.
    pub source_map: SourceMap,
    /// The desugared entry module.
    pub core_module: obfusku_core::ast::Module,
    /// The ambient prelude plus imports, as seeded into the evaluator.
    pub values: HashMap<String, Value>,
    /// Non-fatal diagnostics from the checker.
    pub warnings: Vec<Diagnostic>,
}
