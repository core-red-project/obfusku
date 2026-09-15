//! Document analysis: the same pipeline as `obfusku_cli::check_file`
//! (`spec/IMPLEMENTATION_ARCHITECTURE.md` §12), except the entry file's
//! text comes from the editor buffer rather than disk, so diagnostics
//! reflect unsaved edits.

use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range};
use obfusku_cli::{array_native, modules, natives, numeric_native, project, stdlib};
use obfusku_diagnostics::{Diagnostic, Severity, SourceMap};
use std::path::Path;

/// Runs parse → desugar → type-check for `source`, resolving imports and
/// the stdlib prelude against `path`'s project root. Returns the
/// `SourceMap` every diagnostic's span is relative to, alongside all
/// diagnostics (fatal errors or, on success, non-fatal warnings).
pub fn analyze(path: &Path, source: &str) -> (SourceMap, Vec<Diagnostic>) {
    let mut source_map = SourceMap::new();

    let entry = match project::resolve_entry_point(path) {
        Ok(e) => e,
        Err(d) => {
            source_map.add_file("");
            return (source_map, vec![d]);
        }
    };
    let source_id = source_map.add_file(source);

    let tokens = match obfusku_syntax::lexer::tokenize(source, source_id) {
        Ok(t) => t,
        Err(d) => return (source_map, vec![d]),
    };
    let surface = match obfusku_syntax::parser::parse(&tokens, source_id) {
        Ok(m) => m,
        Err(ds) => return (source_map, ds),
    };

    let project_root = entry.project_root.as_path();
    let (mut type_prelude, _, mut all_type_decls) = match stdlib::load(&mut source_map) {
        Ok(p) => p,
        Err(ds) => return (source_map, ds),
    };
    let (native_types, _) = natives::load(project_root);
    type_prelude.extend(native_types);
    let (array_types, _) = array_native::load();
    type_prelude.extend(array_types);
    let (numeric_types, _) = numeric_native::load();
    type_prelude.extend(numeric_types);

    let mut cache = std::collections::HashMap::new();
    let mut resolving = std::collections::HashSet::new();
    match modules::resolve_imports(
        &surface,
        project_root,
        &mut source_map,
        &mut cache,
        &mut resolving,
    ) {
        Ok((import_types, _, import_type_decls)) => {
            type_prelude.extend(import_types);
            all_type_decls.extend(import_type_decls);
        }
        Err(ds) => return (source_map, ds),
    }

    let core_module = match obfusku_syntax::desugar::desugar(&surface) {
        Ok(m) => m,
        Err(ds) => return (source_map, ds),
    };
    match obfusku_typecheck::check_with_prelude_and_types(
        &core_module,
        type_prelude,
        all_type_decls,
    ) {
        Ok(typed) => (source_map, typed.warnings),
        Err(ds) => (source_map, ds),
    }
}

/// Converts Obfusku diagnostics (byte-offset spans against `source_map`)
/// into LSP diagnostics (0-indexed UTF-16-ish line/column, per the LSP
/// spec) for the file identified by `source_map`'s first entry — the
/// entry file `analyze` always registers first.
pub fn to_lsp_diagnostics(
    source_map: &SourceMap,
    diagnostics: &[Diagnostic],
) -> Vec<LspDiagnostic> {
    diagnostics
        .iter()
        .map(|d| {
            let (start, end) = source_map.span_location(d.primary);
            LspDiagnostic {
                range: Range {
                    start: Position::new(
                        start.line.saturating_sub(1),
                        start.column.saturating_sub(1),
                    ),
                    end: Position::new(end.line.saturating_sub(1), end.column.saturating_sub(1)),
                },
                severity: Some(match d.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Note => DiagnosticSeverity::INFORMATION,
                }),
                source: Some("obfusku".to_string()),
                message: d.message.clone(),
                ..Default::default()
            }
        })
        .collect()
}
