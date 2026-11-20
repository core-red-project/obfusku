//! Type annotations (`CONCRETE_SYMBOLIC_GRAMMAR.md` §9).

use super::{PResult, Parser, starts_uppercase};
use crate::ast::{BaseType, TypeRef};
use crate::lexer::TokenKind;

impl Parser<'_> {
    /// `Type ::= BaseType | Tag | FunctionType | TypeApplication | "(" Type ")"`.
    ///
    /// Descends through `→` first (the loosest) and `▷` (tighter) below it.
    pub(super) fn parse_type_ref(&mut self) -> PResult<TypeRef> {
        self.nested(Self::parse_function_type)
    }

    /// Right-associative: `Int → Int → Int` is `Int → (Int → Int)`.
    fn parse_function_type(&mut self) -> PResult<TypeRef> {
        let lhs = self.parse_type_application()?;
        if self.eat(&TokenKind::Arrow) {
            let rhs = self.parse_function_type()?;
            let span = lhs.span().merge(rhs.span());
            Ok(TypeRef::Function(Box::new(lhs), Box::new(rhs), span))
        } else {
            Ok(lhs)
        }
    }

    /// Left-associative: `Array ▷ Int ▷ Real` is `(Array ▷ Int) ▷ Real`.
    ///
    /// Also the entry point for a function declaration's return type: the
    /// full [`parse_type_ref`](Self::parse_type_ref) would let a function-typed
    /// return swallow the declaration's own body-introducing `→`, and the
    /// parser does not backtrack. A function-typed return needs explicit
    /// parentheses: `λf(…): (⟁ → ⟁) → body`.
    pub(super) fn parse_type_application(&mut self) -> PResult<TypeRef> {
        let mut ty = self.parse_type_primary()?;
        while self.eat(&TokenKind::Pipe) {
            let rhs = self.parse_type_primary()?;
            let span = ty.span().merge(rhs.span());
            ty = TypeRef::Apply(Box::new(ty), Box::new(rhs), span);
        }
        Ok(ty)
    }

    fn parse_type_primary(&mut self) -> PResult<TypeRef> {
        let span = self.peek_span();
        let base = match self.peek() {
            TokenKind::TyInt => Some(BaseType::Int),
            TokenKind::TyReal => Some(BaseType::Real),
            TokenKind::TyStr => Some(BaseType::Str),
            TokenKind::TyBool => Some(BaseType::Bool),
            TokenKind::TyUnit => Some(BaseType::Unit),
            _ => None,
        };
        if let Some(base) = base {
            self.advance();
            return Ok(TypeRef::Base(base, span));
        }
        match self.peek().clone() {
            TokenKind::Ident(name) if starts_uppercase(&name) => {
                self.advance();
                Ok(TypeRef::Named(name, span))
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(TypeRef::Var(name, span))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_type_ref()?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            other => Err(self.error(format!("expected a type, found {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{BaseType, Declaration, TypeRef};
    use crate::parser::testing::{error, module};

    fn annotation(ty: &str) -> TypeRef {
        match module(&format!("x: {ty} ≔ 0\n❧")).declarations.remove(0) {
            Declaration::Value(v) => v.ty.expect("an annotation"),
            other => panic!("expected a value, got {other:?}"),
        }
    }

    #[test]
    fn each_base_glyph_is_a_base_type() {
        let cases = [
            ("⟁", BaseType::Int),
            ("⧆", BaseType::Real),
            ("⌘", BaseType::Str),
            ("○", BaseType::Bool),
            ("∅", BaseType::Unit),
        ];
        for (glyph, expected) in cases {
            assert!(matches!(annotation(glyph), TypeRef::Base(b, _) if b == expected));
        }
    }

    #[test]
    fn uppercase_is_a_named_type_and_lowercase_a_variable() {
        assert!(matches!(annotation("Foo"), TypeRef::Named(n, _) if n == "Foo"));
        assert!(matches!(annotation("a"), TypeRef::Var(n, _) if n == "a"));
    }

    #[test]
    fn arrows_associate_to_the_right() {
        let TypeRef::Function(_, rhs, _) = annotation("⟁ → ⟁ → ⟁") else {
            panic!("expected a function type");
        };
        assert!(matches!(*rhs, TypeRef::Function(..)));
    }

    #[test]
    fn application_associates_to_the_left_and_binds_tighter_than_arrows() {
        let TypeRef::Apply(lhs, _, _) = annotation("Pair ▷ ⟁ ▷ ⌘") else {
            panic!("expected an application");
        };
        assert!(matches!(*lhs, TypeRef::Apply(..)));
        assert!(matches!(annotation("A ▷ ⟁ → ⟁"), TypeRef::Function(..)));
    }

    #[test]
    fn parentheses_group() {
        let TypeRef::Function(lhs, _, _) = annotation("(⟁ → ⟁) → ⟁") else {
            panic!("expected a function type");
        };
        assert!(matches!(*lhs, TypeRef::Function(..)));
    }

    #[test]
    fn a_missing_type_is_reported_with_what_was_found() {
        assert!(error("x: ≔ 1\n❧").contains("expected a type"));
    }

    #[test]
    fn an_unclosed_group_is_an_error() {
        assert!(error("x: (⟁ ≔ 1\n❧").contains("expected ')'"));
    }

    #[test]
    fn spans_cover_the_whole_annotation() {
        let ty = annotation("⟁ → ⟁");
        assert_eq!(ty.span().end - ty.span().start, "⟁ → ⟁".len() as u32);
    }
}
