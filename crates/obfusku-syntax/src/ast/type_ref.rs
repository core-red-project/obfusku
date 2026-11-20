//! Type annotations as written (`CONCRETE_SYMBOLIC_GRAMMAR.md` §9).

use obfusku_diagnostics::Span;

/// A type annotation.
#[derive(Debug, Clone)]
pub enum TypeRef {
    /// `BaseType ::= "⟁" | "⧆" | "⌘" | "○" | "∅"`.
    ///
    /// A distinct production from [`TypeRef::Named`], not an alias for the
    /// words `Int`, `Real`, …: those are ordinary tags, so a program may
    /// declare its own `Int` without clashing with `⟁`.
    Base(BaseType, Span),
    /// An uppercase name: another declared ADT.
    Named(String, Span),
    /// A lowercase name: one of the enclosing declaration's type parameters.
    Var(String, Span),
    /// `Type "→" Type`, right-associative.
    Function(Box<TypeRef>, Box<TypeRef>, Span),
    /// `Type "▷" Type`, left-associative and tighter than `→`:
    /// `Array ▷ Int` is `Array<Int>`.
    Apply(Box<TypeRef>, Box<TypeRef>, Span),
}

impl TypeRef {
    /// The whole annotation.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            TypeRef::Base(_, s) | TypeRef::Named(_, s) | TypeRef::Var(_, s) => *s,
            TypeRef::Function(_, _, s) | TypeRef::Apply(_, _, s) => *s,
        }
    }
}

/// The five built-in types, each with its own glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseType {
    /// `⟁`
    Int,
    /// `⧆`
    Real,
    /// `⌘`
    Str,
    /// `○`
    Bool,
    /// `∅`
    Unit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::SourceId;

    fn at(start: u32, end: u32) -> Span {
        Span::new(SourceId(0), start, end)
    }

    #[test]
    fn every_form_reports_its_own_span() {
        let base = TypeRef::Base(BaseType::Int, at(0, 1));
        let named = TypeRef::Named("T".into(), at(2, 3));
        let var = TypeRef::Var("a".into(), at(4, 5));
        let function = TypeRef::Function(Box::new(base.clone()), Box::new(named.clone()), at(0, 3));
        let apply = TypeRef::Apply(Box::new(named.clone()), Box::new(var.clone()), at(2, 5));
        assert_eq!(base.span(), at(0, 1));
        assert_eq!(named.span(), at(2, 3));
        assert_eq!(var.span(), at(4, 5));
        assert_eq!(function.span(), at(0, 3));
        assert_eq!(apply.span(), at(2, 5));
    }
}
