//! Where in the source a thing came from: [`SourceId`] and [`Span`].

/// Identifies one source file registered in a [`SourceMap`](crate::SourceMap).
///
/// An index, not a path: a program is checked from text that may never have
/// touched disk (the REPL, the language server's unsaved buffers, the bundled
/// stdlib), so identity cannot be a filename. The id is only meaningful for
/// the map that issued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SourceId(pub u32);

/// A half-open byte range `start..end` within one source file.
///
/// **Why bytes.** The lexer walks UTF-8 and slices by byte offset; storing
/// bytes keeps the hot path allocation-free and makes [`Span::merge`] plain
/// arithmetic. Obfusku's glyphs are multi-byte, so a byte offset is *not* a
/// column: [`SourceMap::line_col`](crate::SourceMap::line_col) converts at
/// the edge, where a message is rendered, and nowhere else.
///
/// **Why `u32`.** It halves every AST node's footprint against `usize`. The
/// limit is a 4 GiB source file, far beyond any program.
///
/// [`Default`] is the empty span at offset 0 of source 0, the placeholder for
/// synthesized nodes that have no position of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// The file this range belongs to.
    pub source: SourceId,
    /// Offset of the first byte.
    pub start: u32,
    /// Offset one past the last byte; `end >= start`.
    pub end: u32,
}

impl Span {
    /// A span over `start..end` of `source`.
    #[must_use]
    pub const fn new(source: SourceId, start: u32, end: u32) -> Self {
        Self { source, start, end }
    }

    /// The empty span at `offset`: a position rather than a range, for
    /// diagnostics about something that is *missing* (an unexpected end of
    /// file, a token that should have been here).
    #[must_use]
    pub const fn point(source: SourceId, offset: u32) -> Self {
        Self::new(source, offset, offset)
    }

    /// Number of bytes covered.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end - self.start
    }

    /// Whether the span covers no bytes (a [`point`](Span::point)).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` falls inside `start..end`. The end is exclusive, so
    /// an empty span contains nothing.
    #[must_use]
    pub const fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// The smallest span containing both `self` and `other`.
    ///
    /// The parser builds a compound node's span from its parts' spans, which
    /// is why the result may also cover any gap between them (whitespace,
    /// comments, a closing delimiter).
    ///
    /// Spans from different files have no common cover; that is a caller bug,
    /// so it is asserted in debug builds and resolved to `self`'s file in
    /// release rather than aborting a user's compile.
    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: SourceId = SourceId(0);

    #[test]
    fn new_stores_its_parts() {
        let span = Span::new(SourceId(3), 4, 9);
        assert_eq!((span.source, span.start, span.end), (SourceId(3), 4, 9));
    }

    #[test]
    fn point_is_empty_and_at_the_offset() {
        let point = Span::point(FILE, 7);
        assert!(point.is_empty());
        assert_eq!((point.start, point.end, point.len()), (7, 7, 0));
    }

    #[test]
    fn len_counts_bytes() {
        assert_eq!(Span::new(FILE, 2, 8).len(), 6);
        assert!(!Span::new(FILE, 2, 8).is_empty());
    }

    #[test]
    fn contains_is_half_open() {
        let span = Span::new(FILE, 2, 5);
        assert!(!span.contains(1));
        assert!(span.contains(2));
        assert!(span.contains(4));
        assert!(!span.contains(5));
    }

    #[test]
    fn empty_span_contains_nothing() {
        assert!(!Span::point(FILE, 3).contains(3));
    }

    #[test]
    fn merge_covers_both_and_the_gap_between() {
        let merged = Span::new(FILE, 10, 12).merge(Span::new(FILE, 2, 4));
        assert_eq!((merged.start, merged.end), (2, 12));
    }

    #[test]
    fn merge_is_commutative_and_idempotent() {
        let a = Span::new(FILE, 1, 3);
        let b = Span::new(FILE, 5, 9);
        assert_eq!(a.merge(b), b.merge(a));
        assert_eq!(a.merge(a), a);
    }

    #[test]
    fn merge_with_a_nested_span_keeps_the_outer() {
        let outer = Span::new(FILE, 0, 10);
        assert_eq!(outer.merge(Span::new(FILE, 3, 4)), outer);
    }

    #[test]
    fn default_is_the_empty_span_of_source_zero() {
        assert_eq!(Span::default(), Span::point(SourceId(0), 0));
    }
}
