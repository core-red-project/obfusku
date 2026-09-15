//! source → tokens — `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §2–§5.
//!
//! `⚙︎` (rebind) is genuinely two codepoints — U+2699 GEAR followed by
//! U+FE0E (a text-presentation variation selector) — handled via
//! explicit two-codepoint lookahead rather than single-`char` dispatch.

use obfusku_diagnostics::{Diagnostic, Severity, SourceId, Span};
use std::iter::Peekable;
use std::str::CharIndices;
use unicode_normalization::{is_nfc, UnicodeNormalization};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Ident(String),
    Bind,    // ≔
    Lambda,  // λ
    Arrow,   // →
    Pipe,    // ▷
    PipeRef, // ◈
    Hole,    // • (§7.2's Argument hole — CONCRETE_SYMBOLIC_GRAMMAR.md §7.2/§12.1)
    Seal,    // ❧
    Mut,     // ˚ (postfix on '≔') / ReferenceForm prefix on a name
    Export,  // ⟳
    Import,  // ⟲
    Rebind,  // ⚙︎ (U+2699 U+FE0E)
    Match,   // ⟡ — match introducer (§11)
    ArmSep,  // ⟢ — generic declarative-list separator (§6): TypeBody
    // variants/fields and Match arms alike, per §6's "one
    // generic mechanism reused everywhere" rule.
    Wildcard, // ASCII '_' alone — reserved, never an ordinary identifier
    // (§10); the lexer recognizes it specially rather than
    // handing the parser an Ident("_") to special-case.
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Plus,    // ✚
    Minus,   // ☠︎ (U+2620 U+FE0E) — binary subtract
    Star,    // ✱
    Slash,   // ÷
    Percent, // ⌗ — modulo
    Neg,     // − (U+2212) — unary negation, distinct glyph from binary Minus
    Not,     // ¬ — unary logical not
    Lt,      // <
    Gt,      // >
    Le,      // ≤ (ADR-016)
    Ge,      // ≥ (ADR-016)
    EqEq,    // ≡ (ADR-016)
    NotEq,   // ≠ (ADR-016)
    And,     // ∧
    Or,      // ∨
    Xor,     // ⊻
    Raise,   // ☄
    Catch,   // ☊
    // Distinct tokens from `Ident` — `Int` (a Tag) is never confusable
    // with `⟁` (this token).
    TyInt,  // ⟁
    TyReal, // ⧆
    TyStr,  // ⌘
    TyBool, // ○
    TyUnit, // ∅
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// Tokenize source text. Stops at the first lexical error.
///
/// §2: source must be Unicode Normalization Form C — checked once, up
/// front, over the whole file, rather than silently renormalized (two
/// byte-identical-looking files must not behave differently depending on
/// which one an editor happened to save in a different normalization
/// form).
pub fn tokenize(source: &str, source_id: SourceId) -> Result<Vec<Token>, Diagnostic> {
    if let Some(at) = first_non_nfc_offset(source) {
        return Err(Diagnostic {
            severity: Severity::Error,
            message: "source is not in Unicode Normalization Form C (NFC) — \
                      CONCRETE_SYMBOLIC_GRAMMAR.md §2 requires NFC and treats a \
                      non-NFC file as a lexical error rather than silently \
                      renormalizing it"
                .to_string(),
            primary: Span {
                source: source_id,
                start: at as u32,
                end: at as u32,
            },
        });
    }
    Lexer::new(source, source_id).run()
}

/// The byte offset of the first character where `source` diverges from
/// its own NFC form, or `None` if `source` is already NFC. `is_nfc` alone
/// would tell us *that* it fails; producing a useful span needs the
/// actual divergence point, found by comparing char-by-char against the
/// normalized string (NFC normalization is not purely per-character, but
/// disagreement always surfaces at some concrete character position).
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

