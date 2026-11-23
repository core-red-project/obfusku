//! Printing literal values so they re-lex to the same value.

use crate::glyph::MIN_INT_LITERAL;
use crate::printer::Printer;
use std::fmt::Write as _;
use unicode_normalization::{char::canonical_combining_class, is_nfc};

impl Printer {
    /// An integer literal.
    ///
    /// `i64::MIN` is the one value with no positive counterpart, so it cannot
    /// be spelled as a negated literal; the lexer accepts its magnitude only
    /// directly after the unary minus, and that spelling is emitted whole.
    pub(crate) fn int(&mut self, value: i64) {
        if value == i64::MIN {
            self.push_str(MIN_INT_LITERAL);
        } else {
            self.push_str(&value.to_string());
        }
    }

    /// A real literal. Whole numbers keep a `.0` so they stay `Real` rather
    /// than re-lexing as `Int`.
    pub(crate) fn real(&mut self, value: f64) {
        let text = if value == value.trunc() && value.is_finite() {
            format!("{value:.1}")
        } else {
            format!("{value}")
        };
        self.push_str(&text);
    }

    /// A string literal.
    ///
    /// Source must be NFC (`CONCRETE_SYMBOLIC_GRAMMAR.md` §2), so a value
    /// holding combining marks is printed with `\u{…}` escapes instead of raw
    /// characters that would not re-lex. Escaping is tried minimally first
    /// (controls and combining marks only) and falls back to escaping *every*
    /// non-ASCII character when even that is not NFC, e.g. when a base
    /// character composes with its neighbour.
    pub(crate) fn string_literal(&mut self, value: &str) {
        let minimal = quote(value, false);
        let literal = if is_nfc(&minimal) {
            minimal
        } else {
            quote(value, true)
        };
        self.push_str(&literal);
    }
}

fn quote(value: &str, should_escape_all_non_ascii: bool) -> String {
    let mut literal = String::from('"');
    for c in value.chars() {
        match c {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            '\t' => literal.push_str("\\t"),
            '\r' => literal.push_str("\\r"),
            '\0' => literal.push_str("\\0"),
            c if c.is_control()
                || canonical_combining_class(c) != 0
                || (should_escape_all_non_ascii && !c.is_ascii()) =>
            {
                let _ = write!(literal, "\\u{{{:X}}}", u32::from(c));
            }
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(f: impl FnOnce(&mut Printer)) -> String {
        let mut p = Printer::default();
        f(&mut p);
        p.finish()
    }

    #[test]
    fn integers_print_in_decimal() {
        assert_eq!(printed(|p| p.int(0)), "0");
        assert_eq!(printed(|p| p.int(42)), "42");
    }

    #[test]
    fn the_minimum_integer_uses_the_unary_minus_spelling() {
        assert_eq!(printed(|p| p.int(i64::MIN)), "−9223372036854775808");
    }

    #[test]
    fn whole_reals_keep_a_decimal_point() {
        assert_eq!(printed(|p| p.real(3.0)), "3.0");
        assert_eq!(printed(|p| p.real(0.0)), "0.0");
    }

    #[test]
    fn fractional_reals_print_shortest_round_trip() {
        assert_eq!(printed(|p| p.real(2.5)), "2.5");
        assert_eq!(printed(|p| p.real(0.1)), "0.1");
    }

    #[test]
    fn plain_strings_are_quoted_verbatim() {
        assert_eq!(printed(|p| p.string_literal("hola ≔ ❧")), "\"hola ≔ ❧\"");
    }

    #[test]
    fn quotes_backslashes_and_whitespace_escapes_round_trip() {
        assert_eq!(
            printed(|p| p.string_literal("a\"b\\c\nd\te\rf\0")),
            r#""a\"b\\c\nd\te\rf\0""#
        );
    }

    #[test]
    fn control_characters_are_escaped_as_code_points() {
        assert_eq!(printed(|p| p.string_literal("\u{7}")), "\"\\u{7}\"");
    }

    #[test]
    fn combining_marks_are_escaped_not_printed_raw() {
        assert_eq!(printed(|p| p.string_literal("e\u{301}")), "\"e\\u{301}\"");
    }

    #[test]
    fn precomposed_non_ascii_stays_raw() {
        assert_eq!(printed(|p| p.string_literal("ñ")), "\"ñ\"");
    }
}
