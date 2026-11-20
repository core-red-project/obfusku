//! Tokens to surface AST (`spec/CONCRETE_SYMBOLIC_GRAMMAR.md`).
//!
//! A hand-written recursive-descent parser with precedence climbing for
//! expressions. §15's declaration-boundary rule (newlines are insignificant;
//! a declaration ends when the next token cannot continue the current
//! expression) falls out of ordinary expression parsing, so no separate
//! algorithm is needed.
//!
//! **Fail at the first error.** Parsing stops at the first syntax error and
//! returns it (as a one-element `Vec`, to share the type with the stages
//! that can report several). After a syntax error the rest of the file has
//! no reliable structure, and a cascade of follow-on errors would bury the
//! one that matters.
//!
//! **Bounded depth.** Later passes recurse over the tree, so unbounded
//! nesting would overflow their stack. Nesting and chain length are capped
//! here, and exceeding either is a diagnostic, not a crash.
//!
//! | Module | Parses |
//! |---|---|
//! | `declaration` | the module, values, functions, types, imports, exceptions |
//! | `type_ref` | type annotations |
//! | `expression` | atoms, calls, lambdas, matches, locals, records |
//! | `operator` | the binary/unary operator ladder (§13) |
//! | `pipe` | `▷` pipelines and the `◈`/`•` stage check (§12) |
//! | `pattern` | match patterns |

mod declaration;
mod expression;
mod operator;
mod pattern;
mod pipe;
mod type_ref;

use crate::ast::Module;
use crate::lexer::{Token, TokenKind};
use obfusku_diagnostics::{Diagnostic, SourceId, Span};

/// Parses a token stream ending in `Eof` into a [`Module`].
///
/// `_source_id` is part of the signature for callers that already hold it;
/// every token carries its own span, so the parser has no use for it.
pub fn parse(tokens: &[Token], _source_id: SourceId) -> Result<Module, Vec<Diagnostic>> {
    Parser {
        tokens,
        pos: 0,
        depth: 0,
        links: 0,
    }
    .parse_module()
}

type PResult<T> = Result<T, Vec<Diagnostic>>;

/// Open nested expressions, patterns and types allowed.
const MAX_NESTING: usize = 2048;
/// Operators and calls chained in one declaration (`a ✚ b ✚ c …`). These
/// build a left-deep tree *without* nesting, so they need their own bound.
const MAX_CHAIN: usize = 100_000;

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    /// Open nested expressions, patterns and types.
    depth: usize,
    /// Operators and calls chained so far in the current declaration.
    links: usize,
}

/// §4/§8.3's disambiguation, decidable from the first character alone: an
/// uppercase name is a *Tag* (a type or constructor); anything else is a
/// value or type variable.
///
/// A canon constructor glyph (`⊘`, `⁝`, …; `GLYPH_SYSTEM_DESIGN.md` §10.3)
/// has no case, so each is admitted as a Tag explicitly rather than by any
/// "not a letter" rule: the combinator glyphs (`⟐`, `⌿`, `⌽`) are ordinary
/// value names and must *not* be mistaken for Tags.
fn starts_uppercase(name: &str) -> bool {
    name == "⌭"
        || name.chars().next().is_some_and(|c| {
            c.is_uppercase() || matches!(c, '⊘' | '⁝' | '⦰' | '⧫' | '✓' | '✗' | '⌑' | '➤' | '◇')
        })
}

