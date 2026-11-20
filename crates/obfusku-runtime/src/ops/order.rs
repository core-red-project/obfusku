//! The generated total order and the comparison operators built on it.

use crate::value::Value;
use crate::variants;
use std::cmp::Ordering;

/// A total order, consistent with structural equality
/// (`SEMANTIC_CORE.md` §18).
///
/// `NaN` equals itself and sorts above every other `Real`, and `-0.0` equals
/// `0.0`, so sorting and map keys are deterministic. Strings compare by
/// bytes (which for UTF-8 is code-point order); constructors of one type
/// compare by declaration order, then their fields left to right; sequences
/// compare lexicographically with the shorter first.
#[must_use]
pub fn cmp_total(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Real(x), Value::Real(y)) => match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        },
        (Value::Str(x), Value::Str(y)) => x.as_bytes().cmp(y.as_bytes()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Unit, Value::Unit) => Ordering::Equal,
        (Value::Adt { tag: ta, args: aa }, Value::Adt { tag: tb, args: ab }) => {
            // The same constructor (the common case when sorting) needs no
            // declaration-order lookup.
            if ta == tb {
                return cmp_seq(aa.iter(), ab.iter());
            }
            variants::ordinal(ta)
                .cmp(&variants::ordinal(tb))
                .then_with(|| ta.cmp(tb))
        }
        (Value::Array(x), Value::Array(y)) => cmp_seq(x.to_vec().iter(), y.to_vec().iter()),
        (Value::Map(x), Value::Map(y)) => {
            let (ex, ey) = (x.entries(), y.entries());
            cmp_seq(
                ex.iter().flat_map(|(k, v)| [k, v]),
                ey.iter().flat_map(|(k, v)| [k, v]),
            )
        }
        (Value::Set(x), Value::Set(y)) => {
            let (ex, ey) = (x.entries(), y.entries());
            cmp_seq(ex.iter().map(|(k, _)| k), ey.iter().map(|(k, _)| k))
        }
        (Value::Bytes(x), Value::Bytes(y)) => x.cmp(y),
        // Unreachable through well-typed source (no order for functions or
        // cells, or across kinds); defined only so the function is total.
        _ => Ordering::Equal,
    }
}

/// Lexicographic comparison, shorter first when one is a prefix.
fn cmp_seq<'a>(
    mut a: impl Iterator<Item = &'a Value>,
    mut b: impl Iterator<Item = &'a Value>,
) -> Ordering {
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match cmp_total(x, y) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }
}

/// The ordering behind `<`, `>`, `≤`, `≥`.
///
/// A bare `Real` follows IEEE-754: `NaN` is unordered, so `None` and every
/// comparison is `false`. Everything else uses [`cmp_total`]. (A `Real`
/// *inside* a structure is ordered totally, so sorting a list holding `NaN`
/// still works.)
#[must_use]
pub fn cmp_operator(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Real(x), Value::Real(y)) => x.partial_cmp(y),
        _ => Some(cmp_total(a, b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent::{PMap, PVec, Text};
    use std::rc::Rc;

    fn int(i: i64) -> Value {
        Value::Int(i)
    }
    fn real(r: f64) -> Value {
        Value::Real(r)
    }
    fn text(s: &str) -> Value {
        Value::Str(Text::from(s))
    }
    fn adt(tag: &str, args: Vec<Value>) -> Value {
        Value::Adt {
            tag: Rc::from(tag),
            args: Rc::from(args),
        }
    }
    fn array(xs: &[i64]) -> Value {
        Value::Array(PVec::from_vec(xs.iter().map(|&i| int(i)).collect()))
    }

    #[test]
    fn integers_order_numerically() {
        assert_eq!(cmp_total(&int(-1), &int(2)), Ordering::Less);
        assert_eq!(cmp_total(&int(2), &int(2)), Ordering::Equal);
    }

    #[test]
    fn nan_is_equal_to_itself_and_above_everything() {
        let nan = real(f64::NAN);
        assert_eq!(cmp_total(&nan, &nan), Ordering::Equal);
        assert_eq!(cmp_total(&nan, &real(f64::INFINITY)), Ordering::Greater);
        assert_eq!(cmp_total(&real(1.0), &nan), Ordering::Less);
    }

    #[test]
    fn negative_zero_equals_zero() {
        assert_eq!(cmp_total(&real(-0.0), &real(0.0)), Ordering::Equal);
    }

    #[test]
    fn strings_compare_by_bytes() {
        assert_eq!(cmp_total(&text("a"), &text("b")), Ordering::Less);
        assert_eq!(cmp_total(&text("ab"), &text("a")), Ordering::Greater);
    }

    #[test]
    fn false_sorts_before_true() {
        assert_eq!(
            cmp_total(&Value::Bool(false), &Value::Bool(true)),
            Ordering::Less
        );
    }

    #[test]
    fn constructors_of_one_type_order_by_declaration() {
        crate::variants::register(&[obfusku_core::ast::TypeDecl {
            tag: "OrdKind".into(),
            type_params: vec![],
            variants: ["OrdFirst", "OrdSecond"]
                .iter()
                .map(|t| obfusku_core::ast::VariantDecl {
                    tag: (*t).into(),
                    fields: vec![],
                    field_names: None,
                    span: obfusku_diagnostics::Span::default(),
                })
                .collect(),
            span: obfusku_diagnostics::Span::default(),
        }]);
        assert_eq!(
            cmp_total(&adt("OrdFirst", vec![]), &adt("OrdSecond", vec![])),
            Ordering::Less
        );
    }

    #[test]
    fn the_same_constructor_compares_its_fields_left_to_right() {
        let a = adt("Pair", vec![int(1), int(9)]);
        let b = adt("Pair", vec![int(2), int(0)]);
        assert_eq!(cmp_total(&a, &b), Ordering::Less);
    }

    #[test]
    fn sequences_are_lexicographic_and_shorter_first() {
        assert_eq!(cmp_total(&array(&[1, 2]), &array(&[1, 3])), Ordering::Less);
        assert_eq!(cmp_total(&array(&[1]), &array(&[1, 0])), Ordering::Less);
        assert_eq!(cmp_total(&array(&[]), &array(&[])), Ordering::Equal);
    }

    #[test]
    fn maps_compare_by_their_interleaved_entries() {
        let m = |k: i64| Value::Map(PMap::new().insert(int(k), int(0)));
        assert_eq!(cmp_total(&m(1), &m(2)), Ordering::Less);
    }

    #[test]
    fn bytes_compare_lexicographically() {
        let b = |xs: &[u8]| Value::Bytes(Rc::from(xs));
        assert_eq!(cmp_total(&b(&[1, 2]), &b(&[1, 3])), Ordering::Less);
    }

    #[test]
    fn unrelated_kinds_are_defined_as_equal_to_stay_total() {
        assert_eq!(cmp_total(&int(1), &text("a")), Ordering::Equal);
    }

    #[test]
    fn the_operator_ordering_of_nan_is_unordered() {
        assert_eq!(cmp_operator(&real(f64::NAN), &real(1.0)), None);
        assert_eq!(cmp_operator(&real(1.0), &real(2.0)), Some(Ordering::Less));
    }

    #[test]
    fn the_operator_ordering_of_other_kinds_is_total() {
        assert_eq!(cmp_operator(&int(1), &int(2)), Some(Ordering::Less));
        assert_eq!(
            cmp_operator(&array(&[1]), &array(&[1])),
            Some(Ordering::Equal)
        );
    }
}
