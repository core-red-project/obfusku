//! Obfusku diagnostics as LSP diagnostics.

use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, Position, Range};
use obfusku_diagnostics::{Diagnostic, Severity, SourceId, SourceMap};

/// The source every analysis registers first: the editor buffer itself.
/// Any other id belongs to an imported module.
const ENTRY: SourceId = SourceId(0);

/// The `source` field clients show next to each diagnostic.
pub(crate) const SOURCE_NAME: &str = "obfusku";

/// Converts diagnostics (byte-offset spans into `source_map`) to LSP ones.
///
/// **Imported files.** A diagnostic can originate in a module the buffer
/// imports, whose offsets mean nothing in this document. Rather than
/// underline unrelated code, it is pinned to the first line and its message
/// says where it really came from.
pub(crate) fn to_lsp_diagnostics(
    source_map: &SourceMap,
    diagnostics: &[Diagnostic],
) -> Vec<LspDiagnostic> {
    diagnostics
        .iter()
        .map(|d| {
            let own = d.primary.source == ENTRY;
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
                severity: Some(severity(d.severity)),
                source: Some(SOURCE_NAME.to_string()),
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

/// The LSP severity for an Obfusku one.
///
/// Kept here, not in `obfusku-diagnostics`: it is the LSP's vocabulary, and
/// the diagnostics crate must not know about any client.
const fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Note => DiagnosticSeverity::INFORMATION,
    }
}

/// The 0-indexed LSP position of a byte offset.
///
/// LSP columns count UTF-16 code units (the default position encoding),
/// whereas `SourceMap` columns count `char`s: they differ after any
/// character outside the Basic Multilingual Plane, which a string literal in
/// an Obfusku program may well hold. An offset past the end clamps to it;
/// one inside a glyph rounds down to its start.
fn utf16_position(map: &SourceMap, source: SourceId, byte_offset: u32) -> Position {
    let text = map.text(source);
    let end = text.floor_char_boundary(byte_offset as usize);
    let line_start = text[..end].rfind('\n').map_or(0, |i| i + 1);
    let line = text[..line_start].matches('\n').count();
    let column = text[line_start..end].encode_utf16().count();
    Position::new(line as u32, column as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::Span;

    fn convert_one(map: &SourceMap, d: Diagnostic) -> LspDiagnostic {
        to_lsp_diagnostics(map, &[d]).remove(0)
    }

    #[test]
    fn an_imported_files_diagnostic_is_pinned_to_line_one_with_a_prefix() {
        let mut map = SourceMap::new();
        map.add_file("main\n");
        let lib = map.add_file("a\nb c d\n");
        let out = convert_one(&map, Diagnostic::error("boom", Span::new(lib, 4, 5)));
        assert_eq!(out.range.start, Position::new(0, 0));
        assert_eq!(out.range.end, Position::new(0, 0));
        assert_eq!(out.message, "in an imported module: boom");
    }

    #[test]
    fn the_entry_files_diagnostic_keeps_its_own_position() {
        let mut map = SourceMap::new();
        let id = map.add_file("ab\ncd\n");
        let out = convert_one(&map, Diagnostic::error("boom", Span::new(id, 4, 5)));
        assert_eq!(out.range.start, Position::new(1, 1));
        assert_eq!(out.range.end, Position::new(1, 2));
        assert_eq!(out.message, "boom");
    }

    #[test]
    fn columns_count_utf16_units_after_astral_characters() {
        let mut map = SourceMap::new();
        // "😀" is 4 bytes in UTF-8 and 2 code units in UTF-16.
        let id = map.add_file("\"😀\" x\n");
        let out = convert_one(&map, Diagnostic::error("boom", Span::new(id, 7, 8)));
        assert_eq!(out.range.start, Position::new(0, 5));
    }

    #[test]
    fn bmp_glyphs_count_one_unit_though_they_take_three_bytes() {
        let mut map = SourceMap::new();
        let id = map.add_file("x ≔ 5\n");
        let offset = "x ≔ ".len() as u32;
        let out = convert_one(&map, Diagnostic::error("m", Span::point(id, offset)));
        assert_eq!(out.range.start, Position::new(0, 4));
    }

    #[test]
    fn a_multi_line_span_has_distinct_start_and_end_lines() {
        let mut map = SourceMap::new();
        let id = map.add_file("ab\ncd\nef");
        let out = convert_one(&map, Diagnostic::error("m", Span::new(id, 1, 7)));
        assert_eq!(out.range.start, Position::new(0, 1));
        assert_eq!(out.range.end, Position::new(2, 1));
    }

    #[test]
    fn an_offset_past_the_end_clamps_to_the_end() {
        let mut map = SourceMap::new();
        let id = map.add_file("ab\n");
        let out = convert_one(&map, Diagnostic::error("m", Span::point(id, 99)));
        assert_eq!(out.range.start, Position::new(1, 0));
    }

    #[test]
    fn an_offset_inside_a_glyph_rounds_down() {
        let mut map = SourceMap::new();
        let id = map.add_file("≔");
        let out = convert_one(&map, Diagnostic::error("m", Span::point(id, 1)));
        assert_eq!(out.range.start, Position::new(0, 0));
    }

    #[test]
    fn severities_map_to_the_lsp_ones() {
        let mut map = SourceMap::new();
        let id = map.add_file("x");
        let at = Span::point(id, 0);
        let got = to_lsp_diagnostics(
            &map,
            &[
                Diagnostic::error("e", at),
                Diagnostic::warning("w", at),
                Diagnostic::note("n", at),
            ],
        )
        .into_iter()
        .map(|d| d.severity)
        .collect::<Vec<_>>();
        assert_eq!(
            got,
            [
                Some(DiagnosticSeverity::ERROR),
                Some(DiagnosticSeverity::WARNING),
                Some(DiagnosticSeverity::INFORMATION)
            ]
        );
    }

    #[test]
    fn every_diagnostic_names_its_source() {
        let mut map = SourceMap::new();
        let id = map.add_file("x");
        let out = convert_one(&map, Diagnostic::error("e", Span::point(id, 0)));
        assert_eq!(out.source.as_deref(), Some("obfusku"));
    }
}