impl Parser<'_> {
    // ---- cursor ----

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        let i = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[i].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    /// Consumes the current token. At `Eof` it stays put, so a parser that
    /// keeps advancing past the end cannot index out of range.
    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    /// Consumes the current token if it is `kind`.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        let matched = self.peek() == kind;
        if matched {
            self.advance();
        }
        matched
    }

    fn error(&self, message: impl Into<String>) -> Vec<Diagnostic> {
        vec![Diagnostic::error(message, self.peek_span())]
    }

    /// Consumes `kind` and returns its span, or fails naming `what`.
    fn expect(&mut self, kind: &TokenKind, what: &str) -> PResult<Span> {
        if self.peek() == kind {
            Ok(self.advance().span)
        } else {
            Err(self.error(format!("expected {what}, found {:?}", self.peek())))
        }
    }

    // ---- limits ----

    /// Counts one more operator or call in the current declaration's chains.
    fn link(&mut self) -> PResult<()> {
        self.links += 1;
        if self.links > MAX_CHAIN {
            return Err(self.error(format!(
                "more than {MAX_CHAIN} chained operators or calls in one declaration; split it up"
            )));
        }
        Ok(())
    }

    /// Runs `f` one nesting level deeper, failing past [`MAX_NESTING`].
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.depth >= MAX_NESTING {
            return Err(self.error(format!(
                "nesting is deeper than {MAX_NESTING} levels; split the expression into named bindings"
            )));
        }
        self.depth += 1;
        let out = f(self);
        self.depth -= 1;
        out
    }

    // ---- shared shapes ----

    /// One or more `item`s separated by `,`.
    fn comma_list<T>(&mut self, mut item: impl FnMut(&mut Self) -> PResult<T>) -> PResult<Vec<T>> {
        let mut items = vec![item(self)?];
        while self.eat(&TokenKind::Comma) {
            items.push(item(self)?);
        }
        Ok(items)
    }

    /// `name`, any case, with its span.
    fn ident(&mut self, expected: &str) -> PResult<(String, Span)> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                Ok((name, span))
            }
            other => Err(self.error(format!("expected {expected}, found {other:?}"))),
        }
    }

    /// A lowercase field name.
    fn field_name(&mut self) -> PResult<String> {
        match self.peek().clone() {
            TokenKind::Ident(n) if !starts_uppercase(&n) => {
                self.advance();
                Ok(n)
            }
            other => Err(self.error(format!("expected a field name, found {other:?}"))),
        }
    }

    /// `name ":" value (⟢ name ":" value)*`, the body of a record type, a
    /// named construction, a record update and a named pattern. The opening
    /// brace is already consumed; the closing one is left for the caller.
    fn named_fields<T>(
        &mut self,
        mut value: impl FnMut(&mut Self) -> PResult<T>,
    ) -> PResult<Vec<(String, T)>> {
        let mut fields = Vec::new();
        loop {
            let name = self.field_name()?;
            self.expect(&TokenKind::Colon, "':'")?;
            fields.push((name, value(self)?));
            if !self.eat(&TokenKind::ArmSep) {
                return Ok(fields);
            }
        }
    }
}

#[cfg(test)]
mod testing {
    use super::*;
    use crate::lexer::tokenize;

    /// Parses `source`, panicking with the diagnostics on failure.
    pub(super) fn module(source: &str) -> Module {
        let id = SourceId(0);
        let tokens = tokenize(source, id).unwrap_or_else(|d| panic!("lex: {}", d.message));
        parse(&tokens, id).unwrap_or_else(|d| panic!("parse {source:?}: {d:?}"))
    }

    /// The first error message `source` produces.
    pub(super) fn error(source: &str) -> String {
        let id = SourceId(0);
        let tokens = tokenize(source, id).unwrap_or_else(|d| panic!("lex: {}", d.message));
        match parse(&tokens, id) {
            Err(mut ds) => ds.remove(0).message,
            Ok(m) => panic!("expected a parse error in {source:?}, got {m:?}"),
        }
    }

    /// The expression of the single declaration `v ≔ <expression>`.
    pub(super) fn expression(source: &str) -> crate::ast::Expression {
        let m = module(&format!("v ≔ {source}\n❧"));
        match m.declarations.into_iter().next() {
            Some(crate::ast::Declaration::Value(v)) => v.value,
            other => panic!("expected a value declaration, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::ast::Declaration;

    #[test]
    fn an_empty_module_is_just_the_seal() {
        let m = module("❧");
        assert!(m.declarations.is_empty());
    }

    #[test]
    fn a_missing_seal_is_an_error_not_a_shorter_program() {
        assert!(error("x ≔ 1").contains("expected '❧' to close the module"));
    }

    #[test]
    fn declarations_need_no_separators() {
        let m = module("a ≔ 1 b ≔ 2 ❧");
        assert_eq!(m.declarations.len(), 2);
    }

    #[test]
    fn the_seal_span_is_recorded() {
        let source = "x ≔ 1 ❧";
        let m = module(source);
        assert_eq!(m.seal.start as usize, source.find('❧').unwrap());
    }

    #[test]
    fn uppercase_and_constructor_glyph_names_are_tags() {
        assert!(starts_uppercase("Foo"));
        assert!(starts_uppercase("⊘"));
        assert!(starts_uppercase("⌭"));
        assert!(!starts_uppercase("foo"));
        assert!(!starts_uppercase("⟐"), "combinators are values, not tags");
    }

    #[test]
    fn the_cursor_never_runs_past_eof() {
        let tokens = crate::lexer::tokenize("", SourceId(0)).unwrap();
        let mut p = Parser {
            tokens: &tokens,
            pos: 0,
            depth: 0,
            links: 0,
        };
        p.advance();
        p.advance();
        assert_eq!(p.peek(), &TokenKind::Eof);
        assert_eq!(p.peek_at(5), &TokenKind::Eof);
    }

    #[test]
    fn a_declaration_may_contain_a_long_chain_within_the_bound() {
        let source = format!("v ≔ {}\n❧", vec!["1"; 5000].join(" ✚ "));
        let m = module(&source);
        assert!(matches!(m.declarations[0], Declaration::Value(_)));
    }
}
