//! Pipeline orchestration — `spec/IMPLEMENTATION_ARCHITECTURE.md` §12.
//! No language logic of its own: every step below is a direct call into
//! the crate that owns that stage.

pub mod bundle;
pub mod bytes_native;
pub mod capability;
pub mod collection_native;
pub mod deps;
pub mod io_native;
pub mod manifest;
pub mod modules;
pub mod native_util;
pub mod natives;
pub mod numeric_native;
pub mod project;
pub mod stdlib;
pub mod surface;
pub mod text_native;

use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use obfusku_typecheck::{Scheme, TypeResult};
use std::path::{Path, PathBuf};

/// `run`'s reported value: the last top-level binding evaluated.
/// Operationally defined as a CLI execution-reporting convention,
/// independent of export visibility (`⟳`).
#[derive(Debug)]
pub struct RunResult(pub Value);

/// The `run` pipeline: parse, desugar, type-check, evaluate.
///
/// Returns the last evaluated binding value alongside any non-fatal
/// diagnostics (such as unreachable match arms).
pub fn run_source(source: &str) -> Result<(RunResult, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let core_module = obfusku_syntax::parse_and_desugar(source, source_id)?;
    let typed = obfusku_typecheck::check(&core_module)?;
    obfusku_runtime::evaluate(&core_module)
        .map(|v| (RunResult(v), typed.warnings))
        .map_err(|d| vec![d])
}

/// The `check` pipeline from §12's table: parse, desugar, type-check —
/// no execution. Returns non-fatal diagnostics (warnings) on success,
/// fatal ones on failure.
pub fn check_source(source: &str) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let core_module = obfusku_syntax::parse_and_desugar(source, source_id)?;
    obfusku_typecheck::check(&core_module).map(|typed| typed.warnings)
}

/// Reads `path`, returning a diagnostic with a default span on I/O error.
fn read_entry_file(sources: modules::ModuleSource, path: &Path) -> Result<String, Diagnostic> {
    sources.read(path).map_err(|e| Diagnostic {
        severity: obfusku_diagnostics::Severity::Error,
        message: format!("could not read '{}': {e}", path.display()),
        primary: obfusku_diagnostics::Span::default(),
    })
}

type Failure = (SourceMap, Vec<Diagnostic>);

/// An entry module taken through resolve → parse → import resolution →
/// desugar → type-check, ready to evaluate.
struct Checked {
    source_map: SourceMap,
    core_module: obfusku_core::ast::Module,
    values: std::collections::HashMap<String, Value>,
    warnings: Vec<Diagnostic>,
}

/// What the embedding host grants the program: its arguments, and the
/// filesystem root for `⌁↓⌬`/`⌁↑⌬` when that is not the Project root
/// (a bundled program reads the caller's directory, `ADR-022`).
#[derive(Default)]
struct Host<'a> {
    args: &'a [String],
    fs_root: Option<&'a Path>,
    sources: modules::ModuleSource<'a>,
    /// Declared by a bundled manifest; for a disk project the manifest at
    /// the project root is read instead (`ADR-029`).
    capabilities: Option<manifest::Capabilities>,
    /// A bundle's embedded package graph; read from disk otherwise.
    packages: Option<deps::Packages>,
}

/// The pipeline shared by `run`, `check` and `build`. `entry` is the
/// already-resolved entry point (or its error), so each command keeps its
/// own entry-point policy (`ADR-017`). Returns the [`SourceMap`] with
/// every failure: a diagnostic can originate in an *imported* file, so
/// rendering needs the map every resolved file was registered in.
fn check_entry(
    entry: Result<project::EntryPoint, Diagnostic>,
    host: &Host,
) -> Result<Checked, Failure> {
    let source_map = SourceMap::new();
    // A `Span::default()` diagnostic needs *some* file in the map to
    // render against; registered only on early-return paths so the entry
    // file's own `SourceId` stays 0 otherwise (`main.rs`'s warning
    // rendering rebuilds its own single-file map and assumes that).
    let early = |mut map: SourceMap, d: Diagnostic| -> Failure {
        map.add_file("");
        (map, vec![d])
    };
    let entry = match entry {
        Ok(e) => e,
        Err(d) => return Err(early(source_map, d)),
    };
    let source = match read_entry_file(host.sources, &entry.entry_file) {
        Ok(s) => s,
        Err(d) => return Err(early(source_map, d)),
    };
    check_text(source_map, &source, &entry.project_root, host)
}

