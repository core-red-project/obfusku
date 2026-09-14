//! Pipeline orchestration — `spec/IMPLEMENTATION_ARCHITECTURE.md` §12.
//! No language logic of its own: every step below is a direct call into
//! the crate that owns that stage.

pub mod modules;
pub mod natives;
pub mod stdlib;

use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use obfusku_typecheck::{Scheme, TypeResult};
use std::path::Path;

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
fn read_entry_file(path: &Path) -> Result<String, Diagnostic> {
    std::fs::read_to_string(path).map_err(|e| Diagnostic {
        severity: obfusku_diagnostics::Severity::Error,
        message: format!("could not read '{}': {e}", path.display()),
        primary: obfusku_diagnostics::Span::default(),
    })
}

/// Like [`run_source`], but resolves `path`'s own `ImportDeclaration`s
/// first (same-directory `<module_name>.obk` files, per
/// [`modules::resolve_module`]) before type-checking/evaluating `path`
/// itself against them.
///
/// Returns the [`SourceMap`] alongside any diagnostics: a diagnostic can
/// originate in an *imported* file, so rendering it needs the shared map
/// every resolved file was registered in, not just `path`'s own text —
/// see `modules`' own doc comment. On success, also returns `check`'s
/// non-fatal warnings — see [`run_source`].
pub fn run_file(path: &Path) -> Result<(RunResult, Vec<Diagnostic>), (SourceMap, Vec<Diagnostic>)> {
    let mut source_map = SourceMap::new();
    let source = match read_entry_file(path) {
        Ok(s) => s,
        Err(d) => return Err((source_map, vec![d])),
    };
    let source_id = source_map.add_file(&source);
    let tokens = match obfusku_syntax::lexer::tokenize(&source, source_id) {
        Ok(t) => t,
        Err(d) => return Err((source_map, vec![d])),
    };
    let surface = match obfusku_syntax::parser::parse(&tokens, source_id) {
        Ok(m) => m,
        Err(ds) => return Err((source_map, ds)),
    };

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let (mut type_prelude, mut value_prelude, mut all_type_decls) =
        match stdlib::load(&mut source_map) {
            Ok(p) => p,
            Err(ds) => return Err((source_map, ds)),
        };
    let (native_types, native_values) = natives::load(base_dir);
    type_prelude.extend(native_types);
    value_prelude.extend(native_values);
    let mut cache = std::collections::HashMap::new();
    let mut resolving = std::collections::HashSet::new();
    match modules::resolve_imports(
        &surface,
        base_dir,
        &mut source_map,
        &mut cache,
        &mut resolving,
    ) {
        // An explicit import wins over an ambient stdlib name of the
        // same name — deliberate, not just "whichever inserts last":
        // the user wrote `⟲` for this one, the stdlib name is only
        // ever implicit.
        Ok((import_types, import_values, import_type_decls)) => {
            type_prelude.extend(import_types);
            value_prelude.extend(import_values);
            all_type_decls.extend(import_type_decls);
        }
        Err(ds) => return Err((source_map, ds)),
    };

    let core_module = match obfusku_syntax::desugar::desugar(&surface) {
        Ok(m) => m,
        Err(ds) => return Err((source_map, ds)),
    };
    let typed = match obfusku_typecheck::check_with_prelude_and_types(
        &core_module,
        type_prelude,
        all_type_decls,
    ) {
        Ok(t) => t,
        Err(ds) => return Err((source_map, ds)),
    };
    match obfusku_runtime::evaluate_with_prelude(&core_module, value_prelude) {
        Ok(bindings) => {
            let last = bindings
                .into_iter()
                .last()
                .map(|(_, _, v)| v)
                .unwrap_or(Value::Unit);
            Ok((RunResult(last), typed.warnings))
        }
        Err(d) => Err((source_map, vec![d])),
    }
}

