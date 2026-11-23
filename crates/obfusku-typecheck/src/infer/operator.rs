//! The closed operator table (`SEMANTIC_CORE.md` §20.2).
//!
//! No typeclasses and no implicit coercion: each operator accepts a fixed
//! set of operand types and `Int ✚ Real` is an error, not a widening.
//! Division and remainder by zero are runtime conditions (§15.3) and are
//! deliberately not checked here.

use super::{Checker, Infer, TypeEnv, reject};
use crate::types::{Constraint, Type};
use obfusku_core::ast::{BinOp, Expr, OpClass, UnOp};
use obfusku_diagnostics::Span;

/// The surface glyph of an operator, for diagnostics only.
fn glyph(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "✚",
        BinOp::Sub => "☠︎",
        BinOp::Mul => "✱",
        BinOp::Div => "÷",
        BinOp::Mod => "⌗",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "≤",
        BinOp::Ge => "≥",
        BinOp::Eq => "≡",
        BinOp::NotEq => "≠",
        BinOp::Xor => "⊻",
    }
}

/// An operator's accepted operand types, with their wording for diagnostics.
struct Domain {
    types: &'static [fn() -> Type],
    wording: &'static str,
}

const NUMERIC: Domain = Domain {
    types: &[|| Type::Int, || Type::Real],
    wording: "Int or Real",
};

const ADDABLE: Domain = Domain {
    types: &[|| Type::Int, || Type::Real, || Type::Str],
    wording: "Int, Real, or Str",
};

impl Checker {
    pub(super) fn infer_binop(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let lhs_ty = self.infer(lhs, env)?;
        match op.class() {
            OpClass::Equality => self.relation(lhs_ty, rhs, Constraint::Eq, span, env),
            OpClass::Ordering => self.relation(lhs_ty, rhs, Constraint::Ord, span, env),
            OpClass::Logical => self.both_operands(&Type::Bool, &lhs_ty, rhs, span, env),
            OpClass::Arithmetic => match op {
                BinOp::Mod => self.both_operands(&Type::Int, &lhs_ty, rhs, span, env),
                BinOp::Add => self.arithmetic(op, &ADDABLE, &lhs_ty, rhs, span, env),
                _ => self.arithmetic(op, &NUMERIC, &lhs_ty, rhs, span, env),
            },
        }
    }

    pub(super) fn infer_unop(
        &mut self,
        op: UnOp,
        operand: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let operand_ty = self.infer(operand, env)?;
        match op {
            UnOp::Not => {
                self.expect_same(&Type::Bool, &operand_ty, span)?;
                Ok(Type::Bool)
            }
            UnOp::Neg => self.operand_in(&operand_ty, "−", &NUMERIC, span),
        }
    }

    /// `==`/`<`-style operators: both sides one type, which must support
    /// `constraint`; the result is `Bool`.
    fn relation(
        &mut self,
        lhs_ty: Type,
        rhs: &Expr,
        constraint: Constraint,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let rhs_ty = self.infer(rhs, env)?;
        self.expect_same(&lhs_ty, &rhs_ty, span)?;
        self.require(lhs_ty, constraint, span);
        Ok(Type::Bool)
    }

    /// Both operands exactly `expected`; the result is `expected`'s type for
    /// `Int` remainder and `Bool` for `⊻`, which are the same type.
    fn both_operands(
        &mut self,
        expected: &Type,
        lhs_ty: &Type,
        rhs: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        self.expect_same(expected, lhs_ty, span)?;
        let rhs_ty = self.infer(rhs, env)?;
        self.expect_same(expected, &rhs_ty, span)?;
        Ok(expected.clone())
    }

    /// The left operand picks the type from `domain`; the right must match.
    fn arithmetic(
        &mut self,
        op: BinOp,
        domain: &Domain,
        lhs_ty: &Type,
        rhs: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let candidate = self.operand_in(lhs_ty, glyph(op), domain, span)?;
        let rhs_ty = self.infer(rhs, env)?;
        self.expect_same(&candidate, &rhs_ty, span)?;
        Ok(candidate)
    }