/// The pipeline from source text on: lex → parse → import resolution →
/// desugar → type-check. `source_map` must be fresh so the entry text is
/// `SourceId(0)`. Shared by every entry point, and by the language
/// server for unsaved editor buffers.
fn check_text(
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
    let caps = host.capabilities.or_else(|| {
        matches!(host.sources, modules::ModuleSource::Disk)
            .then(|| manifest::load_manifest(project_root).ok())
            .flatten()
            .and_then(|m| m.capabilities)
    });
    let fs_root = match caps.map(|c| c.fs) {
        Some(manifest::FsAccess::Cwd) => std::env::current_dir().ok(),
        _ => None,
    };
    let fs_root: &Path = fs_root.as_deref().or(host.fs_root).unwrap_or(project_root);
    for (types, vals) in [
        natives::load(fs_root),
        natives::load_process(host.args),
        collection_native::load(),
        numeric_native::load(),
        text_native::load(),
        bytes_native::load(),
    ] {
        type_prelude.extend(types);
        values.extend(vals);
    }
    let packages = match &host.packages {
        Some(p) => p.clone(),
        None if matches!(host.sources, modules::ModuleSource::Disk) => {
            match deps::load_packages(project_root) {
                Ok(p) => p,
                Err(d) => return Err((source_map, vec![d])),
            }
        }
        None => deps::Packages::default(),
    };
    let denied = caps.map(|c| capability::denied(&c)).unwrap_or_default();
    capability::deny_natives(&denied, &mut values);
    let mut cache = std::collections::HashMap::new();
    let mut resolving = std::collections::HashSet::new();
    // An explicit import wins over an ambient stdlib name of the same
    // name — deliberate, not just "whichever inserts last": the user wrote
    // `⟲` for this one, the stdlib name is only ever implicit.
    let ambient = modules::Ambient {
        types: type_prelude.clone(),
        values: values.clone(),
        type_decls: all_type_decls.clone(),
        packages: std::rc::Rc::new(packages),
    };
    let shadowable;
    match modules::resolve_imports(
        &surface,
        project_root,
        host.sources,
        &ambient,
        &mut source_map,
        &mut cache,
        &mut resolving,
    ) {
        Ok((import_types, import_values, import_type_decls)) => {
            shadowable = ambient.shadowable(&import_types);
            type_prelude.extend(import_types);
            values.extend(import_values);
            all_type_decls.extend(import_type_decls);
        }
        Err(ds) => return Err((source_map, ds)),
    }

    let core_module = match obfusku_syntax::desugar::desugar_with_ambient(
        &surface,
        &std::collections::HashSet::new(),
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

/// Diagnostics for `source` as the entry module of the Project containing
/// `path` — the editor-buffer variant of [`check_file`]. Returns the
/// `SourceMap` the spans are relative to and every diagnostic (errors, or
/// on success the warnings).
#[must_use]
pub fn analyze_text(path: &Path, source: &str) -> (SourceMap, Vec<Diagnostic>) {
    let mut source_map = SourceMap::new();
    let root = match project::resolve_entry_point(path) {
        Ok(e) => e.project_root,
        // A buffer not yet saved: check it against its would-be directory.
        Err(_) if !path.exists() => path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        Err(d) => {
            source_map.add_file("");
            return (source_map, vec![d]);
        }
    };
    match check_text(source_map, source, &root, &Host::default()) {
        Ok(checked) => (checked.source_map, checked.warnings),
        Err((map, ds)) => (map, ds),
    }
}

/// Like [`run_source`], but resolves `path`'s own `ImportDeclaration`s
/// first, against the Project root [`project::resolve_entry_point`]
/// discovers (`ADR-017`/`ADR-018`). `path` may be a single `.obk` file or
/// a Project directory. On success also returns `check`'s non-fatal
/// warnings.
pub fn run_file(path: &Path) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    run_file_with_args(path, &[])
}

/// [`run_file`] with program arguments, readable from Obfusku source via
/// `⌁↓⌥` (`ADR-022`).
pub fn run_file_with_args(
    path: &Path,
    args: &[String],
) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    run_entry(
        project::resolve_entry_point(path),
        &Host {
            args,
            ..Host::default()
        },
    )
}

/// Runs a bundle (`bundle::run_embedded`): modules come from `sources`
/// under `project_root`, the filesystem natives are rooted at `fs_root`.
pub(crate) fn run_bundled(
    project_root: &Path,
    entry_file: &Path,
    fs_root: &Path,
    args: &[String],
    sources: modules::ModuleSource,
    capabilities: Option<manifest::Capabilities>,
    packages: deps::Packages,
) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    run_entry(
        Ok(project::EntryPoint {
            project_root: project_root.to_path_buf(),
            entry_file: entry_file.to_path_buf(),
        }),
        &Host {
            args,
            fs_root: Some(fs_root),
            sources,
            capabilities,
            packages: Some(packages),
        },
    )
}

