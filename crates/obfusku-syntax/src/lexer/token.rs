//! The lexer's output vocabulary.

use obfusku_diagnostics::Span;

/// What a token is. Glyphs are named by *role* (`Bind`, `Arrow`), not by
/// their codepoint, so the parser reads as grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An integer literal. `i64::MIN` also stands for the magnitude `2^63`,
    /// which the parser accepts only directly behind a unary minus.
    Int(i64),
    /// A real literal.
    Real(f64),
    /// A string literal, escapes resolved.
    Str(String),
    /// `◉` or `◎`.
    Bool(bool),
    /// A name: a word, or a glyph-spelled prelude name such as `⊘` or `⌁↑`.
    Ident(String),
    /// `≔`
    Bind,
    /// `λ`
    Lambda,
    /// `→`
    Arrow,
    /// `▷`
    Pipe,
    /// `◈`
    PipeRef,
    /// `•`: a partial-application hole (`§7.2`, `§12.1`).
    Hole,
    /// `❧`: closes a module.
    Seal,
    /// `˚`: postfix on `≔` (a mutable binding), or prefix on a name (an
    /// explicit reference).
    Mut,
    /// `⟳`
    Export,
    /// `⟲`
    Import,
    /// `⚙︎` (U+2699 U+FE0E): two codepoints, one token.
    Rebind,
    /// `⟡`: introduces a match (`§11`).
    Match,
    /// `⟢`: the one generic separator for declarative lists (`§6`): type
    /// variants, record fields and match arms alike.
    ArmSep,
    /// A lone ASCII `_`. Reserved: never an ordinary identifier, so the
    /// parser never has to special-case an `Ident("_")`.
    Wildcard,
    /// `.`: record field access and update.
    Dot,
    /// `☈`: declares a variant of the open `Exception` type.
    ExceptionDecl,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `✚`
    Plus,
    /// `☠︎` (U+2620 U+FE0E): binary subtraction.
    Minus,
    /// `✱`
    Star,
    /// `÷`
    Slash,
    /// `⌗`: modulo.
    Percent,
    /// `−` (U+2212): unary negation, a different glyph from binary `Minus`
    /// so no spacing rule is needed to tell `a ☠︎ b` from `a −b`.
    Neg,
    /// `¬`
    Not,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `≤` (`ADR-016`)
    Le,
    /// `≥` (`ADR-016`)
    Ge,
    /// `≡` (`ADR-016`)
    EqEq,
    /// `≠` (`ADR-016`)
    NotEq,
    /// `∧`
    And,
    /// `∨`
    Or,
    /// `⊻`
    Xor,
    /// `☄`
    Raise,
    /// `☊`
    Catch,
    /// `⟁`: the `Int` type. Distinct from `Ident("Int")`, which is just a
    /// tag.
    TyInt,
    /// `⧆`: the `Real` type.
    TyReal,
    /// `⌘`: the `Str` type.
    TyStr,
    /// `○`: the `Bool` type.
    TyBool,
    /// `∅`: the `Unit` type, and in expression position the `Unit` value.
    TyUnit,
    /// End of input; always the last token.
    Eof,
}

/// A token and where it came from.
#[derive(Debug, Clone)]
pub struct Token {
    /// What it is.
    pub kind: TokenKind,
    /// Its bytes in the source.
    pub span: Span,
}

/// A `//` line comment or a `⌈ … ⌉` block comment, with its delimiters.
///
/// Comments are not tokens: the parser never sees them, and the formatter
/// re-attaches them to declarations by position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Where the comment is.
    pub span: Span,
    /// Its text with trailing whitespace removed.
    pub text: String,
}