struct Lexer<'a> {
    source: &'a str,
    source_id: SourceId,
    chars: Peekable<CharIndices<'a>>,
    len: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, source_id: SourceId) -> Self {
        Self {
            source,
            source_id,
            chars: source.char_indices().peekable(),
            len: source.len(),
        }
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span {
            source: self.source_id,
            start: start as u32,
            end: end as u32,
        }
    }

    fn error(&self, at: usize, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            primary: self.span(at, at),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
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

    fn skip_trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            while let Some(&(_, c)) = self.chars.peek() {
                if c.is_whitespace() {
                    self.chars.next();
                } else {
                    break;
                }
            }

            if let Some(&(_, '/')) = self.chars.peek() {
                let mut lookahead = self.chars.clone();
                lookahead.next();
                if let Some(&(_, '/')) = lookahead.peek() {
                    while let Some(&(_, c)) = self.chars.peek() {
                        self.chars.next();
                        if c == '\n' {
                            break;
                        }
                    }
                    continue;
                }
            }

            if let Some(&(start, '⌈')) = self.chars.peek() {
                self.chars.next();
                let mut depth = 1;
                while depth > 0 {
                    match self.chars.next() {
                        Some((_, '⌈')) => depth += 1,
                        Some((_, '⌉')) => depth -= 1,
                        Some(_) => {}
                        None => return Err(self.error(start, "unterminated block comment")),
                    }
                }
                continue;
            }

            break;
        }
        Ok(())
    }

    fn scan_number(&mut self, start: usize) -> Result<Token, Diagnostic> {
        let mut end = start;
        while let Some(&(i, c)) = self.chars.peek() {
            if c.is_ascii_digit() {
                self.chars.next();
                end = i + c.len_utf8();
            } else {
                break;
            }
        }

        let mut is_real = false;

        if let Some(&(_, '.')) = self.chars.peek() {
            let mut lookahead = self.chars.clone();
            lookahead.next();
            if matches!(lookahead.peek(), Some((_, d)) if d.is_ascii_digit()) {
                is_real = true;
                self.chars.next(); // consume '.'
                while let Some(&(i, c)) = self.chars.peek() {
                    if c.is_ascii_digit() {
                        self.chars.next();
                        end = i + c.len_utf8();
                    } else {
                        break;
                    }
                }
            }
        }

        // §5: `RealLiteral`'s exponent suffix is only reachable *after*
        // the mandatory `"." digit+` fractional part — `IntLiteral` has
        // no exponent production at all. Without the `is_real` guard, a
        // bare `5e10` (no dot) would wrongly lex as one `Real` token
        // instead of `Int(5)` followed by an `Ident("e10")`.
        if is_real && matches!(self.chars.peek(), Some((_, 'e')) | Some((_, 'E'))) {
            self.chars.next();
            if matches!(self.chars.peek(), Some((_, '+')) | Some((_, '-'))) {
                self.chars.next();
            }
            while let Some(&(i, c)) = self.chars.peek() {
                if c.is_ascii_digit() {
                    self.chars.next();
                    end = i + c.len_utf8();
                } else {
                    break;
                }
            }
        }

        let text = &self.source[start..end];
        if is_real {
            let value: f64 = text
                .parse()
                .map_err(|_| self.error(start, "invalid real literal"))?;
            Ok(Token {
                kind: TokenKind::Real(value),
                span: self.span(start, end),
            })
        } else {
            let value: i64 = text
                .parse()
                .map_err(|_| self.error(start, "invalid integer literal"))?;
            Ok(Token {
                kind: TokenKind::Int(value),
                span: self.span(start, end),
            })
        }
    }

    fn scan_string(&mut self, start: usize) -> Result<Token, Diagnostic> {
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
                // production — exactly these six, nothing else.
                Some((i, '\\')) => match self.chars.next() {
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, 'r')) => value.push('\r'),
                    Some((_, '0')) => value.push('\0'),
                    Some((_, other)) => {
                        return Err(
                            self.error(i, format!("unsupported escape sequence '\\{other}'"))
                        )
                    }
                    None => return Err(self.error(start, "unterminated string literal")),
                },
                Some((_, c)) => value.push(c),
            }
        }
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

    /// Two-codepoint glyphs: a base character immediately followed by
    /// U+FE0E (a text-presentation variation selector). `⚙︎` (rebind) was
    /// the first of these; `☠︎` (binary subtract, §20.2) is the second —
    /// both handled by this one table-driven scanner rather than
    /// duplicating the two-codepoint-lookahead logic per glyph.
    fn scan_variation_selector_glyph(&mut self, start: usize, c: char) -> Option<Token> {
        let kind = match c {
            '\u{2699}' => TokenKind::Rebind,
            '\u{2620}' => TokenKind::Minus,
            _ => return None,
        };
        let mut lookahead = self.chars.clone();
        lookahead.next();
        if !matches!(lookahead.peek(), Some((_, '\u{FE0E}'))) {
            return None;
        }
        self.chars.next(); // consume the base character
        let (vs_offset, vs_char) = self.chars.next().expect("checked by lookahead above");
        Some(Token {
            kind,
            span: self.span(start, vs_offset + vs_char.len_utf8()),
        })
    }

    /// Canon glyph names (`GLYPH_SYSTEM_DESIGN.md` §10.3/§10.4/§10.5):
    /// `⊘`/`⁝` are `List`'s `Nil`/`Cons` constructors, `⦰`/`⧫` are
    /// `Optional`'s `None`/`Some` constructors, `✓`/`✗` are `Result`'s
    /// `Ok`/`Err` constructors, `⟐`/`⌿`/`⌽` are the ambient
    /// `map`/`filter`/`fold` combinators over `List`, `⊡`/`⊟`/`⊞`/`#`/`⊙`
    /// are `Array`'s `map`/`filter`/`fold`/`length`/`set` natives. These
    /// reuse the ordinary `Ident` token rather than each minting a
    /// dedicated `TokenKind` — they behave exactly like any other
    /// prelude name (constructor Tag or ordinary reference), just
    /// spelled with a symbol instead of a word; `starts_uppercase` in
    /// the parser treats a symbol-led name as a Tag the same way it
    /// treats an uppercase letter.
    fn scan_glyph_ident(&mut self, start: usize, c: char) -> Option<Token> {
        let name = match c {
            '⊘' | '⁝' | '⦰' | '⧫' | '✓' | '✗' | '⟐' | '⌿' | '⌽' | '⊡' | '⊟' | '⊞' | '#' | '⊙'
            | '↗' | '↘' => c.to_string(),
            _ => return None,
        };
        self.chars.next();
        Some(Token {
            kind: TokenKind::Ident(name),
            span: self.span(start, start + c.len_utf8()),
        })
    }

    /// Host-effect family (`GLYPH_SYSTEM_DESIGN.md` §10.2): `⌁` (host
    /// boundary) composes with an independent direction mark (`↑`
    /// emit/`↓` read) and an optional medium mark (`⌬` file, console by
    /// default) into one name — `⌁↑`, `⌁↓`, `⌁↑⌬`, `⌁↓⌬` — the same way
    /// `≔˚⟳` composes the Binder family's independent postfix marks.
    /// Unlike those, this composition spells a single `Ident`, not
    /// several tokens: `print`/`readLine`/`readFile`/`writeFile` are
    /// ordinary prelude names looked up by their full string, so the
    /// lexer must produce that whole string as one token. An invalid or
    /// incomplete combination (bare `⌁`, or `⌁` with no matching host
    /// native) is not rejected here — it lexes fine and simply fails to
    /// resolve later as an unbound identifier, the same as any other
    /// misspelled name.
    fn scan_host_effect(&mut self, start: usize, c: char) -> Option<Token> {
        if c != '⌁' {
            return None;
        }
        self.chars.next();
        let mut end = start + c.len_utf8();
        if let Some(&(i, dir @ ('↑' | '↓'))) = self.chars.peek() {
            self.chars.next();
            end = i + dir.len_utf8();
            if let Some(&(i, '⌬')) = self.chars.peek() {
                self.chars.next();
                end = i + '⌬'.len_utf8();
            }
        }
        let name = self.source[start..end].to_string();
        Some(Token {
            kind: TokenKind::Ident(name),
            span: self.span(start, end),
        })
    }

    fn scan_glyph(&mut self, start: usize, c: char) -> Option<Token> {
        let kind = match c {
            '≔' => TokenKind::Bind,
            'λ' => TokenKind::Lambda,
            '→' => TokenKind::Arrow,
            '▷' => TokenKind::Pipe,
            '◈' => TokenKind::PipeRef,
            '•' => TokenKind::Hole,
            '❧' => TokenKind::Seal,
            '˚' => TokenKind::Mut,
            '⟳' => TokenKind::Export,
            '⟲' => TokenKind::Import,
            '⟡' => TokenKind::Match,
            '⟢' => TokenKind::ArmSep,
            '◉' => TokenKind::Bool(true),
            '◎' => TokenKind::Bool(false),
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '✚' => TokenKind::Plus,
            '✱' => TokenKind::Star,
            '÷' => TokenKind::Slash,
            '⌗' => TokenKind::Percent,
            '−' => TokenKind::Neg,
            '¬' => TokenKind::Not,
            '∧' => TokenKind::And,
            '∨' => TokenKind::Or,
            '⊻' => TokenKind::Xor,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            '≤' => TokenKind::Le,
            '≥' => TokenKind::Ge,
            '≡' => TokenKind::EqEq,
            '≠' => TokenKind::NotEq,
            '☄' => TokenKind::Raise,
            '☊' => TokenKind::Catch,
            '⟁' => TokenKind::TyInt,
            '⧆' => TokenKind::TyReal,
            '⌘' => TokenKind::TyStr,
            '○' => TokenKind::TyBool,
            '∅' => TokenKind::TyUnit,
            _ => return None,
        };
        self.chars.next();
        Some(Token {
            kind,
            span: self.span(start, start + c.len_utf8()),
        })
    }
}