fn run_entry(
    entry: Result<project::EntryPoint, Diagnostic>,
    host: &Host,
) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    let checked = check_entry(entry, host)?;
    match obfusku_runtime::evaluate_with_prelude_traced(&checked.core_module, checked.values) {
        Ok(bindings) => {
            let last = bindings
                .into_iter()
                .last()
                .map_or(Value::Unit, |(_, _, v)| v);
            Ok((RunResult(last), checked.warnings))
        }
        Err(fault) => {
            // The innermost activations first, as notes at their call
            // sites, so the renderer resolves `line:col` like any other.
            let mut diagnostics = vec![fault.diagnostic];
            // Runs of the same call (deep recursion) collapse to one note.
            let mut frames = fault.trace.iter().peekable();
            while let Some(f) = frames.next() {
                let mut times = 1;
                while frames
                    .next_if(|n| n.function == f.function && n.call_site == f.call_site)
                    .is_some()
                {
                    times += 1;
                }
                let repeat = if times > 1 {
                    format!(" ({times} times in a row)")
                } else {
                    String::new()
                };
                diagnostics.push(Diagnostic {
                    severity: obfusku_diagnostics::Severity::Note,
                    message: format!("called '{}' from here{repeat}", f.function),
                    primary: f.call_site,
                });
            }
            Err((checked.source_map, diagnostics))
        }
    }
}

/// Like [`check_source`], but resolves imports first — see [`run_file`].
pub fn check_file(path: &Path) -> Result<Vec<Diagnostic>, Failure> {
    check_entry(project::resolve_entry_point(path), &Host::default()).map(|c| c.warnings)
}

/// The `build` pipeline: identical to [`check_file`] except entry-point
/// resolution requires a real Project (`project::require_project_entry_point`
/// — no bare-file fallback). Success certifies the Project as a valid,
/// distributable Source Artifact (`ADR-017`); nothing is written.
pub fn build_project(path: &Path) -> Result<Vec<Diagnostic>, Failure> {
    check_entry(project::require_project_entry_point(path), &Host::default()).map(|c| c.warnings)
}

/// The `fmt` pipeline from §12's table: `syntax::parse` (surface only,
/// desugaring not needed) → `fmt::format`.
pub fn fmt_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let (tokens, comments) =
        obfusku_syntax::lexer::tokenize_with_comments(source, source_id).map_err(|d| vec![d])?;
    let module = obfusku_syntax::parser::parse(&tokens, source_id)?;
    Ok(obfusku_fmt::format_with_comments(
        &module, &comments, source,
    ))
}

/// Which intermediate representation `inspect_source` prints — §12's
/// `inspect` command's `--tokens`/`--ast`/`--core` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectKind {
    Tokens,
    Ast,
    Core,
}

/// The `inspect` pipeline from §12's table: exposes an intermediate
/// representation via the debug-printable types already public in
/// `obfusku-syntax`/`obfusku-core` — no dedicated pretty-printer of its
/// own, since none is promised by the contract.
pub fn inspect_source(source: &str, kind: InspectKind) -> Result<String, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let tokens = obfusku_syntax::lexer::tokenize(source, source_id).map_err(|d| vec![d])?;
    if kind == InspectKind::Tokens {
        return Ok(format!("{tokens:#?}"));
    }
    let module = obfusku_syntax::parser::parse(&tokens, source_id)?;
    if kind == InspectKind::Ast {
        return Ok(format!("{module:#?}"));
    }
    let core_module = obfusku_syntax::desugar::desugar(&module)?;
    Ok(format!("{core_module:#?}"))
}

/// Renders every diagnostic against a fresh [`SourceMap`] built from
/// `source` — the CLI's own presentation concern, not part of any
/// pipeline function's return value (§12: this crate does presentation,
/// the others don't know rendering exists).
#[must_use]
pub fn render_diagnostics(source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut map = SourceMap::new();
    map.add_file(source);
    render_diagnostics_map(&map, diagnostics)
}

/// Like [`render_diagnostics`], against an already-built [`SourceMap`]
/// (e.g. [`run_file`]/[`check_file`]'s own, which may register more than
/// one file — a diagnostic's `Span` could belong to any of them).
#[must_use]
pub fn render_diagnostics_map(map: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| map.render(d))
        .collect::<Vec<_>>()
        .join("\n")
}

