//! Document analysis: the same pipeline as `obfusku_cli::check_file`
//! (`spec/IMPLEMENTATION_ARCHITECTURE.md` §12), except the entry file's
//! text comes from the editor buffer rather than disk, so diagnostics
//! reflect unsaved edits.

use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range};
use obfusku_diagnostics::{Diagnostic, Severity, SourceId, SourceMap};
use std::path::Path;

/// Runs the full check pipeline for `source`, resolving imports and the
/// stdlib prelude against `path`'s project root. Returns the `SourceMap`
/// every diagnostic's span is relative to, alongside all diagnostics
/// (fatal errors or, on success, non-fatal warnings).
pub fn analyze(path: &Path, source: &str) -> (SourceMap, Vec<Diagnostic>) {
    obfusku_cli::analyze_text(path, source)
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
            // A span in an imported file has that file's positions: pin it to
            // the first line of this one rather than underline unrelated code.
            let own = d.primary.source.0 == 0;
            let at = |offset: u32| {
                if own {
                    utf16_position(source_map, d.primary.source, offset)
                } else {
                    Position::new(0, 0)
                }
            };
            LspDiagnostic {
                range: Range {
                    start: at(d.primary.start),
                    end: at(d.primary.end),
                },
                severity: Some(match d.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Note => DiagnosticSeverity::INFORMATION,
                }),
                source: Some("obfusku".to_string()),
                message: if own {
                    d.message.clone()
                } else {
                    format!("in an imported module: {}", d.message)
                },
                ..Default::default()
            }
        })
        .collect()
}

/// LSP columns count UTF-16 code units; `SourceMap` columns count `char`s.
fn utf16_position(map: &SourceMap, source: SourceId, byte_offset: u32) -> Position {
    let text = map.text(source);
    let mut end = (byte_offset as usize).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let line_start = text[..end].rfind('\n').map_or(0, |i| i + 1);
    let line = text[..line_start].matches('\n').count();
    let column = text[line_start..end].encode_utf16().count();
    Position::new(line as u32, column as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::{Diagnostic, Severity, Span};

    #[test]
    fn an_imported_files_diagnostic_is_pinned_to_line_one_with_a_prefix() {
        let mut map = SourceMap::new();
        map.add_file("main\n");
        let lib = map.add_file("a\nb c d\n");
        let d = Diagnostic {
            severity: Severity::Error,
            message: "boom".to_string(),
            primary: Span {
                source: lib,
                start: 4,
                end: 5,
            },
        };
        let out = to_lsp_diagnostics(&map, &[d]);
        assert_eq!(out[0].range.start, Position::new(0, 0));
        assert_eq!(out[0].message, "in an imported module: boom");
    }

    #[test]
    fn a_buffer_not_yet_on_disk_is_checked_not_reported_unreadable() {
        let path = std::env::temp_dir().join("obk-unsaved-buffer-does-not-exist.obk");
        let (_, diagnostics) = analyze(&path, "x ≔ 1\n❧\n");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn columns_count_utf16_units_after_astral_characters() {
        let mut map = SourceMap::new();
        // "😀" is 4 bytes in UTF-8 and 2 code units in UTF-16.
        let id = map.add_file("\"😀\" x\n");
        let d = Diagnostic {
            severity: Severity::Error,
            message: "boom".to_string(),
            primary: Span {
                source: id,
                start: 7,
                end: 8,
            },
        };
        let out = to_lsp_diagnostics(&map, &[d]);
        assert_eq!(out[0].range.start, Position::new(0, 5));
    }

    #[test]
    fn the_entry_files_diagnostic_keeps_its_own_position() {
        let mut map = SourceMap::new();
        let id = map.add_file("ab\ncd\n");
        let d = Diagnostic {
            severity: Severity::Error,
            message: "boom".to_string(),
            primary: Span {
                source: id,
                start: 4,
                end: 5,
            },
        };
        let out = to_lsp_diagnostics(&map, &[d]);
        assert_eq!(out[0].range.start, Position::new(1, 1));
        assert_eq!(out[0].message, "boom");
    }
}
