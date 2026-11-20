//! Matching a value against a compiled pattern.

use crate::ir::Pat;
use crate::value::Value;
use std::rc::Rc;

/// Matches `pattern` against `scrutinee`, pushing each variable's value onto
/// `out` in the order the compiler numbered the slots (left to right).
///
/// Returns whether it matched; on `false`, `out` holds a partial prefix the
/// caller discards.
pub(super) fn bind_pattern(pattern: &Pat, scrutinee: &Value, out: &mut Vec<Value>) -> bool {
    match pattern {
        Pat::Wild => true,
        Pat::Var => {
            out.push(scrutinee.clone());
            true
        }
        Pat::Lit(lit) => lit.structural_eq(scrutinee),
        Pat::Con { tag, args } => match scrutinee {
            // Interned tags make the pointer check the usual hit; the text
            // comparison covers tags built outside the compiler.
            Value::Adt {
                tag: vtag,
                args: vargs,
            } if (Rc::ptr_eq(vtag, tag) || **vtag == **tag) && vargs.len() == args.len() => args
                .iter()
                .zip(vargs.iter())
                .all(|(p, v)| bind_pattern(p, v, out)),
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::intern;

    fn adt(tag: &str, args: Vec<Value>) -> Value {
        Value::Adt {
            tag: Rc::from(tag),
            args: Rc::from(args),
        }
    }

    fn con(tag: &str, args: Vec<Pat>) -> Pat {
        Pat::Con {
            tag: intern(tag),
            args,
        }
    }

    fn binds(pattern: &Pat, value: &Value) -> Option<Vec<Value>> {
        let mut out = Vec::new();
        bind_pattern(pattern, value, &mut out).then_some(out)
    }

    #[test]
    fn wildcard_matches_without_binding() {
        assert_eq!(binds(&Pat::Wild, &Value::Int(1)).map(|v| v.len()), Some(0));
    }

    #[test]
    fn a_variable_binds_the_value() {
        let got = binds(&Pat::Var, &Value::Int(7)).unwrap();
        assert!(matches!(got[0], Value::Int(7)));
    }

    #[test]
    fn a_literal_matches_by_structural_equality() {
        assert!(binds(&Pat::Lit(Value::Int(1)), &Value::Int(1)).is_some());
        assert!(binds(&Pat::Lit(Value::Int(1)), &Value::Int(2)).is_none());
    }

    #[test]
    fn a_constructor_binds_its_fields_left_to_right() {
        let pat = con("Pair", vec![Pat::Var, Pat::Var]);
        let got = binds(&pat, &adt("Pair", vec![Value::Int(1), Value::Int(2)])).unwrap();
        assert!(matches!(got[..], [Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn a_different_tag_or_arity_does_not_match() {
        let pat = con("Pair", vec![Pat::Wild, Pat::Wild]);
        assert!(binds(&pat, &adt("Other", vec![Value::Unit, Value::Unit])).is_none());
        assert!(binds(&pat, &adt("Pair", vec![Value::Unit])).is_none());
        assert!(binds(&pat, &Value::Int(1)).is_none());
    }

    #[test]
    fn a_tag_that_is_not_interned_still_matches_by_text() {
        let pat = con("Tagged", vec![]);
        assert!(binds(&pat, &adt("Tagged", vec![])).is_some());
    }

    #[test]
    fn nested_patterns_bind_depth_first() {
        let pat = con("Box", vec![con("Pair", vec![Pat::Var, Pat::Wild])]);
        let value = adt("Box", vec![adt("Pair", vec![Value::Int(9), Value::Unit])]);
        let got = binds(&pat, &value).unwrap();
        assert!(matches!(got[..], [Value::Int(9)]));
    }
}
