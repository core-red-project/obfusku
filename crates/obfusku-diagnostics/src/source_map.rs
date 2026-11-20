//! Turning byte offsets back into something a person can find:
//! [`SourceMap`] and [`LineCol`].

use crate::diagnostic::Diagnostic;
use crate::span::{SourceId, Span};
use std::fmt;

/// A 1-indexed line and column.
///
/// Columns count Unicode scalar values (`char`s), not bytes. [`Span`] is
/// byte-based because that is what the lexer produces, but a byte column
/// would misreport the position on any line holding one of Obfusku's
/// multi-byte glyphs, which is nearly every line. (A `char` is still not a
/// grapheme cluster; combining marks count separately, as in most editors'
/// "column" for plain text.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column, in `char`s.
    pub column: u32,
}

impl fmt::Display for LineCol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Owns every source text of a run and resolves [`Span`]s against them.
///
/// **Why a registry and not a path.** A [`Span`] carries only a [`SourceId`],
/// which keeps AST nodes small and lets text come from anywhere (a file, a
/// REPL line, the bundled stdlib). The text is looked up here, once, when a
/// diagnostic is finally rendered.
///
/// **Why precomputed line starts.** Rendering is cheap but the language
/// server resolves positions on every keystroke; indexing the line breaks
/// once in [`add_file`](SourceMap::add_file) turns each lookup into a binary
/// search instead of a rescan from the top of the file.
///
/// Ids are issued sequentially and never invalidated: files are only added.
/// Passing a [`SourceId`] from a *different* map is a caller bug and panics.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<String>,
    /// Per file, the byte offset of each line's first byte; `line_starts[i][0]`
    /// is always `0`, and a trailing newline yields a final empty line.
    line_starts: Vec<Vec<u32>>,
}

impl SourceMap {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a source text and returns the [`SourceId`] to tokenize and
    /// parse it under.
    pub fn add_file(&mut self, text: impl Into<String>) -> SourceId {
        let text = text.into();
        let line_starts = std::iter::once(0)
            .chain(
                text.bytes()
                    .enumerate()
                    .filter(|&(_, byte)| byte == b'\n')
                    .map(|(i, _)| (i + 1) as u32),
            )
            .collect();
        let id = SourceId(self.files.len() as u32);
        self.files.push(text);
        self.line_starts.push(line_starts);
        id
    }

    /// How many files have been registered.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The full text registered under `id`.
    #[must_use]
    pub fn text(&self, id: SourceId) -> &str {
        &self.files[id.0 as usize]
    }

    /// The text a [`Span`] refers to.
    ///
    /// Panics if the span is not on `char` boundaries of its file, which the
    /// lexer guarantees for every span it produces.
    #[must_use]
    pub fn slice(&self, span: Span) -> &str {
        &self.text(span.source)[span.start as usize..span.end as usize]
    }

    /// The text of 1-indexed `line` of `id`, without its line terminator
    /// (`\n` or `\r\n`), or `None` when the file has no such line.
    #[must_use]
    pub fn line_text(&self, id: SourceId, line: u32) -> Option<&str> {
        let starts = &self.line_starts[id.0 as usize];
        let index = (line as usize).checked_sub(1)?;
        let start = *starts.get(index)? as usize;
        let end = starts
            .get(index + 1)
            .map_or_else(|| self.text(id).len(), |&next| next as usize);
        let raw = &self.text(id)[start..end];
        Some(
            raw.strip_suffix('\n')
                .map_or(raw, |l| l.strip_suffix('\r').unwrap_or(l)),
        )
    }

    /// Resolves a byte offset to its line and column.
    ///
    /// Total on purpose: a diagnostic anchored at end of file (the natural
    /// place for "unexpected end of input") has an offset equal to the length,
    /// and a stale offset from a caller bug must not turn a compile error
    /// into a panic. An offset past the end clamps to the end, resolving to
    /// the position right after the last character; one inside a multi-byte
    /// glyph rounds down to that glyph's start.
    #[must_use]
    pub fn line_col(&self, source: SourceId, byte_offset: u32) -> LineCol {
        let text = self.text(source);
        let offset = text.floor_char_boundary(byte_offset as usize);
        let starts = &self.line_starts[source.0 as usize];
        let line_index = match starts.binary_search(&(offset as u32)) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let column = text[starts[line_index] as usize..offset].chars().count() as u32 + 1;
        LineCol {
            line: line_index as u32 + 1,
            column,
        }
    }

    /// The start and end positions of a [`Span`]; they may be on different
    /// lines. The end is exclusive, like the span.
    #[must_use]
    pub fn span_location(&self, span: Span) -> (LineCol, LineCol) {
        (
            self.line_col(span.source, span.start),
            self.line_col(span.source, span.end),
        )
    }

