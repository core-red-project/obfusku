//! The operator ladder (`CONCRETE_SYMBOLIC_GRAMMAR.md` §13).
//!
//! From loosest to tightest: `∨`, `⊻`, `∧`, equality, comparison, additive,
//! multiplicative, prefix, then application. Each level is one call to a
//! shared helper, so adding or reordering a level is a one-line change and
//! the associativity rule lives in two places only.

use super::{PResult, Parser};
use crate::ast::{BinOp, Expression, UnOp};
use crate::lexer::TokenKind;

type OperatorOf = fn(&TokenKind) -> Option<BinOp>;

fn or_op(t: &TokenKind) -> Option<BinOp> {
    matches!(t, TokenKind::Or).then_some(BinOp::Or)
}

fn xor_op(t: &TokenKind) -> Option<BinOp> {
    matches!(t, TokenKind::Xor).then_some(BinOp::Xor)
}

fn and_op(t: &TokenKind) -> Option<BinOp> {
    matches!(t, TokenKind::And).then_some(BinOp::And)
}

fn equality_op(t: &TokenKind) -> Option<BinOp> {
    match t {
        TokenKind::EqEq => Some(BinOp::Eq),
        TokenKind::NotEq => Some(BinOp::NotEq),
        _ => None,
    }
}

fn comparison_op(t: &TokenKind) -> Option<BinOp> {
    match t {
        TokenKind::Lt => Some(BinOp::Lt),
        TokenKind::Gt => Some(BinOp::Gt),
        TokenKind::Le => Some(BinOp::Le),
        TokenKind::Ge => Some(BinOp::Ge),
        _ => None,
    }
}

fn additive_op(t: &TokenKind) -> Option<BinOp> {
    match t {
        TokenKind::Plus => Some(BinOp::Add),
        TokenKind::Minus => Some(BinOp::Sub),
        _ => None,
    }
}

fn multiplicative_op(t: &TokenKind) -> Option<BinOp> {
    match t {
        TokenKind::Star => Some(BinOp::Mul),
        TokenKind::Slash => Some(BinOp::Div),
        TokenKind::Percent => Some(BinOp::Mod),
        _ => None,
    }
}

fn binary(op: BinOp, lhs: Expression, rhs: Expression) -> Expression {
    let span = lhs.span().merge(rhs.span());
    Expression::BinaryOp {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span,
    }
}

