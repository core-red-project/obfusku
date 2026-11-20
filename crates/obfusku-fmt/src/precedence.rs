//! When an operand needs parentheses.

use obfusku_syntax::ast::{BinOp, Expression};

/// How tightly an expression binds; a larger value binds tighter.
///
/// A child prints bare only when its precedence is at least the minimum its
/// position demands (`CONCRETE_SYMBOLIC_GRAMMAR.md` §13). The ladder is a
/// newtype over `u8` rather than an enum because the printer needs "one level
/// tighter than this operator" ([`Prec::tighter`]) as plain arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Prec(u8);

impl Prec {
    /// Positions that own the rest of the expression (a declaration body, a
    /// bracketed element, an argument): nothing follows that could be
    /// misparsed, so nothing needs protecting.
    pub(crate) const BOTTOM: Prec = Prec(0);
    const OR: Prec = Prec(1);
    const XOR: Prec = Prec(2);
    const AND: Prec = Prec(3);
    const EQUALITY: Prec = Prec(4);
    const COMPARISON: Prec = Prec(5);
    const ADDITIVE: Prec = Prec(6);
    const MULTIPLICATIVE: Prec = Prec(7);
    /// Prefix `¬` and `−`.
    pub(crate) const UNARY: Prec = Prec(8);
    /// Self-delimiting forms (literals, calls, indexing…): safe anywhere.
    pub(crate) const ATOM: Prec = Prec(9);

    /// The next level up the ladder.
    const fn tighter(self) -> Prec {
        Prec(self.0 + 1)
    }

    /// The precedence of a binary operator.
    pub(crate) const fn of_binary(op: BinOp) -> Prec {
        match op {
            BinOp::Or => Prec::OR,
            BinOp::Xor => Prec::XOR,
            BinOp::And => Prec::AND,
            BinOp::Eq | BinOp::NotEq => Prec::EQUALITY,
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => Prec::COMPARISON,
            BinOp::Add | BinOp::Sub => Prec::ADDITIVE,
            BinOp::Mul | BinOp::Div | BinOp::Mod => Prec::MULTIPLICATIVE,
        }
    }

    /// The precedence an expression has as an operand.
    ///
    /// `Lambda`, `Rebind`, `Pipe` and the local forms get [`Prec::BOTTOM`]:
    /// each parses its trailing piece with the full expression grammar and no
    /// closing delimiter, so printing one bare as an operand would let it
    /// swallow whatever should follow.
    pub(crate) const fn of_expression(e: &Expression) -> Prec {
        match e {
            Expression::BinaryOp { op, .. } => Prec::of_binary(*op),
            Expression::UnaryOp { .. } => Prec::UNARY,
            Expression::Lambda { .. }
            | Expression::Rebind { .. }
            | Expression::Pipe { .. }
            | Expression::LocalValue { .. }
            | Expression::LocalFunctions { .. } => Prec::BOTTOM,
            _ => Prec::ATOM,
        }
    }
}

/// The minimum precedence of the two operands of a binary operator, as
/// `(left, right)`.
///
/// Left-associative operators let the *left* operand share their level (so
/// `a ✚ b ✚ c` groups naturally) but not the right, or `a ☠︎ b ☠︎ c` would
/// re-parse as `a ☠︎ (b ☠︎ c)`. Comparison and equality are non-associative:
/// the parser rejects chaining, so both sides must be strictly tighter.
pub(crate) const fn operand_minimums(op: BinOp) -> (Prec, Prec) {
    let own = Prec::of_binary(op);
    let right = own.tighter();
    match op {
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => (right, right),
        _ => (own, right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_orders_logic_below_comparison_below_arithmetic() {
        let ladder = [
            BinOp::Or,
            BinOp::Xor,
            BinOp::And,
            BinOp::Eq,
            BinOp::Lt,
            BinOp::Add,
            BinOp::Mul,
        ]
        .map(Prec::of_binary);
        assert!(ladder.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(Prec::of_binary(BinOp::Mul) < Prec::UNARY);
        assert!(Prec::UNARY < Prec::ATOM);
    }

    #[test]
    fn operators_of_one_tier_share_a_precedence() {
        assert_eq!(Prec::of_binary(BinOp::Add), Prec::of_binary(BinOp::Sub));
        assert_eq!(Prec::of_binary(BinOp::Mul), Prec::of_binary(BinOp::Mod));
        assert_eq!(Prec::of_binary(BinOp::Le), Prec::of_binary(BinOp::Gt));
    }

    #[test]
    fn left_associative_operators_accept_their_own_level_on_the_left_only() {
        let (left, right) = operand_minimums(BinOp::Sub);
        assert_eq!(left, Prec::of_binary(BinOp::Sub));
        assert!(right > left);
    }

    #[test]
    fn non_associative_operators_require_tighter_on_both_sides() {
        for op in [
            BinOp::Eq,
            BinOp::NotEq,
            BinOp::Lt,
            BinOp::Gt,
            BinOp::Le,
            BinOp::Ge,
        ] {
            let (left, right) = operand_minimums(op);
            assert!(left > Prec::of_binary(op), "{op:?}");
            assert_eq!(left, right);
        }
    }
}
