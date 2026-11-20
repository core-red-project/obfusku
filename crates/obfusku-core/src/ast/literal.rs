//! Literal values (`SEMANTIC_CORE.md` §1, §2).

use crate::types::Type;

/// A literal written in source: the only values that exist before evaluation.
///
/// The set is closed on purpose. Everything richer (arrays, records, ADTs)
/// is built from these by constructors and operations, so the evaluator and
/// the checker each need exactly one rule per literal kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// A 64-bit signed integer. Arithmetic on it is checked and raises
    /// `IntegerOverflow` rather than wrapping (`ADR-008`).
    Int(i64),
    /// An IEEE-754 double. Equality on it is exact (`SEMANTIC_CORE.md` §18),
    /// so `NaN` equals `NaN` is a defined result, not an accident of `f64`.
    Real(f64),
    /// A UTF-8 string. Strings are values, not references.
    Str(String),
    /// `true` or `false`.
    Bool(bool),
    /// The single value of the unit type, used where a result carries no
    /// information.
    Unit,
}

impl Literal {
    /// The type every literal of this kind has. Total and variable-free:
    /// no literal is polymorphic, so no inference is needed to type one.
    #[must_use]
    pub fn ty(&self) -> Type {
        match self {
            Literal::Int(_) => Type::Int,
            Literal::Real(_) => Type::Real,
            Literal::Str(_) => Type::Str,
            Literal::Bool(_) => Type::Bool,
            Literal::Unit => Type::Unit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_literal_kind_has_its_own_type() {
        assert_eq!(Literal::Int(1).ty(), Type::Int);
        assert_eq!(Literal::Real(1.5).ty(), Type::Real);
        assert_eq!(Literal::Str(String::new()).ty(), Type::Str);
        assert_eq!(Literal::Bool(false).ty(), Type::Bool);
        assert_eq!(Literal::Unit.ty(), Type::Unit);
    }

    #[test]
    fn int_and_real_literals_never_share_a_type() {
        // §20.2: no implicit coercion, so `1` and `1.0` differ at the type level.
        assert_ne!(Literal::Int(1).ty(), Literal::Real(1.0).ty());
    }

    #[test]
    fn literal_equality_is_structural() {
        assert_eq!(Literal::Str("a".into()), Literal::Str("a".into()));
        assert_ne!(Literal::Int(1), Literal::Real(1.0));
        assert_ne!(Literal::Bool(true), Literal::Unit);
    }
}
