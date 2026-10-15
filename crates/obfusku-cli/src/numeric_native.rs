//! Explicit `Int ↔ Real` conversion (`SEMANTIC_CORE.md` §20.3,
//! `GLYPH_SYSTEM_DESIGN.md` §10.6) — host-provided natives for the same
//! reason `Array`'s combinators are (`ADR-021`): there is no Core
//! primitive that crosses between `Int` and `Real`, and none should be
//! added (§20.2's "no implicit coercion" stays true at the operator
//! level; these are the two explicit escape hatches).
//!
//! - `↗ : Int → Real` — total, never raises. May lose precision for
//!   `|n| > 2^53` (`f64`'s exact-integer range) — intentional, not an
//!   exceptional condition.
//! - `↘ : Real → Int` — partial. The three-way disjoint contract below
//!   is tested directly against §20.3's table rather than delegated to
//!   whatever Rust's own `as` cast does at its edges (which saturates,
//!   not what this language's contract specifies).

use crate::native_util::{Registry, func, int_arg, invalid};
use obfusku_runtime::Value;
use obfusku_runtime::eval::Raised;
use obfusku_runtime::ops::cmp_total;
use obfusku_typecheck::{Constraint, Prelude, Type, TypeVarId};
use std::collections::HashMap;
use std::rc::Rc;

/// The exact `i64` range as `f64` bounds — `i64::MAX as f64` itself
/// rounds *up* past the true maximum (`f64` cannot represent
/// `i64::MAX` exactly), so comparing against it directly would wrongly
/// accept some values that don't actually fit. `2.0^63` is exactly
/// representable in `f64` and is the correct exclusive upper bound;
/// `-2.0^63` (`i64::MIN`) is exactly representable and is the correct
/// inclusive lower bound.
const I64_MIN_AS_F64: f64 = -9_223_372_036_854_775_808.0;
const I64_MAX_EXCLUSIVE_AS_F64: f64 = 9_223_372_036_854_775_808.0;

fn integer_overflow() -> Raised {
    Raised(Value::Adt {
        tag: Rc::from("IntegerOverflow"),
        args: Rc::from(vec![]),
    })
}

/// §20.3's table, applied directly — no reliance on `f64 as i64`'s own
/// (saturating, not raising) edge-case behavior.
fn real_to_int(r: f64) -> Result<i64, Raised> {
    if r.is_nan() || r.is_infinite() {
        return Err(invalid(format!(
            "'\u{2198}' expects a finite Real, found {r}"
        )));
    }
    let truncated = r.trunc();
    if !(I64_MIN_AS_F64..I64_MAX_EXCLUSIVE_AS_F64).contains(&truncated) {
        return Err(integer_overflow());
    }
    Ok(truncated as i64)
}

type IntOp = fn(i64, i64) -> i64;
type RealFn = fn(f64) -> f64;
type RealOp = fn(f64, f64) -> f64;

fn int_pow(base: i64, exp: i64) -> Result<i64, Raised> {
    if exp < 0 {
        return Err(invalid(format!(
            "'⋆' expects a non-negative exponent, found {exp}"
        )));
    }
    let (mut acc, mut b, mut e) = (1i64, base, exp);
    while e > 0 {
        if e & 1 == 1 {
            acc = acc.checked_mul(b).ok_or_else(integer_overflow)?;
        }
        e >>= 1;
        if e > 0 {
            b = b.checked_mul(b).ok_or_else(integer_overflow)?;
        }
    }
    Ok(acc)
}

fn shift_count(op: &str, n: i64) -> Result<u32, Raised> {
    match u32::try_from(n) {
        Ok(c) if c < 64 => Ok(c),
        _ => Err(invalid(format!(
            "'{op}' expects a shift count in 0..=63, found {n}"
        ))),
    }
}

fn real_arg(op: &str, v: &Value) -> Result<f64, Raised> {
    match v {
        Value::Real(r) => Ok(*r),
        other => Err(invalid(format!("'{op}' expects a Real, found {other:?}"))),
    }
}

