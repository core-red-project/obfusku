//! Glyph recognition: every fixed symbol, and the glyph-spelled names.
//!
//! Tried *before* identifiers: several glyphs (notably `λ`) are
//! Unicode-alphabetic and would otherwise be scanned as the start of a name.

use super::Lexer;
use super::token::{Token, TokenKind};

impl Lexer<'_> {
    /// Two-codepoint glyphs: a base character immediately followed by
    /// U+FE0E (a text-presentation variation selector). `⚙︎` (rebind) was
    /// the first of these; `☠︎` (binary subtract, §20.2) is the second —
    /// both handled by this one table-driven scanner rather than
    /// duplicating the two-codepoint-lookahead logic per glyph.
    pub(super) fn scan_variation_selector_glyph(&mut self, start: usize, c: char) -> Option<Token> {
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
    pub(super) fn scan_glyph_ident(&mut self, start: usize, c: char) -> Option<Token> {
        // `⌭` alone is the bytes type; `⌭` + a role glyph names the byte
        // twin of the Str/Array operation with that glyph (`ADR-027`).
        if c == '⌭' {
            let mut ahead = self.chars.clone();
            ahead.next();
            let (len, name) = match ahead.peek() {
                Some(&(_, r @ ('#' | '⌷' | '⍪' | '⊠' | '⇋' | '⇌' | '⌕' | '⌘' | '⍕'))) => {
                    (c.len_utf8() + r.len_utf8(), format!("{c}{r}"))
                }
                _ => (c.len_utf8(), c.to_string()),
            };
            self.chars.next();
            if len > c.len_utf8() {
                self.chars.next();
            }
            return Some(Token {
                kind: TokenKind::Ident(name),
                span: self.span(start, start + len),
            });
        }
        let name = match c {
            '⊘' | '⁝' | '⦰' | '⧫' | '✓' | '✗' | '⟐' | '⌿' | '⌽' | '⊡' | '⊟' | '⊞' | '#' | '⊙'
            | '↗' | '↘' | '⫽' | '⫴' | '⌫' | '∋' | '⍴' | '⊏' | '↦' | '⇝' | '⍕' | '⚑' | '⊕' | '⍪'
            | '⌷' | '⍳' | '⊠' | '⍋' | '⇋' | '⇌' | '⍬' | '⊎' | '⊖' | '⌕' | '⊛' | '⌸' | '⌹' | '⍭'
            | '⊌' | '⊝' | '∈' | '⊚' | '⌻' | '⌑' | '➤' | '◇' | '∥' | '⋆' | '⊓' | '⊔' | '⊗' | '∼'
            | '⋘' | '⋙' | '⊥' | '⊤' | '√' | '⌊' | '⌃' | '⌾' | '⋇' | 'ℯ' | 'ℓ' | '∿' | '∾' | '∠'
            | '⍥' | '∦' | 'ϖ' | '∞' | '⌶' | '⍂' | '⌯' | '⌮' | '⇧' | '⇩' | '⤳' | '⊢' | '⊣' | '⍸' => {
                c.to_string()
            }
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
    pub(super) fn scan_host_effect(&mut self, start: usize, c: char) -> Option<Token> {
        if c != '⌁' {
            return None;
        }
        self.chars.next();
        let mut end = start + c.len_utf8();
        if let Some(&(i, dir @ ('↑' | '↓' | '↯' | '⍰' | '⊕' | '⊖' | '⇄' | '⧉'))) = self.chars.peek()
        {
            self.chars.next();
            end = i + dir.len_utf8();
            // `⌬` file, `⌥` program arguments, `⎈` environment (`ADR-022`),
            // `⍉` error stream (`ADR-023`), `⌂` directory, `⌭` bytes (`ADR-024`),
            // `⎙` text stream, `⎚` byte stream, `⌖` terminal, `⧗` wall clock,
            // `⧖` monotonic clock, `⚀` integer dice, `⚁` real dice (`ADR-027`).
            if let Some(&(
                i,
                medium @ ('⌬' | '⌥' | '⎈' | '⍉' | '⌂' | '⌭' | '⎙' | '⎚' | '⌖' | '⧗' | '⧖' | '⚀'
                | '⚁'),
            )) = self.chars.peek()
            {
                self.chars.next();
                end = i + medium.len_utf8();
            }
        }
        let name = self.source[start..end].to_string();
        Some(Token {
            kind: TokenKind::Ident(name),
            span: self.span(start, end),
        })
    }

    pub(super) fn scan_glyph(&mut self, start: usize, c: char) -> Option<Token> {
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
            '☈' => TokenKind::ExceptionDecl,
            '.' => TokenKind::Dot,
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

#[cfg(test)]
mod tests {
    use crate::lexer::TokenKind::*;
    use crate::lexer::testing::{ident, kinds};

    #[test]
    fn fixed_glyphs_map_to_their_roles() {
        assert_eq!(
            kinds("≔ λ → ▷ ◈ • ❧ ˚ ⟳ ⟲ ⟡ ⟢"),
            [
                Bind, Lambda, Arrow, Pipe, PipeRef, Hole, Seal, Mut, Export, Import, Match, ArmSep
            ]
        );
    }

    #[test]
    fn lambda_is_a_glyph_not_the_start_of_a_name() {
        assert_eq!(kinds("λf"), [Lambda, ident("f")]);
    }

    #[test]
    fn two_codepoint_glyphs_need_the_variation_selector() {
        assert_eq!(kinds("\u{2699}\u{FE0E}"), [Rebind]);
        assert_eq!(kinds("\u{2620}\u{FE0E}"), [Minus]);
    }

    #[test]
    fn a_base_character_without_the_selector_is_not_that_glyph() {
        assert!(crate::lexer::tokenize("\u{2699}", crate::SourceId(0)).is_err());
    }

    #[test]
    fn subtraction_and_negation_are_different_glyphs() {
        assert_eq!(
            kinds("a \u{2620}\u{FE0E} b"),
            [ident("a"), Minus, ident("b")]
        );
        assert_eq!(kinds("−b"), [Neg, ident("b")]);
    }

    #[test]
    fn operators_and_punctuation() {
        assert_eq!(
            kinds("✚ ✱ ÷ ⌗ ¬ ∧ ∨ ⊻ < > ≤ ≥ ≡ ≠"),
            [
                Plus, Star, Slash, Percent, Not, And, Or, Xor, Lt, Gt, Le, Ge, EqEq, NotEq
            ]
        );
        assert_eq!(
            kinds("( ) { } [ ] , : ."),
            [
                LParen, RParen, LBrace, RBrace, LBracket, RBracket, Comma, Colon, Dot
            ]
        );
    }

    #[test]
    fn type_glyphs_are_distinct_from_the_words() {
        assert_eq!(kinds("⟁ ⧆ ⌘ ○ ∅"), [TyInt, TyReal, TyStr, TyBool, TyUnit]);
        assert_eq!(kinds("Int"), [ident("Int")]);
    }

    #[test]
    fn booleans_exceptions_and_catch() {
        assert_eq!(
            kinds("◉ ◎ ☄ ☈ ☊"),
            [Bool(true), Bool(false), Raise, ExceptionDecl, Catch]
        );
    }

    #[test]
    fn glyph_spelled_prelude_names_are_identifiers() {
        assert_eq!(
            kinds("⊘ ⁝ ⦰ ⟐"),
            [ident("⊘"), ident("⁝"), ident("⦰"), ident("⟐")]
        );
    }

    #[test]
    fn the_bytes_glyph_composes_with_a_role_glyph_into_one_name() {
        assert_eq!(kinds("⌭"), [ident("⌭")]);
        assert_eq!(kinds("⌭#"), [ident("⌭#")]);
        assert_eq!(kinds("⌭ #"), [ident("⌭"), ident("#")]);
    }

    #[test]
    fn host_effects_compose_direction_and_medium_into_one_name() {
        assert_eq!(kinds("⌁↑"), [ident("⌁↑")]);
        assert_eq!(kinds("⌁↓⌬"), [ident("⌁↓⌬")]);
        assert_eq!(kinds("⌁↑⌥"), [ident("⌁↑⌥")]);
    }

    #[test]
    fn a_bare_host_glyph_lexes_and_fails_later_as_an_unbound_name() {
        assert_eq!(kinds("⌁"), [ident("⌁")]);
    }
}
