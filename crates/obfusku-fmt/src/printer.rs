//! The output buffer and the one piece of state printing needs.

use crate::glyph::FIELD_SEPARATOR;

/// Two spaces per level: the canonical indentation.
const INDENT: &str = "  ";

/// Accumulates canonical text and tracks how deeply nested the expression
/// being printed is.
///
/// **Why an explicit value.** Nesting depth decides the indentation of match
/// arms and local-binding chains, and it must follow the call stack exactly.
/// Carrying it next to the buffer makes that a property of the type (a
/// [`nested`](Printer::nested) call is the only way to change it, and it is
/// always restored) instead of hidden global state that tests running on
/// parallel threads could not share safely.
///
/// Printing is infallible: every method appends, nothing reads back.
#[derive(Debug, Default)]
pub(crate) struct Printer {
    out: String,
    depth: usize,
}

impl Printer {
    /// Whether nothing has been written yet.
    pub(crate) fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// The finished text.
    pub(crate) fn finish(self) -> String {
        self.out
    }

    /// Appends one character.
    pub(crate) fn push(&mut self, c: char) {
        self.out.push(c);
    }

    /// Appends text.
    pub(crate) fn push_str(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Appends `level` indentation steps.
    pub(crate) fn indent(&mut self, level: usize) {
        for _ in 0..level {
            self.out.push_str(INDENT);
        }
    }

    /// The current arm depth: how many enclosing `⟡` arms or local chains
    /// the expression being printed sits inside.
    pub(crate) fn depth(&self) -> usize {
        self.depth
    }

    /// Runs `f` one arm level deeper, restoring the depth afterwards.
    pub(crate) fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    /// Prints `items` with `separator` between consecutive ones.
    ///
    /// Every comma- or `⟢`-separated list in the grammar goes through here,
    /// so "no separator before the first item" is written once.
    pub(crate) fn separated<T>(
        &mut self,
        items: impl IntoIterator<Item = T>,
        separator: &str,
        mut print_item: impl FnMut(&mut Self, T),
    ) {
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                self.push_str(separator);
            }
            print_item(self, item);
        }
    }

    /// Prints `glyph` with a space on each side: the spacing of every infix
    /// glyph (`→`, `▷`, `⚙︎`).
    pub(crate) fn spaced(&mut self, glyph: &str) {
        self.push(' ');
        self.push_str(glyph);
        self.push(' ');
    }

    /// Prints `{ name: value ⟢ name: value }`, the shared shape of record
    /// types, named construction, record update and named patterns.
    pub(crate) fn braced_fields<N: AsRef<str>, V>(
        &mut self,
        fields: impl IntoIterator<Item = (N, V)>,
        mut print_value: impl FnMut(&mut Self, V),
    ) {
        self.push_str("{ ");
        self.separated(fields, FIELD_SEPARATOR, |p, (name, value)| {
            p.push_str(name.as_ref());
            p.push_str(": ");
            print_value(p, value);
        });
        self.push_str(" }");
    }

    /// Prints `inner` between `open` and `close`.
    pub(crate) fn enclosed(&mut self, open: char, close: char, inner: impl FnOnce(&mut Self)) {
        self.push(open);
        inner(self);
        self.push(close);
    }

    /// Prints `inner` in parentheses.
    pub(crate) fn parenthesized(&mut self, inner: impl FnOnce(&mut Self)) {
        self.enclosed('(', ')', inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indent_is_two_spaces_per_level() {
        let mut p = Printer::default();
        p.indent(3);
        assert_eq!(p.finish(), "      ");
    }

    #[test]
    fn nested_deepens_and_restores() {
        let mut p = Printer::default();
        assert_eq!(p.depth(), 0);
        p.nested(|p| {
            assert_eq!(p.depth(), 1);
            p.nested(|p| assert_eq!(p.depth(), 2));
            assert_eq!(p.depth(), 1);
        });
        assert_eq!(p.depth(), 0);
    }

    #[test]
    fn nested_returns_the_closures_value() {
        assert_eq!(Printer::default().nested(|p| p.depth() * 10), 10);
    }

    #[test]
    fn separated_puts_the_separator_between_items_only() {
        let mut p = Printer::default();
        p.separated(["a", "b", "c"], ", ", Printer::push_str);
        assert_eq!(p.finish(), "a, b, c");
    }

    #[test]
    fn separated_of_nothing_writes_nothing() {
        let mut p = Printer::default();
        p.separated(std::iter::empty::<&str>(), ", ", Printer::push_str);
        assert!(p.is_empty());
    }

    #[test]
    fn spaced_surrounds_a_glyph() {
        let mut p = Printer::default();
        p.spaced("→");
        assert_eq!(p.finish(), " → ");
    }

    #[test]
    fn braced_fields_prints_name_colon_value_with_separators() {
        let mut p = Printer::default();
        p.braced_fields([("a", 1), ("b", 2)], |p, v| p.push_str(&v.to_string()));
        assert_eq!(p.finish(), "{ a: 1 \u{27E2} b: 2 }");
    }

    #[test]
    fn parenthesized_wraps_its_content() {
        let mut p = Printer::default();
        p.parenthesized(|p| p.push_str("x"));
        assert_eq!(p.finish(), "(x)");
    }
}
