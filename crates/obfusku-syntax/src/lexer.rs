//! Source text to tokens (`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §2–§5).
//!
//! **Longest match first.** At each position the scanner tries, in order:
//! two-codepoint glyphs, fixed glyphs, glyph-spelled names, host effects,
//! numbers, strings, then identifiers. That order is the grammar's lexical
//! priority (§2). Fixed glyphs precede identifiers because several (`λ`) are
//! Unicode-alphabetic and would otherwise be eaten as a name.
//!
//! `⚙︎` (rebind) and `☠︎` (binary subtract) are really two codepoints, a base
//! character plus U+FE0E (the text-presentation selector), so they are found
//! with explicit two-codepoint lookahead instead of single-`char` dispatch.
//!
//! **NFC is checked, not enforced.** Source must be Unicode Normalization
//! Form C (§2). A non-NFC file is a lexical *error*, never silently
//! renormalized: two files that look identical must not behave differently
//! because an editor saved one in another normalization form.
//!
//! | Module | Scans |
//! |---|---|
//! | `token` | the output types: [`Token`], [`TokenKind`], [`Comment`] |
//! | `scanner` | the loop, whitespace, comments and identifiers |
//! | `number` | integer and real literals |
//! | `string` | string literals and escapes |
//! | `glyph` | fixed glyphs, glyph-spelled names and host effects |

mod glyph;
mod number;
mod scanner;
mod string;
mod token;

pub use token::{Comment, Token, TokenKind};

use obfusku_diagnostics::{Diagnostic, SourceId, Span};
use std::iter::Peekable;
use std::str::CharIndices;
use unicode_normalization::{UnicodeNormalization, is_nfc};

/// Tokenizes `source`, stopping at the first lexical error.
pub fn tokenize(source: &str, source_id: SourceId) -> Result<Vec<Token>, Diagnostic> {
    tokenize_with_comments(source, source_id).map(|(tokens, _)| tokens)
}

/// [`tokenize`], also returning every comment in source order.
///
/// Comments are not tokens, so the parser never sees them; the formatter
/// re-attaches them to declarations by position.
pub fn tokenize_with_comments(
    source: &str,
    source_id: SourceId,
) -> Result<(Vec<Token>, Vec<Comment>), Diagnostic> {
    if let Some(at) = first_non_nfc_offset(source) {
        return Err(Diagnostic::error(
            "source is not in Unicode Normalization Form C (NFC) — \
             CONCRETE_SYMBOLIC_GRAMMAR.md §2 requires NFC and treats a \
             non-NFC file as a lexical error rather than silently \
             renormalizing it",
            Span::point(source_id, at as u32),
        ));
    }
    let mut lexer = Lexer::new(source, source_id);
    let tokens = lexer.run()?;
    let comments = lexer
        .comments
        .iter()
        .map(|&(from, to)| Comment {
            span: Span::new(source_id, from as u32, to as u32),
            text: source[from..to].trim_end().to_string(),
        })
        .collect();
    Ok((tokens, comments))
}

/// The byte offset where `source` first diverges from its own NFC form, or
/// `None` if it is already NFC.
///
/// `is_nfc` alone says *that* it fails; a useful span needs *where*. NFC is
/// not purely per-character, but disagreement always surfaces at some
/// concrete character, found by comparing against the normalized text.
fn first_non_nfc_offset(source: &str) -> Option<usize> {
    if is_nfc(source) {
        return None;
    }
    let normalized: String = source.nfc().collect();
    source
        .char_indices()
        .zip(normalized.chars().chain(std::iter::repeat('\0')))
        .find(|((_, c), n)| c != n)
        .map(|((i, _), _)| i)
        .or(Some(0))
}

