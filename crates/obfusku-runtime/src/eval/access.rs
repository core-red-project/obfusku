//! Records and arrays: field read, field update, indexing.

use super::raise::{EvalResult, Raised, err_value};
use crate::value::Value;
use crate::variants;
use obfusku_diagnostics::Span;
use std::rc::Rc;

/// `base.name`: the field of a record value.
pub(super) fn eval_field(base: &Value, name: &str, span: Span) -> EvalResult {
    match base {
        Value::Adt { tag, args } => match variants::field_index(tag, name) {
            Some(i) if i < args.len() => Ok(args[i].clone()),
            _ => Err(Raised(err_value(
                span,
                format!("'{tag}' has no field '{name}'"),
            ))),
        },
        other => Err(Raised(err_value(
            span,
            format!("attempted to read field '{name}' of a non-record value: {other:?}"),
        ))),
    }
}

/// `base.{ name: value, … }`: a copy of a record with some fields replaced.
/// Fields apply left to right, so a name written twice keeps the last.
pub(super) fn eval_update(base: &Value, fields: Vec<(&String, Value)>, span: Span) -> EvalResult {
    let Value::Adt { tag, args } = base else {
        return Err(Raised(err_value(
            span,
            format!("attempted to update a non-record value: {base:?}"),
        )));
    };
    let mut new_args = args.to_vec();
    for (name, value) in fields {
        match variants::field_index(tag, name) {
            Some(i) if i < new_args.len() => new_args[i] = value,
            _ => {
                return Err(Raised(err_value(
                    span,
                    format!("'{tag}' has no field '{name}'"),
                )));
            }
        }
    }
    Ok(Value::Adt {
        tag: Rc::clone(tag),
        args: Rc::from(new_args),
    })
}

/// `array[index]`.
///
/// A negative or too-large index raises `InvalidOperation`, catchable like
/// any exception: an out-of-range read is a program error to recover from,
/// not a reason to abort the process.
pub(super) fn eval_index(array: &Value, index: &Value, span: Span) -> EvalResult {
    let Value::Array(items) = array else {
        return Err(Raised(err_value(
            span,
            format!("attempted to index a non-array value: {array:?}"),
        )));
    };
    let Value::Int(i) = index else {
        return Err(Raised(err_value(
            span,
            format!("array index must be Int, found: {index:?}"),
        )));
    };
    match usize::try_from(*i).ok().and_then(|i| items.get(i)) {
        Some(v) => Ok(v.clone()),
        None => Err(Raised(err_value(
            span,
            format!(
                "array index out of bounds: index {i}, length {}",
                items.len()
            ),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent::PVec;
    use obfusku_core::ast::{TypeDecl, VariantDecl};

    fn point(x: i64, y: i64) -> Value {
        variants::register(&[TypeDecl {
            tag: "AccPoint".into(),
            type_params: vec![],
            variants: vec![VariantDecl {
                tag: "AccPoint".into(),
                fields: vec![],
                field_names: Some(vec!["x".into(), "y".into()]),
                span: Span::default(),
            }],
            span: Span::default(),
        }]);
        Value::Adt {
            tag: Rc::from("AccPoint"),
            args: Rc::from(vec![Value::Int(x), Value::Int(y)]),
        }
    }

    fn int_of(v: &Value) -> i64 {
        match v {
            Value::Int(i) => *i,
            other => panic!("expected Int, got {other:?}"),
        }
    }

    fn message(r: &EvalResult) -> String {
        match r {
            Err(Raised(Value::Adt { args, .. })) => match &args[0] {
                Value::Str(s) => s.to_string(),
                other => panic!("expected a message, got {other:?}"),
            },
            other => panic!("expected an error, got {other:?}"),
        }
    }

    fn array(xs: &[i64]) -> Value {
        Value::Array(PVec::from_vec(xs.iter().map(|&i| Value::Int(i)).collect()))
    }

    #[test]
    fn a_field_is_read_by_name() {
        let got = eval_field(&point(1, 2), "y", Span::default()).unwrap();
        assert_eq!(int_of(&got), 2);
    }

    #[test]
    fn a_missing_field_or_non_record_is_an_error() {
        assert!(message(&eval_field(&point(1, 2), "z", Span::default())).contains("no field 'z'"));
        assert!(message(&eval_field(&Value::Int(1), "x", Span::default())).contains("non-record"));
    }

    #[test]
    fn update_replaces_fields_and_leaves_the_original() {
        let base = point(1, 2);
        let name = "x".to_string();
        let updated = eval_update(&base, vec![(&name, Value::Int(9))], Span::default()).unwrap();
        assert_eq!(
            int_of(&eval_field(&updated, "x", Span::default()).unwrap()),
            9
        );
        assert_eq!(
            int_of(&eval_field(&updated, "y", Span::default()).unwrap()),
            2
        );
        assert_eq!(int_of(&eval_field(&base, "x", Span::default()).unwrap()), 1);
    }

    #[test]
    fn update_with_a_repeated_name_keeps_the_last_value() {
        let name = "x".to_string();
        let updated = eval_update(
            &point(1, 2),
            vec![(&name, Value::Int(5)), (&name, Value::Int(6))],
            Span::default(),
        )
        .unwrap();
        assert_eq!(
            int_of(&eval_field(&updated, "x", Span::default()).unwrap()),
            6
        );
    }

    #[test]
    fn update_of_an_unknown_field_or_non_record_is_an_error() {
        let z = "z".to_string();
        assert!(
            message(&eval_update(
                &point(1, 2),
                vec![(&z, Value::Unit)],
                Span::default()
            ))
            .contains("no field 'z'")
        );
        assert!(
            message(&eval_update(&Value::Unit, vec![], Span::default())).contains("non-record")
        );
    }

    #[test]
    fn indexing_reads_an_element() {
        let got = eval_index(&array(&[10, 20]), &Value::Int(1), Span::default()).unwrap();
        assert_eq!(int_of(&got), 20);
    }

    #[test]
    fn out_of_range_and_negative_indexes_name_index_and_length() {
        for i in [2, -1] {
            let msg = message(&eval_index(
                &array(&[10, 20]),
                &Value::Int(i),
                Span::default(),
            ));
            assert_eq!(
                msg,
                format!("array index out of bounds: index {i}, length 2")
            );
        }
    }

    #[test]
    fn indexing_a_non_array_or_with_a_non_int_is_an_error() {
        assert!(
            message(&eval_index(&Value::Int(1), &Value::Int(0), Span::default()))
                .contains("non-array")
        );
        assert!(
            message(&eval_index(&array(&[1]), &Value::Unit, Span::default()))
                .contains("must be Int")
        );
    }
}