impl Parser<'_> {
    pub(super) fn parse_or(&mut self) -> PResult<Expression> {
        self.left_associative(Self::parse_xor, or_op)
    }

    fn parse_xor(&mut self) -> PResult<Expression> {
        self.left_associative(Self::parse_and, xor_op)
    }

    fn parse_and(&mut self) -> PResult<Expression> {
        self.left_associative(Self::parse_equality, and_op)
    }

    /// Non-associative: `a ≡ b ≡ c` is a syntax error.
    fn parse_equality(&mut self) -> PResult<Expression> {
        self.non_associative(
            Self::parse_comparison,
            equality_op,
            "equality operators do not chain — 'a == b == c' is not valid Obfusku \
             (CONCRETE_SYMBOLIC_GRAMMAR.md §13: non-associative, not sugar for 'a==b ∧ b==c')",
        )
    }

    /// Non-associative: `a < b < c` is a syntax error.
    fn parse_comparison(&mut self) -> PResult<Expression> {
        self.non_associative(
            Self::parse_additive,
            comparison_op,
            "comparison operators do not chain — 'a < b < c' is not valid Obfusku \
             (CONCRETE_SYMBOLIC_GRAMMAR.md §13: non-associative, not sugar for 'a<b ∧ b<c')",
        )
    }

    fn parse_additive(&mut self) -> PResult<Expression> {
        self.left_associative(Self::parse_multiplicative, additive_op)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expression> {
        self.left_associative(Self::parse_unary, multiplicative_op)
    }

    /// `operand (op operand)*`, grouping to the left: `a ☠︎ b ☠︎ c` is
    /// `(a ☠︎ b) ☠︎ c`. Each operator counts toward the chain limit.
    fn left_associative(
        &mut self,
        operand: impl Fn(&mut Self) -> PResult<Expression>,
        op_of: OperatorOf,
    ) -> PResult<Expression> {
        let mut expr = operand(self)?;
        while let Some(op) = op_of(self.peek()) {
            self.advance();
            self.link()?;
            let rhs = operand(self)?;
            expr = binary(op, expr, rhs);
        }
        Ok(expr)
    }

    /// `operand (op operand)?`: at most one operator of this level, and a
    /// second is `chain_error`. `a < b < c` would otherwise read as
    /// `(a < b) < c`, comparing a `Bool` with a number, which is never what
    /// was meant, and the language refuses to guess.
    fn non_associative(
        &mut self,
        operand: impl Fn(&mut Self) -> PResult<Expression>,
        op_of: OperatorOf,
        chain_error: &str,
    ) -> PResult<Expression> {
        let lhs = operand(self)?;
        let Some(op) = op_of(self.peek()) else {
            return Ok(lhs);
        };
        self.advance();
        let rhs = operand(self)?;
        let node = binary(op, lhs, rhs);
        if op_of(self.peek()).is_some() {
            return Err(self.error(chain_error));
        }
        Ok(node)
    }

    /// Prefix `¬` and `−`, right-to-left: `¬¬x` is `¬(¬x)`, by ordinary
    /// recursion. Binary subtraction is a different glyph (`☠︎`), so there is
    /// nothing to disambiguate.
    ///
    /// `−9223372036854775808` is the one place the lexer's `i64::MIN`
    /// magnitude is legal: it folds into a single literal here, because
    /// `2^63` cannot exist on its own to be negated.
    fn parse_unary(&mut self) -> PResult<Expression> {
        let op = match self.peek() {
            TokenKind::Not => UnOp::Not,
            TokenKind::Neg => UnOp::Neg,
            _ => return self.parse_application(),
        };
        let start = self.advance().span;
        if op == UnOp::Neg && matches!(self.peek(), TokenKind::Int(i64::MIN)) {
            let lit = self.advance().span;
            return Ok(Expression::Int(i64::MIN, start.merge(lit)));
        }
        let operand = self.parse_unary()?;
        let span = start.merge(operand.span());
        Ok(Expression::UnaryOp {
            op,
            operand: Box::new(operand),
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{BinOp, Expression, UnOp};
    use crate::parser::testing::{error, expression};

    fn op_of(e: &Expression) -> BinOp {
        match e {
            Expression::BinaryOp { op, .. } => *op,
            other => panic!("expected a binary operation, got {other:?}"),
        }
    }

    fn parts(e: &Expression) -> (&Expression, &Expression) {
        match e {
            Expression::BinaryOp { lhs, rhs, .. } => (lhs, rhs),
            other => panic!("expected a binary operation, got {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let e = expression("1 ✚ 2 ✱ 3");
        assert_eq!(op_of(&e), BinOp::Add);
        assert_eq!(op_of(parts(&e).1), BinOp::Mul);
    }

    #[test]
    fn additive_operators_group_to_the_left() {
        let e = expression("1 ☠︎ 2 ☠︎ 3");
        assert_eq!(op_of(&e), BinOp::Sub);
        assert_eq!(op_of(parts(&e).0), BinOp::Sub);
        assert!(matches!(parts(&e).1, Expression::Int(3, _)));
    }

    #[test]
    fn the_logical_levels_nest_or_over_xor_over_and() {
        let e = expression("a ∨ b ⊻ c ∧ d");
        assert_eq!(op_of(&e), BinOp::Or);
        let rhs = parts(&e).1;
        assert_eq!(op_of(rhs), BinOp::Xor);
        assert_eq!(op_of(parts(rhs).1), BinOp::And);
    }

    #[test]
    fn comparison_binds_tighter_than_equality_and_arithmetic_tighter_than_both() {
        let e = expression("a ✚ 1 < b ≡ c");
        assert_eq!(op_of(&e), BinOp::Eq);
        assert_eq!(op_of(parts(&e).0), BinOp::Lt);
        assert_eq!(op_of(parts(parts(&e).0).0), BinOp::Add);
    }

    #[test]
    fn comparison_and_equality_do_not_chain() {
        assert!(error("v ≔ a < b < c\n❧").contains("comparison operators do not chain"));
        assert!(error("v ≔ a ≡ b ≠ c\n❧").contains("equality operators do not chain"));
    }

    #[test]
    fn parentheses_allow_what_chaining_forbids() {
        let e = expression("(a < b) ≡ (c < d)");
        assert_eq!(op_of(&e), BinOp::Eq);
    }

    #[test]
    fn prefix_operators_nest_to_the_right() {
        let Expression::UnaryOp { op, operand, .. } = expression("¬¬x") else {
            panic!("expected a unary operation");
        };
        assert_eq!(op, UnOp::Not);
        assert!(matches!(
            *operand,
            Expression::UnaryOp { op: UnOp::Not, .. }
        ));
    }

    #[test]
    fn negation_binds_tighter_than_multiplication() {
        let e = expression("−a ✱ b");
        assert_eq!(op_of(&e), BinOp::Mul);
        assert!(matches!(
            parts(&e).0,
            Expression::UnaryOp { op: UnOp::Neg, .. }
        ));
    }

    #[test]
    fn the_minimum_integer_folds_into_one_literal_behind_a_minus() {
        assert!(matches!(
            expression("−9223372036854775808"),
            Expression::Int(i64::MIN, _)
        ));
    }

    #[test]
    fn the_magnitude_alone_is_out_of_range() {
        assert!(error("v ≔ 9223372036854775808\n❧").contains("out of range as a literal"));
    }

    #[test]
    fn every_arithmetic_and_relational_glyph_has_an_operator() {
        for (src, expected) in [
            ("a ÷ b", BinOp::Div),
            ("a ⌗ b", BinOp::Mod),
            ("a ≤ b", BinOp::Le),
            ("a ≥ b", BinOp::Ge),
            ("a > b", BinOp::Gt),
            ("a ≠ b", BinOp::NotEq),
        ] {
            assert_eq!(op_of(&expression(src)), expected, "{src}");
        }
    }
}