    /// Renders a diagnostic as `"{severity}: {message} ({line}:{column})"`,
    /// anchored at the start of its primary span.
    ///
    /// The single-line, source-free form is deliberate: it is what test
    /// assertions and the REPL print. Richer presentation (JSON, LSP ranges)
    /// is built from [`span_location`](SourceMap::span_location) by the
    /// consumer that needs it.
    #[must_use]
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let at = self.line_col(diagnostic.primary.source, diagnostic.primary.start);
        format!("{}: {} ({at})", diagnostic.severity, diagnostic.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    fn map_with(text: &str) -> (SourceMap, SourceId) {
        let mut map = SourceMap::new();
        let id = map.add_file(text);
        (map, id)
    }

    fn at(line: u32, column: u32) -> LineCol {
        LineCol { line, column }
    }

    #[test]
    fn new_map_has_no_files() {
        assert_eq!(SourceMap::new().file_count(), 0);
    }

    #[test]
    fn ids_are_sequential_and_texts_are_kept_apart() {
        let mut map = SourceMap::new();
        let a = map.add_file("alpha");
        let b = map.add_file(String::from("beta"));
        assert_eq!((a, b), (SourceId(0), SourceId(1)));
        assert_eq!((map.text(a), map.text(b)), ("alpha", "beta"));
        assert_eq!(map.file_count(), 2);
    }

    #[test]
    fn slice_returns_the_spanned_text_across_glyphs() {
        let (map, id) = map_with("x ≔ 5\n");
        let start = "x ".len() as u32;
        assert_eq!(
            map.slice(Span::new(id, start, start + "≔".len() as u32)),
            "≔"
        );
    }

    #[test]
    fn offset_on_the_first_line_resolves_to_line_one() {
        let (map, id) = map_with("x ≔ 5\n❧\n");
        assert_eq!(map.line_col(id, 0), at(1, 1));
        assert_eq!(map.line_col(id, 2), at(1, 3));
    }

    #[test]
    fn offset_on_a_later_line_resolves_correctly() {
        let text = "x ≔ 1\ny ≔ 2\n❧\n";
        let (map, id) = map_with(text);
        assert_eq!(map.line_col(id, text.find('y').unwrap() as u32), at(2, 1));
    }

    #[test]
    fn column_counts_chars_not_bytes_on_a_multi_byte_glyph_line() {
        // '≔' is 3 bytes: the space after it sits at byte 5 but char column 4.
        let (map, id) = map_with("x ≔ 5\n❧\n");
        assert_eq!(map.line_col(id, "x ≔".len() as u32), at(1, 4));
    }

    #[test]
    fn offset_inside_a_glyph_rounds_down_to_its_start() {
        let (map, id) = map_with("x ≔ 5");
        let inside = "x ".len() as u32 + 1;
        assert_eq!(map.line_col(id, inside), at(1, 3));
    }

    #[test]
    fn offset_at_a_line_break_belongs_to_the_line_it_ends() {
        let (map, id) = map_with("ab\ncd");
        assert_eq!(map.line_col(id, 2), at(1, 3));
        assert_eq!(map.line_col(id, 3), at(2, 1));
    }

    #[test]
    fn offset_at_eof_resolves_after_the_last_character() {
        let text = "x ≔ 5\n❧\n";
        let (map, id) = map_with(text);
        let eof = text.len() as u32;
        assert_eq!(map.line_col(id, eof), at(3, 1));
        assert_eq!(map.line_col(id, eof + 100), at(3, 1));
    }

    #[test]
    fn empty_file_resolves_to_line_one_column_one() {
        let (map, id) = map_with("");
        assert_eq!(map.line_col(id, 0), at(1, 1));
        assert_eq!(map.line_col(id, 9), at(1, 1));
    }

    #[test]
    fn span_crossing_lines_resolves_each_end_independently() {
        let text = "x ≔ 1\n  ✚ 2\n❧\n";
        let (map, id) = map_with(text);
        let span = Span::new(
            id,
            text.find('x').unwrap() as u32,
            text.find('❧').unwrap() as u32,
        );
        assert_eq!(map.span_location(span), (at(1, 1), at(3, 1)));
    }

    #[test]
    fn line_text_strips_the_terminator() {
        let (map, id) = map_with("one\r\ntwo\nthree");
        assert_eq!(map.line_text(id, 1), Some("one"));
        assert_eq!(map.line_text(id, 2), Some("two"));
        assert_eq!(map.line_text(id, 3), Some("three"));
    }

    #[test]
    fn line_text_out_of_range_is_none() {
        let (map, id) = map_with("one\n");
        assert_eq!(map.line_text(id, 0), None);
        assert_eq!(map.line_text(id, 2), Some(""));
        assert_eq!(map.line_text(id, 3), None);
    }

    #[test]
    fn line_col_display_is_line_colon_column() {
        assert_eq!(at(12, 4).to_string(), "12:4");
    }

    #[test]
    fn render_has_severity_message_and_location() {
        let text = "x ≔ 1\ny ≔ nope\n❧\n";
        let (map, id) = map_with(text);
        let offset = text.find("nope").unwrap() as u32;
        let d = Diagnostic::error("unknown variable 'nope'", Span::new(id, offset, offset + 4));
        assert_eq!(map.render(&d), "error: unknown variable 'nope' (2:5)");
    }

    #[test]
    fn render_uses_each_severitys_word() {
        let (map, id) = map_with("x");
        for (severity, word) in [
            (Severity::Error, "error"),
            (Severity::Warning, "warning"),
            (Severity::Note, "note"),
        ] {
            let d = Diagnostic::new(severity, "m", Span::point(id, 0));
            assert_eq!(map.render(&d), format!("{word}: m (1:1)"));
        }
    }
}
