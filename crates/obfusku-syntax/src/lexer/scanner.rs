//! The scanning loop: trivia, dispatch and identifiers.

use super::Lexer;
use super::token::{Token, TokenKind};
use obfusku_diagnostics::Diagnostic;

impl Lexer<'_> {
    /// Scans the whole source, ending with an `Eof` token. Stops at the first
    /// lexical error.
    pub(super) fn run(&mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let Some(&(start, c)) = self.chars.peek() else {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: self.span(self.len, self.len),
                });
                break;
            };

            // Try multi-codepoint/multi-character glyphs before
            // single-char ones — longest-match-first, per §2's stated
            // lexical priority.
            if let Some(tok) = self.scan_variation_selector_glyph(start, c) {
                tokens.push(tok);
                continue;
            }
            // Try fixed glyphs first — several (notably 'λ') are
            // Unicode-alphabetic and would otherwise be mis-scanned as
            // the start of an identifier if checked second.
            if let Some(tok) = self.scan_glyph(start, c) {
                tokens.push(tok);
                continue;
            }
            if let Some(tok) = self.scan_glyph_ident(start, c) {
                tokens.push(tok);
                continue;
            }
            if let Some(tok) = self.scan_host_effect(start, c) {
                tokens.push(tok);
                continue;
            }
            if c.is_ascii_digit() {
                tokens.push(self.scan_number(start)?);
                continue;
            }
            if c == '"' {
                tokens.push(self.scan_string(start)?);
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                tokens.push(self.scan_identifier(start));
                continue;
            }

            return Err(self.error(start, format!("unexpected character '{c}'")));
        }
        Ok(tokens)
    }

    /// Skips whitespace and comments, recording each comment's byte range.
    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            while let Some(&(_, c)) = self.chars.peek() {
                if c.is_whitespace() {
                    self.chars.next();
                } else {
                    break;
                }
            }

            if let Some(&(from, '/')) = self.chars.peek() {
                let mut lookahead = self.chars.clone();
                lookahead.next();
                if let Some(&(_, '/')) = lookahead.peek() {
                    let mut to = from;
                    while let Some(&(i, c)) = self.chars.peek() {
                        self.chars.next();
                        if c == '\n' {
                            break;
                        }
                        to = i + c.len_utf8();
                    }
                    self.comments.push((from, to));
                    continue;
                }
            }

            if let Some(&(start, '⌈')) = self.chars.peek() {
                self.chars.next();
                let mut depth = 1;
                let mut to = start;
                while depth > 0 {
                    match self.chars.next() {
                        Some((i, c @ '⌈')) => {
                            depth += 1;
                            to = i + c.len_utf8();
                        }
                        Some((i, c @ '⌉')) => {
                            depth -= 1;
                            to = i + c.len_utf8();
                        }
                        Some((i, c)) => to = i + c.len_utf8(),
                        None => return Err(self.error(start, "unterminated block comment")),
                    }
                }
                self.comments.push((start, to));
                continue;
            }

            break;
        }
        Ok(())
    }

    fn scan_identifier(&mut self, start: usize) -> Token {
        let mut end = start;
        while let Some(&(i, c)) = self.chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.chars.next();
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        let text = self.source[start..end].to_string();
        // §10: a bare "_" is the reserved Wildcard, never an ordinary
        // identifier — recognized here so the parser never needs to
        // special-case an Ident("_") itself.
        let kind = if text == "_" {
            TokenKind::Wildcard
        } else {
            TokenKind::Ident(text)
        };
        Token {
            kind,
            span: self.span(start, end),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::TokenKind::{Eof, Wildcard};
    use crate::lexer::testing::{ident, kinds};
    use crate::lexer::tokenize;
    use obfusku_diagnostics::SourceId;

    #[test]
    fn identifiers_may_hold_letters_digits_and_underscores() {
        assert_eq!(kinds("abc_9 Ünï"), [ident("abc_9"), ident("Ünï")]);
    }

    #[test]
    fn a_lone_underscore_is_the_wildcard_not_a_name() {
        assert_eq!(kinds("_"), [Wildcard]);
        assert_eq!(kinds("_x"), [ident("_x")]);
    }

    #[test]
    fn whitespace_of_any_kind_separates_tokens() {
        assert_eq!(kinds("a\t\n\r b"), [ident("a"), ident("b")]);
    }

    #[test]
    fn empty_and_blank_sources_are_just_eof() {
        for source in ["", "  \n\t"] {
            let tokens = tokenize(source, SourceId(0)).unwrap();
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, Eof);
        }
    }

    #[test]
    fn a_single_slash_is_not_a_comment() {
        assert!(tokenize("a / b", SourceId(0)).is_err());
    }
}
