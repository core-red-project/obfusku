//! The operator table (`SEMANTIC_CORE.md` §20.2).

use super::raise::{EvalResult, Raised, division_by_zero, err_value, integer_overflow};
use crate::ops::cmp_operator;
use crate::persistent::Text;
use crate::value::Value;
use obfusku_core::ast::{BinOp, UnOp};
use obfusku_diagnostics::Span;
use std::cmp::Ordering;

/// `Some(n)` as an `Int`, or `IntegerOverflow` when the operation had no
/// representable result.
///
/// `Int` is a fixed-width `i64`, and an unrepresentable result raises a
/// deterministic, catchable exception on every build profile instead of
/// wrapping (release) or panicking (debug). `Real` has no equivalent: IEEE-754
/// arithmetic is total (§15.3).
fn checked(result: Option<i64>, span: Span) -> EvalResult {
    result
        .map(Value::Int)
        .ok_or_else(|| Raised(integer_overflow(span)))
}

/// Integer division or remainder.
///
/// Zero is `DivisionByZero`. The one other unrepresentable case,
/// `i64::MIN ÷ -1` (and the same for the remainder), is `IntegerOverflow`:
/// genuinely a different condition, not a second zero check (§15.3).
fn divide(a: i64, b: i64, op: fn(i64, i64) -> Option<i64>, span: Span) -> EvalResult {
    if b == 0 {
        Err(Raised(division_by_zero(span)))
    } else {
        checked(op(a, b), span)
    }
}

/// Whether `ordering` satisfies the relational operator `op`. An unordered
/// pair (`NaN`) satisfies none, so all four are `false`.
fn ordering_holds(op: BinOp, ordering: Option<Ordering>) -> bool {
    matches!(
        (op, ordering),
        (BinOp::Lt, Some(Ordering::Less))
            | (BinOp::Gt, Some(Ordering::Greater))
            | (BinOp::Le, Some(Ordering::Less | Ordering::Equal))
            | (BinOp::Ge, Some(Ordering::Greater | Ordering::Equal))
    )
}

/// Applies a binary operator to two evaluated operands.
///
/// A closed table: no coercion between `Int` and `Real`, no overloading. A
/// pair it does not recognise is a program the checker should have
/// rejected, reported rather than panicked on.
pub(super) fn eval_binop(op: BinOp, lv: Value, rv: Value, span: Span) -> EvalResult {
    use BinOp::{Add, Div, Eq, Ge, Gt, Le, Lt, Mod, Mul, NotEq, Sub, Xor};
    match (op, lv, rv) {
        (Add, Value::Int(a), Value::Int(b)) => checked(a.checked_add(b), span),
        (Add, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a + b)),
        (Add, Value::Str(ref a), Value::Str(ref b)) => Ok(Value::Str(Text::concat(a, b))),

        (Sub, Value::Int(a), Value::Int(b)) => checked(a.checked_sub(b), span),
        (Sub, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a - b)),

        (Mul, Value::Int(a), Value::Int(b)) => checked(a.checked_mul(b), span),
        (Mul, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a * b)),

        (Div, Value::Int(a), Value::Int(b)) => divide(a, b, i64::checked_div, span),
        (Div, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a / b)),

        // Truncating, sign of the dividend (Rust's own `%`).
        (Mod, Value::Int(a), Value::Int(b)) => divide(a, b, i64::checked_rem, span),

        // `Int` is the common case; keep it off the generic path.
        (Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        // Generated ordering over any orderable type (`ADR-025`).
        (Lt | Gt | Le | Ge, a, b) => Ok(Value::Bool(ordering_holds(op, cmp_operator(&a, &b)))),

        // Total structural equality, including NaN == NaN (§18).
        (Eq, a, b) => Ok(Value::Bool(a.structural_eq(&b))),
        (NotEq, a, b) => Ok(Value::Bool(!a.structural_eq(&b))),

        (Xor, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a != b)),

        (op, a, b) => Err(Raised(err_value(
            span,
            format!(
                "invalid operand types for operator (typecheck should have rejected this): \
                 {op:?} on {a:?}, {b:?}"
            ),
        ))),
    }
}