#[must_use]
pub fn load() -> (Prelude, HashMap<String, Value>) {
    let mut r = Registry::default();
    let (int, real) = (Type::Int, Type::Real);
    let binary = |t: &Type| func(t.clone(), func(t.clone(), t.clone()));
    let unary = |t: &Type| func(t.clone(), t.clone());

    r.add(
        "\u{2197}",
        &[],
        &[],
        func(Type::Int, Type::Real),
        1,
        Rc::new(|a| Ok(Value::Real(int_arg("\u{2197}", &a[0])? as f64))),
    );
    r.add(
        "\u{2198}",
        &[],
        &[],
        func(Type::Real, Type::Int),
        1,
        Rc::new(|a| real_to_int(real_arg("\u{2198}", &a[0])?).map(Value::Int)),
    );

    // Int (`ADR-027`).
    r.add(
        "∥",
        &[],
        &[],
        unary(&int),
        1,
        Rc::new(|a| {
            int_arg("∥", &a[0])?
                .checked_abs()
                .map(Value::Int)
                .ok_or_else(integer_overflow)
        }),
    );
    r.add(
        "⋆",
        &[],
        &[],
        binary(&int),
        2,
        Rc::new(|a| int_pow(int_arg("⋆", &a[0])?, int_arg("⋆", &a[1])?).map(Value::Int)),
    );
    let bitwise: [(&'static str, IntOp); 3] = [
        ("⊓", |x, y| x & y),
        ("⊔", |x, y| x | y),
        ("⊗", |x, y| x ^ y),
    ];
    for (name, op) in bitwise {
        r.add(
            name,
            &[],
            &[],
            binary(&int),
            2,
            Rc::new(move |a| Ok(Value::Int(op(int_arg(name, &a[0])?, int_arg(name, &a[1])?)))),
        );
    }
    r.add(
        "∼",
        &[],
        &[],
        unary(&int),
        1,
        Rc::new(|a| Ok(Value::Int(!int_arg("∼", &a[0])?))),
    );
    r.add(
        "⋘",
        &[],
        &[],
        binary(&int),
        2,
        Rc::new(|a| {
            let n = shift_count("⋘", int_arg("⋘", &a[1])?)?;
            Ok(Value::Int(((int_arg("⋘", &a[0])? as u64) << n) as i64))
        }),
    );
    r.add(
        "⋙",
        &[],
        &[],
        binary(&int),
        2,
        Rc::new(|a| {
            let n = shift_count("⋙", int_arg("⋙", &a[1])?)?;
            Ok(Value::Int(int_arg("⋙", &a[0])? >> n))
        }),
    );

    // Minimum and maximum over any ordered type, by the generated order.
    let a = Type::Var(TypeVarId(0));
    let ord = [(0, Constraint::Ord)];
    r.add(
        "⊥",
        &[0],
        &ord,
        binary(&a),
        2,
        Rc::new(|v| {
            Ok(if cmp_total(&v[1], &v[0]).is_lt() {
                v[1].clone()
            } else {
                v[0].clone()
            })
        }),
    );
    r.add(
        "⊤",
        &[0],
        &ord,
        binary(&a),
        2,
        Rc::new(|v| {
            Ok(if cmp_total(&v[1], &v[0]).is_gt() {
                v[1].clone()
            } else {
                v[0].clone()
            })
        }),
    );

    // Real: IEEE-754 — an argument outside a function's domain gives
    // `NaN`; nothing here raises.
    let unary_real: [(&'static str, RealFn); 9] = [
        ("√", f64::sqrt),
        ("⌊", f64::floor),
        ("⌃", f64::ceil),
        ("⌾", f64::round),
        ("ℯ", f64::exp),
        ("ℓ", f64::ln),
        ("∿", f64::sin),
        ("∾", f64::cos),
        ("∦", f64::abs),
    ];
    for (name, f) in unary_real {
        r.add(
            name,
            &[],
            &[],
            unary(&real),
            1,
            Rc::new(move |a| Ok(Value::Real(f(real_arg(name, &a[0])?)))),
        );
    }
    let binary_real: [(&'static str, RealOp); 2] = [("⋇", f64::powf), ("∠", f64::atan2)];
    for (name, f) in binary_real {
        r.add(
            name,
            &[],
            &[],
            binary(&real),
            2,
            Rc::new(move |a| {
                Ok(Value::Real(f(
                    real_arg(name, &a[0])?,
                    real_arg(name, &a[1])?,
                )))
            }),
        );
    }
    r.add(
        "⍥",
        &[],
        &[],
        func(real.clone(), Type::Bool),
        1,
        Rc::new(|a| Ok(Value::Bool(real_arg("⍥", &a[0])?.is_finite()))),
    );
    r.add(
        "ϖ",
        &[],
        &[],
        real.clone(),
        0,
        Rc::new(|_| Ok(Value::Real(std::f64::consts::PI))),
    );
    r.add(
        "∞",
        &[],
        &[],
        real,
        0,
        Rc::new(|_| Ok(Value::Real(f64::INFINITY))),
    );

    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, values: &HashMap<String, Value>, arg: Value) -> Result<Value, Raised> {
        match values.get(name).unwrap() {
            Value::Native(f) => (f.func)(arg),
            _ => panic!("expected a native"),
        }
    }

    #[test]
    fn to_real_scheme_matches_actual_behavior() {
        let (_, values) = load();
        assert!(matches!(
            call("\u{2197}", &values, Value::Int(42)),
            Ok(Value::Real(r)) if r == 42.0
        ));
    }

    #[test]
    fn to_real_never_raises_even_for_large_magnitudes() {
        let (_, values) = load();
        assert!(call("\u{2197}", &values, Value::Int(i64::MAX)).is_ok());
        assert!(call("\u{2197}", &values, Value::Int(i64::MIN)).is_ok());
    }

    #[test]
    fn to_int_truncates_toward_zero_for_positive_and_negative_fractions() {
        let (_, values) = load();
        assert!(matches!(
            call("\u{2198}", &values, Value::Real(3.9)),
            Ok(Value::Int(3))
        ));
        assert!(matches!(
            call("\u{2198}", &values, Value::Real(-3.9)),
            Ok(Value::Int(-3))
        ));
    }

    #[test]
    fn to_int_rejects_nan_and_infinity_as_invalid() {
        let (_, values) = load();
        for r in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            match &call("\u{2198}", &values, Value::Real(r)) {
                Err(Raised(Value::Adt { tag, .. })) => assert_eq!(&**tag, "InvalidOperation"),
                other => panic!("expected InvalidOperation, got {other:?}"),
            }
        }
    }

    #[test]
    fn to_int_rejects_out_of_i64_range_values_as_integer_overflow() {
        let (_, values) = load();
        for r in [1e30, -1e30, I64_MAX_EXCLUSIVE_AS_F64, I64_MIN_AS_F64 * 1.1] {
            match &call("\u{2198}", &values, Value::Real(r)) {
                Err(Raised(Value::Adt { tag, .. })) => assert_eq!(&**tag, "IntegerOverflow"),
                other => panic!("expected IntegerOverflow, got {other:?}"),
            }
        }
    }

    #[test]
    fn to_int_accepts_the_exact_i64_boundaries() {
        let (_, values) = load();
        assert!(matches!(
            call("\u{2198}", &values, Value::Real(I64_MIN_AS_F64)),
            Ok(Value::Int(i64::MIN))
        ));
    }
}