    /// `ty` resolved, if it is in `domain`; the dispatch needs a known type
    /// to choose, so a still-unresolved operand is rejected too.
    fn operand_in(&self, ty: &Type, glyph: &str, domain: &Domain, span: Span) -> Infer<Type> {
        let resolved = self.resolve(ty);
        if domain.types.iter().any(|accepts| accepts() == resolved) {
            return Ok(resolved);
        }
        reject(
            span,
            format!(
                "'{glyph}' is only defined for {} (SEMANTIC_CORE.md §20.2's closed operator \
                 table) — found {}",
                domain.wording,
                resolved.describe()
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{boolean, checker, int, lambda, span, string, var};
    use super::*;
    use obfusku_core::ast::Literal;

    fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::BinaryOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span: span(),
        }
    }

    fn real(value: f64) -> Expr {
        Expr::Lit(Literal::Real(value), span())
    }

    fn infer(expr: &Expr) -> Result<Type, String> {
        let mut c = checker();
        c.infer(expr, &TypeEnv::new())
            .map(|ty| c.resolve(&ty))
            .map_err(|ds| ds[0].message.clone())
    }

    #[test]
    fn addition_accepts_int_real_and_str_of_one_kind() {
        assert_eq!(infer(&binary(BinOp::Add, int(1), int(2))), Ok(Type::Int));
        assert_eq!(
            infer(&binary(BinOp::Add, real(1.0), real(2.0))),
            Ok(Type::Real)
        );
        assert_eq!(
            infer(&binary(BinOp::Add, string("a"), string("b"))),
            Ok(Type::Str)
        );
    }

    #[test]
    fn mixing_int_and_real_is_not_coerced() {
        assert!(infer(&binary(BinOp::Add, int(1), real(2.0))).is_err());
    }

    #[test]
    fn addition_names_the_domain_when_the_operand_is_outside_it() {
        let error = infer(&binary(BinOp::Add, boolean(true), boolean(false))).expect_err("bool");
        assert!(error.contains("'✚' is only defined for Int, Real, or Str"));
        assert!(error.ends_with("found Bool"));
    }

    #[test]
    fn subtraction_rejects_strings() {
        let error = infer(&binary(BinOp::Sub, string("a"), string("b"))).expect_err("str");
        assert!(error.contains("'☠︎' is only defined for Int or Real"));
    }

    #[test]
    fn remainder_is_int_only() {
        assert_eq!(infer(&binary(BinOp::Mod, int(5), int(2))), Ok(Type::Int));
        assert!(infer(&binary(BinOp::Mod, real(5.0), real(2.0))).is_err());
    }

    #[test]
    fn xor_is_bool_only() {
        assert_eq!(
            infer(&binary(BinOp::Xor, boolean(true), boolean(false))),
            Ok(Type::Bool)
        );
        assert!(infer(&binary(BinOp::Xor, int(1), int(0))).is_err());
    }

    #[test]
    fn comparisons_yield_bool_and_need_matching_operands() {
        assert_eq!(infer(&binary(BinOp::Lt, int(1), int(2))), Ok(Type::Bool));
        assert!(infer(&binary(BinOp::Lt, int(1), real(2.0))).is_err());
    }

    #[test]
    fn equality_of_functions_is_a_static_error() {
        let f = || lambda("x", var("x"));
        let mut c = checker();
        let ty = c
            .infer(&binary(BinOp::Eq, f(), f()), &TypeEnv::new())
            .expect("operands unify");
        assert_eq!(ty, Type::Bool);
        let errors = c.finish();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("has no equality"));
    }

    #[test]
    fn unary_not_wants_a_bool() {
        let not = |operand| Expr::UnaryOp {
            op: UnOp::Not,
            operand: Box::new(operand),
            span: span(),
        };
        assert_eq!(infer(&not(boolean(true))), Ok(Type::Bool));
        assert!(infer(&not(int(1))).is_err());
    }

    #[test]
    fn negation_keeps_the_operand_type() {
        let neg = |operand| Expr::UnaryOp {
            op: UnOp::Neg,
            operand: Box::new(operand),
            span: span(),
        };
        assert_eq!(infer(&neg(int(1))), Ok(Type::Int));
        assert_eq!(infer(&neg(real(1.0))), Ok(Type::Real));
        assert!(infer(&neg(string("s"))).is_err());
    }
}