/// Like [`check_source`], but resolves imports first — see [`run_file`].
pub fn check_file(path: &Path) -> Result<Vec<Diagnostic>, (SourceMap, Vec<Diagnostic>)> {
    let mut source_map = SourceMap::new();
    let source = match read_entry_file(path) {
        Ok(s) => s,
        Err(d) => return Err((source_map, vec![d])),
    };
    let source_id = source_map.add_file(&source);
    let tokens = match obfusku_syntax::lexer::tokenize(&source, source_id) {
        Ok(t) => t,
        Err(d) => return Err((source_map, vec![d])),
    };
    let surface = match obfusku_syntax::parser::parse(&tokens, source_id) {
        Ok(m) => m,
        Err(ds) => return Err((source_map, ds)),
    };

    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let (mut type_prelude, _, mut all_type_decls) = match stdlib::load(&mut source_map) {
        Ok(p) => p,
        Err(ds) => return Err((source_map, ds)),
    };
    let (native_types, _) = natives::load(base_dir);
    type_prelude.extend(native_types);
    let mut cache = std::collections::HashMap::new();
    let mut resolving = std::collections::HashSet::new();
    match modules::resolve_imports(
        &surface,
        base_dir,
        &mut source_map,
        &mut cache,
        &mut resolving,
    ) {
        Ok((import_types, _, import_type_decls)) => {
            type_prelude.extend(import_types);
            all_type_decls.extend(import_type_decls);
        }
        Err(ds) => return Err((source_map, ds)),
    };

    let core_module = match obfusku_syntax::desugar::desugar(&surface) {
        Ok(m) => m,
        Err(ds) => return Err((source_map, ds)),
    };
    match obfusku_typecheck::check_with_prelude_and_types(
        &core_module,
        type_prelude,
        all_type_decls,
    ) {
        Ok(typed) => Ok(typed.warnings),
        Err(ds) => Err((source_map, ds)),
    }
}

/// The `fmt` pipeline from §12's table: `syntax::parse` (surface only,
/// desugaring not needed) → `fmt::format`.
pub fn fmt_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let tokens = obfusku_syntax::lexer::tokenize(source, source_id).map_err(|d| vec![d])?;
    let module = obfusku_syntax::parser::parse(&tokens, source_id)?;
    Ok(obfusku_fmt::format(&module))
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
pub fn render_diagnostics(source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut map = SourceMap::new();
    map.add_file(source);
    render_diagnostics_map(&map, diagnostics)
}

/// Like [`render_diagnostics`], against an already-built [`SourceMap`]
/// (e.g. [`run_file`]/[`check_file`]'s own, which may register more than
/// one file — a diagnostic's `Span` could belong to any of them).
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
        let (io_types, io_values) = natives::load_io_only();
        type_prelude.extend(io_types);
        value_prelude.extend(io_values);
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
        let core_module =
            obfusku_syntax::desugar::desugar_with_ambient_mut_names(&surface, &self.mut_names)?;
        let typed = obfusku_typecheck::check_with_prelude_and_types(
            &core_module,
            self.type_prelude.clone(),
            self.type_decls.clone(),
        )?;
        let bindings =
            obfusku_runtime::evaluate_with_prelude(&core_module, self.value_prelude.clone())
                .map_err(|d| vec![d])?;

        let last = bindings
            .last()
            .map(|(_, _, v)| v.clone())
            .unwrap_or(Value::Unit);

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
            if let obfusku_core::ast::BindingGroup::Let(b) = group {
                if matches!(b.value, obfusku_core::ast::Expr::MutCell { .. }) {
                    self.mut_names.insert(b.name.clone());
                }
            }
        }
        self.type_decls.extend(core_module.type_decls);

        Ok((last, typed.warnings))
    }

    /// Renders diagnostics from a failed [`submit`] call against this
    /// session's own accumulated `SourceMap` — every line ever submitted
    /// (accepted or not) is registered in it, so a span from any of them
    /// renders correctly.
    pub fn render(&self, diagnostics: &[Diagnostic]) -> String {
        render_diagnostics_map(&self.source_map, diagnostics)
    }
}
