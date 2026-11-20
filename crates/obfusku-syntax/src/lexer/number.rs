//! Numeric literals (`CONCRETE_SYMBOLIC_GRAMMAR.md` §5).

use super::Lexer;
use super::token::{Token, TokenKind};
use obfusku_diagnostics::Diagnostic;

impl Lexer<'_> {
    /// Digits of one radix with single `_` separators between digits.
    fn scan_digits(&mut self, radix: u32, end: &mut usize) {
        while let Some(&(i, c)) = self.chars.peek() {
            if c.is_digit(radix) {
                self.chars.next();
                *end = i + c.len_utf8();
            } else if c == '_' {
                let mut ahead = self.chars.clone();
                ahead.next();
                if matches!(ahead.peek(), Some((_, d)) if d.is_digit(radix)) {
                    self.chars.next();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    pub(super) fn scan_number(&mut self, start: usize) -> Result<Token, Diagnostic> {
        let mut end = start;
        // `0x`/`0b`/`0o` integers (`CONCRETE_SYMBOLIC_GRAMMAR.md` §5).
        if self.source[start..].starts_with('0') {
            let radix = match self.source[start + 1..].chars().next() {
                Some('x' | 'X') => 16,
                Some('b' | 'B') => 2,
                Some('o' | 'O') => 8,
                _ => 0,
            };
            if radix != 0 {
                let mut ahead = self.chars.clone();
                ahead.next();
                ahead.next();
                if matches!(ahead.peek(), Some((_, d)) if d.is_digit(radix)) {
                    self.chars.next();
                    self.chars.next();
                    let digits_start = start + 2;
                    end = digits_start;
                    self.scan_digits(radix, &mut end);
                    let digits: String = self.source[digits_start..end]
                        .chars()
                        .filter(|c| *c != '_')
                        .collect();
                    let value = i64::from_str_radix(&digits, radix)
                        .map_err(|_| self.error(start, "integer literal out of range"))?;
                    return Ok(Token {
                        kind: TokenKind::Int(value),
                        span: self.span(start, end),
                    });
                }
            }
        }

        self.scan_digits(10, &mut end);
        let mut is_real = false;

        if let Some(&(_, '.')) = self.chars.peek() {
            let mut lookahead = self.chars.clone();
            lookahead.next();
            if matches!(lookahead.peek(), Some((_, d)) if d.is_ascii_digit()) {
                is_real = true;
                self.chars.next(); // consume '.'
                self.scan_digits(10, &mut end);
            }
        }

        // An exponent makes a `Real` with or without a fractional part;
        // `e` not followed by digits stays the start of an identifier.
        if matches!(self.chars.peek(), Some((_, 'e' | 'E'))) {
            let mut ahead = self.chars.clone();
            ahead.next();
            if matches!(ahead.peek(), Some((_, '+' | '-'))) {
                ahead.next();
            }
            if matches!(ahead.peek(), Some((_, d)) if d.is_ascii_digit()) {
                is_real = true;
                self.chars.next();
                if matches!(self.chars.peek(), Some((_, '+' | '-'))) {
                    self.chars.next();
                }
                self.scan_digits(10, &mut end);
            }
        }

        let text: String = self.source[start..end]
            .chars()
            .filter(|c| *c != '_')
            .collect();
        if is_real {
            let value: f64 = text
                .parse()
                .map_err(|_| self.error(start, "invalid real literal"))?;
            Ok(Token {
                kind: TokenKind::Real(value),
                span: self.span(start, end),
            })
        } else {
            // 2^63 is only meaningful as the magnitude of `−2^63`; it
            // lexes as `i64::MIN` and the parser accepts it solely
            // behind a unary minus.
            let value: i64 = if text == "9223372036854775808" {
                i64::MIN
            } else {
                text.parse()
                    .map_err(|_| self.error(start, "integer literal out of range"))?
            };
            Ok(Token {
                kind: TokenKind::Int(value),
                span: self.span(start, end),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::TokenKind::{Int, Real};
    use crate::lexer::testing::{error, ident, kinds};

    #[test]
    fn decimal_integers_with_digit_separators() {
        assert_eq!(kinds("42"), [Int(42)]);
        assert_eq!(kinds("1_000_000"), [Int(1_000_000)]);
    }

    #[test]
    fn radix_prefixes_hex_binary_octal() {
        assert_eq!(kinds("0xFF"), [Int(255)]);
        assert_eq!(kinds("0b101"), [Int(5)]);
        assert_eq!(kinds("0o17"), [Int(15)]);
        assert_eq!(kinds("0xff_ff"), [Int(65_535)]);
    }

    #[test]
    fn a_radix_prefix_without_digits_is_a_zero_then_a_name() {
        assert_eq!(kinds("0x"), [Int(0), ident("x")]);
    }

    #[test]
    fn reals_need_digits_on_both_sides_of_the_point() {
        assert_eq!(kinds("1.5"), [Real(1.5)]);
        assert_eq!(
            kinds("1.x"),
            [Int(1), crate::lexer::TokenKind::Dot, ident("x")]
        );
    }

    #[test]
    fn an_exponent_makes_a_real_with_or_without_a_fraction() {
        assert_eq!(kinds("2e3"), [Real(2000.0)]);
        assert_eq!(kinds("1.5e-2"), [Real(0.015)]);
        assert_eq!(kinds("1E+2"), [Real(100.0)]);
    }

    #[test]
    fn an_e_without_digits_starts_an_identifier() {
        assert_eq!(kinds("2e"), [Int(2), ident("e")]);
    }

    #[test]
    fn two_to_the_sixty_third_lexes_as_the_minimum_for_the_parser_to_police() {
        assert_eq!(kinds("9223372036854775808"), [Int(i64::MIN)]);
    }

    #[test]
    fn integers_beyond_range_are_errors() {
        assert!(error("9223372036854775809").contains("out of range"));
        assert!(error("0xFFFFFFFFFFFFFFFF").contains("out of range"));
    }
}
