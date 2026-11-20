//! Checking an editor buffer.

use crate::convert::{SOURCE_NAME, to_lsp_diagnostics};
use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity};
use std::path::Path;

/// The LSP diagnostics for `source`, the text of the buffer at `path`.
///
/// It is the pipeline of `obfusku_cli::check_file`
/// (`IMPLEMENTATION_ARCHITECTURE.md` §12) with one difference: the entry
/// file's text comes from the editor, so results reflect unsaved edits, and
/// `path` only locates the Project root for imports and the stdlib prelude.
/// A buffer that is not on disk yet is therefore checked, not reported
/// unreadable.
///
/// **Panic isolation.** The checker is large and the server holds every open
/// file; a bug on one input must not take them all down. A panic becomes one
/// diagnostic asking for a report, and the server keeps running.
pub(crate) fn diagnostics_for(path: &Path, source: &str) -> Vec<LspDiagnostic> {
    std::panic::catch_unwind(|| {
        let (source_map, diagnostics) = obfusku_cli::analyze_text(path, source);
        to_lsp_diagnostics(&source_map, &diagnostics)
    })
    .unwrap_or_else(|_| vec![internal_error()])
}

fn internal_error() -> LspDiagnostic {
    LspDiagnostic {
        message: "internal error: the checker panicked on this file; please report it".to_string(),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some(SOURCE_NAME.to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsaved(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("obk-lsp-{name}-does-not-exist.obk"))
    }

    #[test]
    fn a_buffer_not_yet_on_disk_is_checked_not_reported_unreadable() {
        let out = diagnostics_for(&unsaved("clean"), "x ≔ 1\n❧\n");
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_type_error_is_reported_with_its_position() {
        let out = diagnostics_for(&unsaved("bad"), "x ≔ 1 ✚ \"a\"\n❧\n");
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(out[0].range.start.line, 0);
    }

    #[test]
    fn a_syntax_error_is_reported() {
        let out = diagnostics_for(&unsaved("syntax"), "x ≔\n❧\n");
        assert!(!out.is_empty());
    }

    #[test]
    fn the_internal_error_diagnostic_asks_for_a_report() {
        let d = internal_error();
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert!(d.message.contains("please report"));
    }
}