/// Applies a unary operator. `-i64::MIN` is the one unrepresentable negation.
pub(super) fn eval_unop(op: UnOp, v: Value, span: Span) -> EvalResult {
    match (op, v) {
        (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        (UnOp::Neg, Value::Int(n)) => checked(n.checked_neg(), span),
        (UnOp::Neg, Value::Real(n)) => Ok(Value::Real(-n)),
        (op, v) => Err(Raised(err_value(
            span,
            format!(
                "invalid operand type for operator (typecheck should have rejected this): {op:?} on {v:?}"
            ),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(op: BinOp, a: Value, b: Value) -> EvalResult {
        eval_binop(op, a, b, Span::default())
    }

    fn int(i: i64) -> Value {
        Value::Int(i)
    }

    fn int_of(r: EvalResult) -> i64 {
        match r {
            Ok(Value::Int(i)) => i,
            other => panic!("expected Int, got {other:?}"),
        }
    }

    fn bool_of(r: EvalResult) -> bool {
        match r {
            Ok(Value::Bool(b)) => b,
            other => panic!("expected Bool, got {other:?}"),
        }
    }

    fn raised_tag(r: &EvalResult) -> String {
        match r {
            Err(Raised(Value::Adt { tag, .. })) => tag.to_string(),
            other => panic!("expected a raise, got {other:?}"),
        }
    }

    #[test]
    fn integer_arithmetic() {
        assert_eq!(int_of(bin(BinOp::Add, int(2), int(3))), 5);
        assert_eq!(int_of(bin(BinOp::Sub, int(2), int(3))), -1);
        assert_eq!(int_of(bin(BinOp::Mul, int(4), int(3))), 12);
        assert_eq!(int_of(bin(BinOp::Div, int(7), int(2))), 3);
    }

    #[test]
    fn remainder_truncates_with_the_sign_of_the_dividend() {
        assert_eq!(int_of(bin(BinOp::Mod, int(-7), int(2))), -1);
        assert_eq!(int_of(bin(BinOp::Mod, int(7), int(-2))), 1);
    }

    #[test]
    fn overflow_raises_integer_overflow_in_every_operator() {
        for (op, a, b) in [
            (BinOp::Add, i64::MAX, 1),
            (BinOp::Sub, i64::MIN, 1),
            (BinOp::Mul, i64::MAX, 2),
            (BinOp::Div, i64::MIN, -1),
            (BinOp::Mod, i64::MIN, -1),
        ] {
            assert_eq!(
                raised_tag(&bin(op, int(a), int(b))),
                "IntegerOverflow",
                "{op:?}"
            );
        }
        assert_eq!(
            raised_tag(&eval_unop(UnOp::Neg, int(i64::MIN), Span::default())),
            "IntegerOverflow"
        );
    }

    #[test]
    fn division_and_remainder_by_zero_raise_division_by_zero() {
        assert_eq!(
            raised_tag(&bin(BinOp::Div, int(1), int(0))),
            "DivisionByZero"
        );
        assert_eq!(
            raised_tag(&bin(BinOp::Mod, int(1), int(0))),
            "DivisionByZero"
        );
    }

    #[test]
    fn real_arithmetic_is_ieee() {
        let r = bin(BinOp::Div, Value::Real(1.0), Value::Real(0.0));
        assert!(matches!(r, Ok(Value::Real(x)) if x.is_infinite()));
    }

    #[test]
    fn string_addition_concatenates() {
        let r = bin(
            BinOp::Add,
            Value::Str(Text::from("a")),
            Value::Str(Text::from("b")),
        );
        assert!(matches!(&r, Ok(Value::Str(s)) if s == "ab"));
    }

    #[test]
    fn relational_operators_on_integers() {
        assert!(bool_of(bin(BinOp::Lt, int(1), int(2))));
        assert!(!bool_of(bin(BinOp::Gt, int(1), int(2))));
        assert!(bool_of(bin(BinOp::Le, int(2), int(2))));
        assert!(bool_of(bin(BinOp::Ge, int(2), int(2))));
    }

    #[test]
    fn relational_operators_on_other_types_use_the_generated_order() {
        let (a, b) = (Value::Str(Text::from("a")), Value::Str(Text::from("b")));
        assert!(bool_of(bin(BinOp::Lt, a.clone(), b.clone())));
        assert!(bool_of(bin(BinOp::Le, a.clone(), a)));
    }

    #[test]
    fn nan_makes_every_relational_operator_false_but_equals_itself() {
        let nan = || Value::Real(f64::NAN);
        for op in [BinOp::Lt, BinOp::Gt, BinOp::Le, BinOp::Ge] {
            assert!(!bool_of(bin(op, nan(), Value::Real(1.0))), "{op:?}");
        }
        assert!(bool_of(bin(BinOp::Eq, nan(), nan())));
        assert!(!bool_of(bin(BinOp::NotEq, nan(), nan())));
    }

    #[test]
    fn xor_is_eager_and_boolean() {
        assert!(bool_of(bin(
            BinOp::Xor,
            Value::Bool(true),
            Value::Bool(false)
        )));
        assert!(!bool_of(bin(
            BinOp::Xor,
            Value::Bool(true),
            Value::Bool(true)
        )));
    }

    #[test]
    fn unary_not_and_negation() {
        assert!(!bool_of(eval_unop(
            UnOp::Not,
            Value::Bool(true),
            Span::default()
        )));
        assert_eq!(int_of(eval_unop(UnOp::Neg, int(3), Span::default())), -3);
        assert!(matches!(
            eval_unop(UnOp::Neg, Value::Real(1.5), Span::default()),
            Ok(Value::Real(x)) if x == -1.5
        ));
    }

    #[test]
    fn an_operand_pair_the_checker_would_reject_is_reported_not_panicked_on() {
        assert_eq!(
            raised_tag(&bin(BinOp::Add, int(1), Value::Unit)),
            "InvalidOperation"
        );
        assert_eq!(
            raised_tag(&eval_unop(UnOp::Not, int(1), Span::default())),
            "InvalidOperation"
        );
    }
}
