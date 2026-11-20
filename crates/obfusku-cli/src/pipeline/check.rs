//! Taking an entry module from text to a checked, ready-to-run [`Checked`].
//!
//! One path serves every consumer — `run`, `check`, `build`, bundles and the
//! language server's unsaved editor buffers — so they cannot disagree about
//! what a program means. The order is fixed by what each step needs:
//!
//! 1. lex and parse the entry text (syntax errors stop here);
//! 2. assemble the ambient prelude: stdlib, then natives filtered by the
//!    manifest's capabilities (`ADR-029`);
//! 3. resolve imports, each module through the same pipeline
//!    (`project::modules`);
//! 4. desugar, then reject uses of denied natives in the entry module;
//! 5. type-check with the ambient names shadowable only where `ADR-028`
//!    allows.

use super::host::{Checked, Host};
use crate::natives::{self, capability};
use crate::project::deps::{self, Packages};
use crate::project::entry::EntryPoint;
use crate::project::manifest::{self, FsAccess};
use crate::project::modules::{self, Ambient, ModuleSource};
use crate::report::host_error::read_error;
use crate::stdlib;
use obfusku_diagnostics::{Diagnostic, SourceMap};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A failed pipeline: the map every diagnostic's span refers to, and the
/// diagnostics themselves.
pub type Failure = (SourceMap, Vec<Diagnostic>);

/// The pipeline shared by `run`, `check` and `build`.
///
/// `entry` is the already-resolved entry point (or its error), so each
/// command keeps its own entry-point policy (`ADR-017`). The returned
/// [`SourceMap`] accompanies every failure: a diagnostic can originate in an
/// *imported* file, so rendering needs the map every resolved file was
/// registered in.
pub(super) fn check_entry(
    entry: Result<EntryPoint, Diagnostic>,
    host: &Host,
) -> Result<Checked, Failure> {
    // A `Span::default()` diagnostic needs *some* file in the map to render
    // against. It is registered only on these early exits so the entry
    // file's own `SourceId` stays 0 otherwise — the CLI's warning rendering
    // rebuilds a single-file map and relies on that.
    let early = |mut map: SourceMap, d: Diagnostic| -> Failure {
        map.add_file("");
        (map, vec![d])
    };
    let source_map = SourceMap::new();
    let entry = match entry {
        Ok(e) => e,
        Err(d) => return Err(early(source_map, d)),
    };
    let source = match host.sources.read(&entry.entry_file) {
        Ok(s) => s,
        Err(e) => return Err(early(source_map, read_error(&entry.entry_file, &e))),
    };
    check_text(source_map, &source, &entry.project_root, host)
}

/// Capabilities in force: a bundle's own, else the manifest at the project
/// root, but only for files on disk (an in-memory source has no root).
fn capabilities_of(host: &Host, project_root: &Path) -> Option<manifest::Capabilities> {
    host.capabilities.or_else(|| {
        matches!(host.sources, ModuleSource::Disk)
            .then(|| manifest::load_manifest(project_root).ok())
            .flatten()
            .and_then(|m| m.capabilities)
    })
}

/// Packages visible to imports: a bundle's embedded graph, else the one
/// read from disk, else none.
fn packages_of(host: &Host, project_root: &Path) -> Result<Packages, Diagnostic> {
    match &host.packages {
        Some(p) => Ok(p.clone()),
        None if matches!(host.sources, ModuleSource::Disk) => deps::load_packages(project_root),
        None => Ok(Packages::default()),
    }
}

/// The pipeline from source text on: lex → parse → import resolution →
/// desugar → type-check. `source_map` must be fresh so the entry text is
/// `SourceId(0)`.
pub(super) fn check_text(
    mut source_map: SourceMap,
    source: &str,
    project_root: &Path,
    host: &Host,
) -> Result<Checked, Failure> {
    let source_id = source_map.add_file(source);
    let tokens = match obfusku_syntax::lexer::tokenize(source, source_id) {
        Ok(t) => t,
        Err(d) => return Err((source_map, vec![d])),
    };
    let surface = match obfusku_syntax::parser::parse(&tokens, source_id) {
        Ok(m) => m,
        Err(ds) => return Err((source_map, ds)),
    };

    let (mut type_prelude, mut values, mut all_type_decls) = match stdlib::load(&mut source_map) {
        Ok(p) => p,
        Err(ds) => return Err((source_map, ds)),
    };

    let caps = capabilities_of(host, project_root);
    // `fs = "cwd"` widens the filesystem root from the Project to the
    // caller's working directory; any other setting keeps the host's choice.
    let cwd_root = match caps.map(|c| c.fs) {
        Some(FsAccess::Cwd) => std::env::current_dir().ok(),
        _ => None,
    };
    let fs_root: &Path = cwd_root.as_deref().or(host.fs_root).unwrap_or(project_root);
    let (native_types, native_values) = natives::load(Some(fs_root), host.args);
    type_prelude.extend(native_types);
    values.extend(native_values);

    let packages = match packages_of(host, project_root) {
        Ok(p) => p,
        Err(d) => return Err((source_map, vec![d])),
    };
    let denied = caps.map(|c| capability::denied(&c)).unwrap_or_default();
    capability::deny_natives(&denied, &mut values);

    // An explicit import wins over an ambient stdlib name of the same name.
    // That is deliberate rather than "whichever inserts last": the author
    // wrote `⟲` for this one, while the stdlib name is only ever implicit.
    let ambient = Ambient {
        types: type_prelude.clone(),
        values: values.clone(),
        type_decls: all_type_decls.clone(),
        packages: std::rc::Rc::new(packages),
    };
    let (import_types, import_values, import_type_decls) = match modules::resolve_imports(
        &surface,
        project_root,
        host.sources,
        &ambient,
        &mut source_map,
        &mut HashMap::new(),
        &mut HashSet::new(),
    ) {
        Ok(imports) => imports,
        Err(ds) => return Err((source_map, ds)),
    };
    let shadowable = ambient.shadowable(&import_types);
    type_prelude.extend(import_types);
    values.extend(import_values);
    all_type_decls.extend(import_type_decls);

    let core_module = match obfusku_syntax::desugar::desugar_with_ambient(
        &surface,
        &HashSet::new(),
        &all_type_decls,
    ) {
        Ok(m) => m,
        Err(ds) => return Err((source_map, ds)),
    };
    let denied_uses = capability::denied_uses(&core_module, &denied);
    if !denied_uses.is_empty() {
        return Err((source_map, denied_uses));
    }
    match obfusku_typecheck::check_shadowing(
        &core_module,
        type_prelude,
        &all_type_decls,
        &shadowable,
    ) {
        Ok(typed) => Ok(Checked {
            source_map,
            core_module,
            values,
            warnings: typed.warnings,
        }),
        Err(ds) => Err((source_map, ds)),
    }
}
