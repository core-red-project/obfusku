//! Primitive operators (`SEMANTIC_CORE.md` §16, §20.2).

/// A binary operator that survives into the Core.
///
/// `∧` and `∨` are **absent**: they desugar to `Match` at the
/// `obfusku-syntax` boundary, so short-circuiting is a consequence of
/// `Match`'s evaluation rule instead of a second conditional-evaluation path
/// the evaluator and checker would both have to carry (§16, §20.2). Every
/// operator here evaluates `lhs` then `rhs`, strictly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// Addition; also string concatenation (§20.2).
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division. Integer division by zero raises `DivisionByZero` (§15.3).
    Div,
    /// Remainder, defined on `Int` only (§20.2).
    Mod,
    /// Strictly less than.
    Lt,
    /// Strictly greater than.
    Gt,
    /// Less than or equal.
    Le,
    /// Greater than or equal.
    Ge,
    /// Structural equality (§18).
    Eq,
    /// Structural inequality (§18).
    NotEq,
    /// Exclusive or on `Bool`. Not short-circuiting, which is why it can be
    /// a primitive while `∧`/`∨` cannot.
    Xor,
}

/// Which typing rule family an operator belongs to (§20.2).
///
/// Consumers dispatch on the family, not the individual operator, so adding
/// an operator to an existing family needs no change in the checker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpClass {
    /// Add, subtract, multiply, divide, remainder: operands of one numeric
    /// type, result of that type.
    Arithmetic,
    /// The four relational operators: operands of one orderable type,
    /// result `Bool`.
    Ordering,
    /// Equality and inequality: operands of one comparable type, result `Bool`.
    Equality,
    /// Exclusive or: `Bool` operands, `Bool` result.
    Logical,
}

impl BinOp {
    /// The typing family of this operator.
    #[must_use]
    pub fn class(self) -> OpClass {
        match self {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => OpClass::Arithmetic,
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => OpClass::Ordering,
            BinOp::Eq | BinOp::NotEq => OpClass::Equality,
            BinOp::Xor => OpClass::Logical,
        }
    }

    /// Whether the result is always `Bool`, whatever the operand type.
    #[must_use]
    pub fn yields_bool(self) -> bool {
        !matches!(self.class(), OpClass::Arithmetic)
    }
}

/// A unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// Logical negation of a `Bool`.
    Not,
    /// Arithmetic negation of an `Int` or `Real`. Negating `Int::MIN`
    /// overflows and raises like any other checked arithmetic (`ADR-008`).
    Neg,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [BinOp; 12] = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Div,
        BinOp::Mod,
        BinOp::Lt,
        BinOp::Gt,
        BinOp::Le,
        BinOp::Ge,
        BinOp::Eq,
        BinOp::NotEq,
        BinOp::Xor,
    ];

    #[test]
    fn every_operator_has_exactly_one_class() {
        let count = |c: OpClass| ALL.iter().filter(|op| op.class() == c).count();
        assert_eq!(count(OpClass::Arithmetic), 5);
        assert_eq!(count(OpClass::Ordering), 4);
        assert_eq!(count(OpClass::Equality), 2);
        assert_eq!(count(OpClass::Logical), 1);
    }

    #[test]
    fn only_arithmetic_keeps_the_operand_type() {
        for op in ALL {
            assert_eq!(
                op.yields_bool(),
                op.class() != OpClass::Arithmetic,
                "{op:?}"
            );
        }
    }

    #[test]
    fn comparisons_and_xor_yield_bool() {
        assert!(BinOp::Lt.yields_bool() && BinOp::Eq.yields_bool() && BinOp::Xor.yields_bool());
        assert!(!BinOp::Add.yields_bool() && !BinOp::Mod.yields_bool());
    }
}
