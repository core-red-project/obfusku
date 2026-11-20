//! String literals and their escapes (`CONCRETE_SYMBOLIC_GRAMMAR.md` §5).

use super::Lexer;
use super::token::{Token, TokenKind};
use obfusku_diagnostics::Diagnostic;

impl Lexer<'_> {
    pub(super) fn scan_string(&mut self, start: usize) -> Result<Token, Diagnostic> {
        self.chars.next(); // consume opening quote
        let mut value = String::new();
        loop {
            match self.chars.next() {
                None => return Err(self.error(start, "unterminated string literal")),
                Some((i, '"')) => {
                    return Ok(Token {
                        kind: TokenKind::Str(value),
                        span: self.span(start, i + 1),
                    });
                }
                // CONCRETE_SYMBOLIC_GRAMMAR.md §5's EscapeSequence
                // production.
                Some((i, '\\')) => match self.chars.next() {
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, 'r')) => value.push('\r'),
                    Some((_, '0')) => value.push('\0'),
                    Some((_, 'u')) => value.push(self.scan_unicode_escape(i)?),
                    Some((_, other)) => {
                        return Err(
                            self.error(i, format!("unsupported escape sequence '\\{other}'"))
                        );
                    }
                    None => return Err(self.error(start, "unterminated string literal")),
                },
                Some((_, c)) => value.push(c),
            }
        }
    }

    /// `\u{h…}` — one to six hex digits naming a Unicode scalar value.
    fn scan_unicode_escape(&mut self, at: usize) -> Result<char, Diagnostic> {
        if !matches!(self.chars.next(), Some((_, '{'))) {
            return Err(self.error(at, "expected '{' after '\\u'"));
        }
        let mut digits = String::new();
        loop {
            match self.chars.next() {
                Some((_, '}')) => break,
                Some((_, c)) if c.is_ascii_hexdigit() && digits.len() < 6 => digits.push(c),
                _ => return Err(self.error(at, "malformed '\\u{…}' escape")),
            }
        }
        u32::from_str_radix(&digits, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| self.error(at, "'\\u{…}' is not a Unicode scalar value"))
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::TokenKind::Str;
    use crate::lexer::testing::{error, kinds};

    fn text(s: &str) -> crate::lexer::TokenKind {
        Str(s.to_string())
    }

    #[test]
    fn plain_and_empty_strings() {
        assert_eq!(kinds("\"abc\""), [text("abc")]);
        assert_eq!(kinds("\"\""), [text("")]);
    }

    #[test]
    fn simple_escapes() {
        assert_eq!(
            kinds(r#""a\"b\\c\nd\te\rf\0""#),
            [text("a\"b\\c\nd\te\rf\0")]
        );
    }

    #[test]
    fn unicode_escapes_name_a_scalar_value() {
        assert_eq!(kinds(r#""\u{41}\u{1F600}""#), [text("A😀")]);
    }

    #[test]
    fn malformed_unicode_escapes_are_errors() {
        assert!(error(r#""\u41""#).contains("expected '{'"));
        assert!(error(r#""\u{}""#).contains("not a Unicode scalar value"));
        assert!(error(r#""\u{D800}""#).contains("not a Unicode scalar value"));
        assert!(error(r#""\u{1234567}""#).contains("malformed"));
        assert!(error(r#""\u{zz}""#).contains("malformed"));
    }

    #[test]
    fn an_unknown_escape_is_an_error() {
        assert!(error(r#""\q""#).contains("unsupported escape sequence '\\q'"));
    }

    #[test]
    fn an_unterminated_string_is_an_error_at_its_start() {
        assert!(error("\"abc").contains("unterminated string"));
        assert!(error("\"abc\\").contains("unterminated string"));
    }

    #[test]
    fn a_string_may_span_lines() {
        assert_eq!(kinds("\"a\nb\""), [text("a\nb")]);
    }
}
