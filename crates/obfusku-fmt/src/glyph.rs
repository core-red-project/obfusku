//! Every glyph the formatter emits, named once.
//!
//! The spellings come from `CONCRETE_SYMBOLIC_GRAMMAR.md`; the lexer is their
//! other consumer. Keeping them here (and written as escapes, since several
//! are visually confusable) means a change to the language's alphabet is a
//! one-file edit and the printers read as grammar, not as a pile of symbols.

use obfusku_syntax::ast::{BaseType, BinOp, UnOp};

/// `≔`: introduces a value or a type body.
pub(crate) const BIND: &str = "\u{2254}";
/// `⚙︎`: rebinding a mutable name. Carries the text-presentation selector.
pub(crate) const REBIND: &str = "\u{2699}\u{FE0E}";
/// `→`: function arrows, lambda and match-arm bodies.
pub(crate) const ARROW: &str = "\u{2192}";
/// `▷`: pipe and type application.
pub(crate) const PIPE: &str = "\u{25B7}";
/// `⟢` between spaces: separates record fields and sum variants.
pub(crate) const FIELD_SEPARATOR: &str = " \u{27E2} ";
/// `⟢`: separates match arms (followed by a line break, not a space).
pub(crate) const ARM_SEPARATOR: &str = "\u{27E2}";

/// `˚`: marks a mutable binding or an explicit capture.
pub(crate) const MUTABLE: char = '\u{02DA}';
/// `⟳`: marks an exported declaration.
pub(crate) const EXPORT: char = '\u{27F3}';
/// `⟲`: introduces an import.
pub(crate) const IMPORT: char = '\u{27F2}';
/// `☈`: introduces an exception declaration.
pub(crate) const EXCEPTION: char = '\u{2608}';
/// `λ`: functions and lambdas.
pub(crate) const LAMBDA: char = '\u{03BB}';
/// `⟡`: introduces a match.
pub(crate) const MATCH: char = '\u{27E1}';
/// `☄`: raises an exception.
pub(crate) const RAISE: char = '\u{2604}';
/// `☊`: introduces a catch.
pub(crate) const CATCH: char = '\u{260A}';
/// `•`: an unfilled argument in a partial application.
pub(crate) const HOLE: char = '\u{2022}';
/// `◈`: the piped subject inside a pipe stage.
pub(crate) const PIPE_SUBJECT: char = '\u{25C8}';
/// `◉`: the boolean true literal.
pub(crate) const TRUE: char = '\u{25C9}';
/// `◎`: the boolean false literal.
pub(crate) const FALSE: char = '\u{25CE}';
/// `∅`: the unit literal and the unit type.
pub(crate) const UNIT: char = '\u{2205}';
/// `❧`: ends every module.
pub(crate) const END_OF_MODULE: char = '\u{2767}';

/// `−`: the unary minus glyph, also used to spell the one literal that has no
/// positive counterpart.
pub(crate) const MIN_INT_LITERAL: &str = "\u{2212}9223372036854775808";

/// The glyph of a base type.
pub(crate) const fn base_type(ty: BaseType) -> &'static str {
    match ty {
        BaseType::Int => "\u{27C1}",  // ⟁
        BaseType::Real => "\u{29C6}", // ⧆
        BaseType::Str => "\u{2318}",  // ⌘
        BaseType::Bool => "\u{25CB}", // ○
        BaseType::Unit => "\u{2205}", // ∅
    }
}

/// The glyph of a binary operator. Binary subtraction (`☠︎`) is deliberately
/// a different glyph from unary negation (`−`), so no spacing rule is needed
/// to tell `a ☠︎ b` from `a −b`.
pub(crate) const fn binary(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "\u{271A}",
        BinOp::Sub => "\u{2620}\u{FE0E}",
        BinOp::Mul => "\u{2731}",
        BinOp::Div => "\u{00F7}",
        BinOp::Mod => "\u{2317}",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "\u{2264}",
        BinOp::Ge => "\u{2265}",
        BinOp::Eq => "\u{2261}",
        BinOp::NotEq => "\u{2260}",
        BinOp::And => "\u{2227}",
        BinOp::Or => "\u{2228}",
        BinOp::Xor => "\u{22BB}",
    }
}

/// The glyph of a unary operator.
pub(crate) const fn unary(op: UnOp) -> &'static str {
    match op {
        UnOp::Not => "\u{00AC}",
        UnOp::Neg => "\u{2212}",
    }
}
