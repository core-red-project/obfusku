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

use obfusku_runtime::eval::Raised;
use obfusku_runtime::Value;
use obfusku_typecheck::{Prelude, Scheme, Type};
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

fn invalid_operation(message: impl Into<String>) -> Raised {
    Raised(Value::Adt {
        tag: Rc::from("InvalidOperation"),
        args: Rc::from(vec![Value::Str(Rc::from(message.into().as_str()))]),
    })
}

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
        return Err(invalid_operation(format!(
            "'\u{2198}' expects a finite Real, found {r}"
        )));
    }
    let truncated = r.trunc();
    if truncated < I64_MIN_AS_F64 || truncated >= I64_MAX_EXCLUSIVE_AS_F64 {
        return Err(integer_overflow());
    }
    Ok(truncated as i64)
}

pub fn load() -> (Prelude, HashMap<String, Value>) {
    let mut types = Prelude::new();
    let mut values = HashMap::new();

    types.insert(
        "\u{2197}".to_string(),
        Scheme::monomorphic(Type::Function(Box::new(Type::Int), Box::new(Type::Real))),
    );
    values.insert(
        "\u{2197}".to_string(),
        Value::Native(Rc::new(obfusku_runtime::NativeFn {
            name: "\u{2197}".to_string(),
            func: Box::new(|arg| match arg {
                Value::Int(i) => Ok(Value::Real(i as f64)),
                other => Err(invalid_operation(format!(
                    "'\u{2197}' expects an Int, found {other:?}"
                ))),
            }),
        })),
    );

    types.insert(
        "\u{2198}".to_string(),
        Scheme::monomorphic(Type::Function(Box::new(Type::Real), Box::new(Type::Int))),
    );
    values.insert(
        "\u{2198}".to_string(),
        Value::Native(Rc::new(obfusku_runtime::NativeFn {
            name: "\u{2198}".to_string(),
            func: Box::new(|arg| match arg {
                Value::Real(r) => real_to_int(r).map(Value::Int),
                other => Err(invalid_operation(format!(
                    "'\u{2198}' expects a Real, found {other:?}"
                ))),
            }),
        })),
    );

    (types, values)
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
    fn to_int_rejects_nan_and_infinity_as_invalid_operation() {
        let (_, values) = load();
        for r in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            match call("\u{2198}", &values, Value::Real(r)) {
                Err(Raised(Value::Adt { tag, .. })) => assert_eq!(&*tag, "InvalidOperation"),
                other => panic!("expected InvalidOperation, got {other:?}"),
            }
        }
    }

    #[test]
    fn to_int_rejects_out_of_i64_range_values_as_integer_overflow() {
        let (_, values) = load();
        for r in [1e30, -1e30, I64_MAX_EXCLUSIVE_AS_F64, I64_MIN_AS_F64 * 1.1] {
            match call("\u{2198}", &values, Value::Real(r)) {
                Err(Raised(Value::Adt { tag, .. })) => assert_eq!(&*tag, "IntegerOverflow"),
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