/// Scanner state, shared by the `impl` blocks in the submodules.
struct Lexer<'a> {
    source: &'a str,
    source_id: SourceId,
    chars: Peekable<CharIndices<'a>>,
    len: usize,
    /// Byte ranges of every comment skipped, in source order.
    comments: Vec<(usize, usize)>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, source_id: SourceId) -> Self {
        Self {
            source,
            source_id,
            chars: source.char_indices().peekable(),
            len: source.len(),
            comments: Vec::new(),
        }
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.source_id, start as u32, end as u32)
    }

    fn error(&self, at: usize, message: impl Into<String>) -> Diagnostic {
        Diagnostic::error(message, self.span(at, at))
    }
}

/// Helpers for the unit tests of this module group.
#[cfg(test)]
mod testing {
    use super::*;

    /// The token kinds of `source`, without the final `Eof`.
    pub(super) fn kinds(source: &str) -> Vec<TokenKind> {
        let mut kinds: Vec<_> = tokenize(source, SourceId(0))
            .unwrap_or_else(|d| panic!("lex error in {source:?}: {}", d.message))
            .into_iter()
            .map(|t| t.kind)
            .collect();
        assert_eq!(kinds.pop(), Some(TokenKind::Eof));
        kinds
    }

    /// The message of the lexical error `source` produces.
    pub(super) fn error(source: &str) -> String {
        match tokenize(source, SourceId(0)) {
            Err(d) => d.message,
            Ok(t) => panic!("expected a lex error in {source:?}, got {t:?}"),
        }
    }

    pub(super) fn ident(name: &str) -> TokenKind {
        TokenKind::Ident(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn the_token_stream_always_ends_with_eof_at_the_end_of_the_source() {
        let source = "x ≔ 1";
        let tokens = tokenize(source, SourceId(0)).unwrap();
        let last = tokens.last().unwrap();
        assert_eq!(last.kind, TokenKind::Eof);
        assert_eq!(last.span, Span::point(SourceId(0), source.len() as u32));
    }

    #[test]
    fn spans_are_byte_offsets_across_multi_byte_glyphs() {
        let tokens = tokenize("x ≔ 5", SourceId(0)).unwrap();
        let bind = &tokens[1];
        assert_eq!(bind.kind, TokenKind::Bind);
        assert_eq!((bind.span.start, bind.span.end), (2, 5));
    }

    #[test]
    fn a_line_comment_is_collected_not_tokenized() {
        let (tokens, comments) = tokenize_with_comments("x // note\n≔", SourceId(0)).unwrap();
        assert_eq!(tokens.len(), 3, "x, ≔, Eof");
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, "// note");
    }

    #[test]
    fn block_comments_nest() {
        let (_, comments) = tokenize_with_comments("⌈ a ⌈ b ⌉ c ⌉ x", SourceId(0)).unwrap();
        assert_eq!(comments[0].text, "⌈ a ⌈ b ⌉ c ⌉");
        assert_eq!(kinds("⌈ a ⌈ b ⌉ c ⌉ x"), [ident("x")]);
    }

    #[test]
    fn an_unterminated_block_comment_is_an_error() {
        assert!(error("⌈ never closed").contains("unterminated block comment"));
    }

    #[test]
    fn comments_come_back_in_source_order() {
        let (_, comments) = tokenize_with_comments("// a\nx // b\n// c\n", SourceId(0)).unwrap();
        let texts: Vec<_> = comments.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["// a", "// b", "// c"]);
    }

    #[test]
    fn non_nfc_source_is_rejected_at_the_first_divergence() {
        // "e" + COMBINING ACUTE composes to "é" in NFC.
        let source = "ab e\u{301}";
        let err = tokenize(source, SourceId(0)).unwrap_err();
        assert!(err.message.contains("NFC"));
        assert_eq!(err.primary.start, 3);
    }

    #[test]
    fn nfc_source_with_combining_free_text_is_accepted() {
        assert_eq!(kinds("é"), [ident("é")]);
    }

    #[test]
    fn an_unexpected_character_is_reported_with_its_position() {
        let err = tokenize("x ≔ @", SourceId(0)).unwrap_err();
        assert!(err.message.contains("unexpected character '@'"));
        assert_eq!(err.primary.start, 6);
    }
}
