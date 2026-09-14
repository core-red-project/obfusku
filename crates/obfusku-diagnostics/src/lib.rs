//! Foundational source-location and diagnostic types.
//!
//! No dependency on any other Obfusku crate — see
//! `spec/IMPLEMENTATION_ARCHITECTURE.md` §14. Nearly every other crate
//! depends on this one; keeping it minimal and stable is the point.

/// Identifies a source file within a [`SourceMap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SourceId(pub u32);

/// A byte-range location within a single source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub source: SourceId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// The smallest span containing both `self` and `other`. Used by the
    /// parser to build a compound node's span from its parts' spans.
    pub fn merge(self, other: Span) -> Span {
        debug_assert_eq!(
            self.source, other.source,
            "merging spans from different source files"
        );
        Span {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Severity of a [`Diagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A single diagnostic message anchored to a primary span.
///
/// Every crate that reports errors converts its own strongly-typed error
/// enum into this shape for rendering — see
/// `spec/IMPLEMENTATION_ARCHITECTURE.md` §17.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub primary: Span,
}

/// A 1-indexed line/column position. Columns count Unicode scalar
/// values (`char`s), not bytes — `Span` is byte-based (as produced by
/// the lexer), but a byte column would misreport position on any line
/// containing one of Obfusku's many multi-byte glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: u32,
    pub column: u32,
}

impl std::fmt::Display for LineCol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Maps [`SourceId`]s to their text, for resolving spans to line/column
/// when rendering diagnostics.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<String>,
    /// Byte offset of each line's first byte, per file — `line_starts[i][0]`
    /// is always `0`. Built once in [`SourceMap::add_file`] so
    /// [`SourceMap::line_col`] can binary-search instead of rescanning.
    line_starts: Vec<Vec<u32>>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a source file's text and returns the [`SourceId`] to
    /// tokenize/parse it under.
    pub fn add_file(&mut self, text: impl Into<String>) -> SourceId {
        let text = text.into();
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        let id = SourceId(self.files.len() as u32);
        self.files.push(text);
        self.line_starts.push(line_starts);
        id
    }

    pub fn text(&self, id: SourceId) -> &str {
        &self.files[id.0 as usize]
    }

    /// The source text a [`Span`] refers to.
    pub fn slice(&self, span: Span) -> &str {
        &self.text(span.source)[span.start as usize..span.end as usize]
    }

    /// Resolves a byte offset to its 1-indexed line/column. An offset
    /// past the end of the file (e.g. an EOF-anchored diagnostic) is
    /// clamped to the end, resolving to the position right after the
    /// last character rather than panicking.
    pub fn line_col(&self, source: SourceId, byte_offset: u32) -> LineCol {
        let text = self.text(source);
        let byte_offset = byte_offset.min(text.len() as u32);
        let starts = &self.line_starts[source.0 as usize];
        let line_idx = match starts.binary_search(&byte_offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = starts[line_idx] as usize;
        let column = text[line_start..byte_offset as usize].chars().count() as u32 + 1;
        LineCol {
            line: (line_idx + 1) as u32,
            column,
        }
    }

    /// The `(start, end)` line/column pair for a [`Span`] — may span
    /// multiple lines.
    pub fn span_location(&self, span: Span) -> (LineCol, LineCol) {
        (
            self.line_col(span.source, span.start),
            self.line_col(span.source, span.end),
        )
    }

    /// Renders a diagnostic as `"{severity}: {message} (line:col)"`,
    /// anchored at the primary span's start.
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let loc = self.line_col(diagnostic.primary.source, diagnostic.primary.start);
        let severity = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        format!("{severity}: {} ({loc})", diagnostic.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_is_constructible() {
        let s = Span {
            source: SourceId(0),
            start: 0,
            end: 3,
        };
        assert_eq!(s.start, 0);
        assert_eq!(s.end, 3);
    }

    #[test]
    fn source_map_starts_empty() {
        let map = SourceMap::new();
        assert!(map.files.is_empty());
    }

    #[test]
    fn offset_on_the_first_line_resolves_to_line_one() {
        let mut map = SourceMap::new();
        let id = map.add_file("x ≔ 5\n❧\n");
        assert_eq!(map.line_col(id, 0), LineCol { line: 1, column: 1 });
        assert_eq!(map.line_col(id, 2), LineCol { line: 1, column: 3 });
    }

    #[test]
    fn offset_on_a_later_line_resolves_correctly() {
        let mut map = SourceMap::new();
        let text = "x ≔ 1\ny ≔ 2\n❧\n";
        let id = map.add_file(text);
        let y_offset = text.find('y').unwrap() as u32;
        assert_eq!(map.line_col(id, y_offset), LineCol { line: 2, column: 1 });
    }

    #[test]
    fn column_counts_chars_not_bytes_on_a_multi_byte_glyph_line() {
        // '≔' is a 3-byte UTF-8 sequence; the 'x' before it is 1 char/1
        // byte, so the space right after '≔' sits at byte offset 5 but
        // char-column 4 (x, space, ≔, space).
        let mut map = SourceMap::new();
        let id = map.add_file("x ≔ 5\n❧\n");
        let space_after_bind = "x ≔".len() as u32; // byte offset, spans 3 bytes for ≔
        assert_eq!(
            map.line_col(id, space_after_bind),
            LineCol { line: 1, column: 4 }
        );
    }

    #[test]
    fn offset_at_the_very_start_of_the_file_is_line_one_column_one() {
        let mut map = SourceMap::new();
        let id = map.add_file("❧\n");
        assert_eq!(map.line_col(id, 0), LineCol { line: 1, column: 1 });
    }

    #[test]
    fn offset_at_eof_resolves_past_the_last_character_without_panicking() {
        let mut map = SourceMap::new();
        let text = "x ≔ 5\n❧\n";
        let id = map.add_file(text);
        let eof = text.len() as u32;
        let loc = map.line_col(id, eof);
        assert_eq!(loc, LineCol { line: 3, column: 1 });
        // An offset genuinely past the end (a caller bug elsewhere)
        // still resolves instead of panicking, clamped to EOF.
        assert_eq!(map.line_col(id, eof + 100), loc);
    }

    #[test]
    fn span_crossing_multiple_lines_resolves_start_and_end_independently() {
        let mut map = SourceMap::new();
        let text = "x ≔ 1\n  ✚ 2\n❧\n";
        let id = map.add_file(text);
        let start = text.find('x').unwrap() as u32;
        let end = text.find('❧').unwrap() as u32;
        let span = Span {
            source: id,
            start,
            end,
        };
        let (start_loc, end_loc) = map.span_location(span);
        assert_eq!(start_loc, LineCol { line: 1, column: 1 });
        assert_eq!(end_loc.line, 3);
    }

    #[test]
    fn render_includes_severity_message_and_location() {
        let mut map = SourceMap::new();
        let text = "x ≔ 1\ny ≔ nope\n❧\n";
        let id = map.add_file(text);
        let offset = text.find("nope").unwrap() as u32;
        let d = Diagnostic {
            severity: Severity::Error,
            message: "unknown variable 'nope'".to_string(),
            primary: Span {
                source: id,
                start: offset,
                end: offset + 4,
            },
        };
        let rendered = map.render(&d);
        assert_eq!(rendered, "error: unknown variable 'nope' (2:5)");
    }
}