/// An interactive REPL session with incremental evaluation. Each submitted
/// line is parsed, type-checked, and evaluated once against the accumulated
/// environment, so side-effecting operations are never re-executed.
///
/// Initialized with the ambient stdlib and interactive I/O (`print`/`readLine`).
pub struct ReplSession {
    source_map: SourceMap,
    type_prelude: obfusku_typecheck::Prelude,
    value_prelude: std::collections::HashMap<String, Value>,
    type_decls: Vec<obfusku_core::ast::TypeDecl>,
    /// Names a previous `submit` call declared `≔˚` — threaded into the
    /// next call's `desugar_with_ambient_mut_names` so a later line
    /// referencing/rebinding one of them lowers to `MutRead`/`MutRebind`
    /// (as it would within one ordinary file) instead of a bare `Var`
    /// that would try to use the raw `Value::Cell` where its element
    /// type is expected.
    mut_names: std::collections::HashSet<String>,
}

impl ReplSession {
    pub fn new() -> Result<Self, Vec<Diagnostic>> {
        let mut source_map = SourceMap::new();
        let (mut type_prelude, mut value_prelude, type_decls) = stdlib::load(&mut source_map)?;
        for (types, vals) in [natives::load_io_only(), natives::load_process(&[])] {
            type_prelude.extend(types);
            value_prelude.extend(vals);
        }
        let (array_types, array_values) = collection_native::load();
        type_prelude.extend(array_types);
        value_prelude.extend(array_values);
        let (numeric_types, numeric_values) = numeric_native::load();
        type_prelude.extend(numeric_types);
        value_prelude.extend(numeric_values);
        let (text_types, text_values) = text_native::load();
        type_prelude.extend(text_types);
        value_prelude.extend(text_values);
        let (bytes_types, bytes_values) = bytes_native::load();
        type_prelude.extend(bytes_types);
        value_prelude.extend(bytes_values);
        Ok(Self {
            source_map,
            type_prelude,
            value_prelude,
            type_decls,
            mut_names: std::collections::HashSet::new(),
        })
    }

    /// Parses, type-checks, and evaluates a single input line. On success,
    /// declarations become available to subsequent lines. On failure, session
    /// state is preserved. Returns the evaluated value and any warnings.
    pub fn submit(&mut self, line: &str) -> Result<(Value, Vec<Diagnostic>), Vec<Diagnostic>> {
        let candidate = format!("{line}\n\u{2767}\n");
        let source_id = self.source_map.add_file(&candidate);
        let tokens = obfusku_syntax::lexer::tokenize(&candidate, source_id).map_err(|d| vec![d])?;
        let surface = obfusku_syntax::parser::parse(&tokens, source_id)?;
        let core_module = obfusku_syntax::desugar::desugar_with_ambient(
            &surface,
            &self.mut_names,
            &self.type_decls,
        )?;
        let typed = obfusku_typecheck::check_with_prelude_and_types(
            &core_module,
            self.type_prelude.clone(),
            self.type_decls.clone(),
        )?;
        let bindings =
            obfusku_runtime::evaluate_with_prelude(&core_module, self.value_prelude.clone())
                .map_err(|d| vec![d])?;

        let last = bindings.last().map_or(Value::Unit, |(_, _, v)| v.clone());

        for (name, _exported, v) in bindings {
            if !modules::BUILTIN_EXPORTS.contains(&name.as_str()) {
                self.value_prelude.insert(name, v);
            }
        }
        for tb in typed.bindings {
            if modules::BUILTIN_EXPORTS.contains(&tb.name.as_str()) {
                continue;
            }
            let scheme = match tb.ty {
                TypeResult::Monomorphic(core_ty) => {
                    Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
                        &core_ty,
                        &std::collections::HashMap::new(),
                    ))
                }
                TypeResult::Polymorphic(scheme) => scheme,
            };
            self.type_prelude.insert(tb.name, scheme);
        }
        for group in &core_module.bindings {
            if let obfusku_core::ast::BindingGroup::Let(b) = group
                && matches!(b.value, obfusku_core::ast::Expr::MutCell { .. })
            {
                self.mut_names.insert(b.name.clone());
            }
        }
        self.type_decls.extend(core_module.type_decls);

        Ok((last, typed.warnings))
    }

    /// Renders diagnostics from a failed [`submit`] call against this
    /// session's own accumulated `SourceMap` — every line ever submitted
    /// (accepted or not) is registered in it, so a span from any of them
    /// renders correctly.
    #[must_use]
    pub fn render(&self, diagnostics: &[Diagnostic]) -> String {
        render_diagnostics_map(&self.source_map, diagnostics)
    }
}
