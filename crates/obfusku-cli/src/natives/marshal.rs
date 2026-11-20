//! Moving values across the host boundary.
//!
//! A native sees runtime [`Value`]s, not typed Rust arguments, because the
//! checker already guaranteed the shapes. Every accessor here still returns
//! `InvalidOperation` on a mismatch instead of panicking: a hand-written
//! type scheme can drift from its closure (`natives.rs` explains why that
//! is only caught by tests), and a wrong call must stay a catchable
//! exception rather than abort the interpreter.
//!
//! Three groups: constructors for the ambient `List`/`Optional`/`Result`/
//! `Failure` shapes, argument accessors, and the type-expression helpers
//! the schemes are written with.

use obfusku_runtime::eval::Raised;
use obfusku_runtime::{Text, Value};
use obfusku_typecheck::Type;
use std::rc::Rc;

/// An ADT value with constructor `tag`.
#[must_use]
pub fn adt(tag: &str, args: Vec<Value>) -> Value {
    Value::Adt {
        tag: Rc::from(tag),
        args: Rc::from(args),
    }
}

/// A `Str` value.
#[must_use]
pub fn str_value(text: &str) -> Value {
    Value::Str(Text::from(text))
}

/// `Optional`: `Some` is `⧫ x`, `None` is `⦰`.
#[must_use]
pub fn optional(v: Option<Value>) -> Value {
    match v {
        Some(x) => adt("⧫", vec![x]),
        None => adt("⦰", vec![]),
    }
}

/// A `List` value built from `items`, in order. Built back to front so the
/// cons cells are allocated once, with no intermediate reversal.
#[must_use]
pub fn list_from(items: Vec<Value>) -> Value {
    items
        .into_iter()
        .rev()
        .fold(adt("⊘", vec![]), |tail, item| adt("⁝", vec![item, tail]))
}

/// The elements of a `List` value. Iterative on purpose: a long list must
/// not overflow the Rust stack, which a recursive walk would.
pub fn vec_from_list(op: &str, v: &Value) -> Result<Vec<Value>, Raised> {
    let mut out = Vec::new();
    let mut cur = v.clone();
    loop {
        let next = match &cur {
            Value::Adt { tag, args } if &**tag == "⊘" && args.is_empty() => return Ok(out),
            Value::Adt { tag, args } if &**tag == "⁝" && args.len() == 2 => {
                out.push(args[0].clone());
                args[1].clone()
            }
            other => return Err(invalid(format!("'{op}' expects a List, found {other:?}"))),
        };
        cur = next;
    }
}

/// The built-in `Failure(tag, message)` value (`SEMANTIC_CORE.md` §15.2).
/// Its two-`Str` shape must match the checker's hard-registered `Failure`.
#[must_use]
pub fn failure(tag: &str, message: &str) -> Value {
    adt("Failure", vec![str_value(tag), str_value(message)])
}

/// The `Failure` tag for an OS error: permission problems are a distinct,
/// catchable condition (`ADR-011`); everything else is a plain `IOError`.
#[must_use]
pub fn io_error_tag(e: &std::io::Error) -> &'static str {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "PermissionDenied"
    } else {
        "IOError"
    }
}

/// An exception for the failed host operation `op`.
#[must_use]
pub fn io_fail(op: &str, e: &std::io::Error) -> Raised {
    Raised(failure(io_error_tag(e), &format!("{op}: {e}")))
}

/// An `InvalidOperation` exception carrying `message`. Reaching it means a
/// native was called with a value its scheme forbids.
#[must_use]
pub fn invalid(message: impl AsRef<str>) -> Raised {
    Raised(adt("InvalidOperation", vec![str_value(message.as_ref())]))
}

/// Most elements a single native will allocate on a count argument; a
/// larger request raises instead of aborting the process on allocation.
pub const MAX_COUNT: usize = 1 << 28;

/// A non-negative count no larger than [`MAX_COUNT`].
pub fn count_arg(op: &str, what: &str, n: i64) -> Result<usize, Raised> {
    let count = usize::try_from(n)
        .map_err(|_| invalid(format!("'{op}' expects a non-negative {what}, found {n}")))?;
    if count > MAX_COUNT {
        return Err(invalid(format!(
            "'{op}' expects a {what} of at most {MAX_COUNT}, found {n}"
        )));
    }
    Ok(count)
}

/// `v` as `Str`, or `InvalidOperation` naming `op`.
pub fn str_arg<'a>(op: &str, v: &'a Value) -> Result<&'a str, Raised> {
    match v {
        Value::Str(t) => Ok(t),
        other => Err(invalid(format!("'{op}' expects a Str, found {other:?}"))),
    }
}

/// `v` as `Int`, or `InvalidOperation` naming `op`.
pub fn int_arg(op: &str, v: &Value) -> Result<i64, Raised> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(invalid(format!("'{op}' expects an Int, found {other:?}"))),
    }
}

/// `v` as bytes, or `InvalidOperation` naming `op`.
pub fn bytes_arg<'a>(op: &str, v: &'a Value) -> Result<&'a [u8], Raised> {
    match v {
        Value::Bytes(b) => Ok(b),
        other => Err(invalid(format!("'{op}' expects bytes, found {other:?}"))),
    }
}

/// Requires the unit argument of a nullary-looking native (`∅ → a`):
/// `Apply` is arity-1, so "no argument" is spelled `∅`.
pub fn unit_arg(op: &str, v: &Value) -> Result<(), Raised> {
    match v {
        Value::Unit => Ok(()),
        other => Err(invalid(format!("'{op}' expects Unit, found {other:?}"))),
    }
}

/// The function type `from → to`.
#[must_use]
pub fn func(from: Type, to: Type) -> Type {
    Type::Function(Box::new(from), Box::new(to))
}

/// `∅ → t`, the shape of every natively-read value.
#[must_use]
pub fn unit_to(t: Type) -> Type {
    func(Type::Unit, t)
}

/// The byte-string type `⌭` (`ADR-027`).
#[must_use]
pub fn bytes_type() -> Type {
    Type::Adt("⌭".to_string(), vec![])
}

/// `List t`.
#[must_use]
pub fn list_of(t: Type) -> Type {
    Type::Adt("List".to_string(), vec![t])
}

/// `Optional t`.
#[must_use]
pub fn optional_of(t: Type) -> Type {
    Type::Adt("Optional".to_string(), vec![t])
}

/// `Result ok err`.
#[must_use]
pub fn result_of(ok: Type, err: Type) -> Type {
    Type::Adt("Result".to_string(), vec![ok, err])
}

/// `Array t`.
#[must_use]
pub fn array_of(t: Type) -> Type {
    Type::Adt("Array".to_string(), vec![t])
}

/// `Map k v`.
#[must_use]
pub fn map_of(k: Type, v: Type) -> Type {
    Type::Adt("Map".to_string(), vec![k, v])
}

/// `Set k`.
#[must_use]
pub fn set_of(k: Type) -> Type {
    Type::Adt("Set".to_string(), vec![k])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(xs: &[i64]) -> Vec<Value> {
        xs.iter().map(|x| Value::Int(*x)).collect()
    }

    fn tag_of(r: &Raised) -> String {
        match &r.0 {
            Value::Adt { tag, .. } => tag.to_string(),
            other => panic!("expected an exception ADT, got {other:?}"),
        }
    }

    #[test]
    fn a_list_round_trips_in_order() {
        let list = list_from(ints(&[1, 2, 3]));
        let back = vec_from_list("t", &list).unwrap();
        assert!(matches!(
            back[..],
            [Value::Int(1), Value::Int(2), Value::Int(3)]
        ));
        assert!(vec_from_list("t", &list_from(vec![])).unwrap().is_empty());
    }

    #[test]
    fn a_very_long_list_does_not_overflow_the_stack() {
        let n = 200_000;
        let list = list_from(vec![Value::Unit; n]);
        assert_eq!(vec_from_list("t", &list).unwrap().len(), n);
    }

    #[test]
    fn a_non_list_is_invalid_operation_naming_the_op() {
        let err = vec_from_list("⟐", &Value::Int(1)).unwrap_err();
        assert_eq!(tag_of(&err), "InvalidOperation");
        assert!(format!("{err:?}").contains("'⟐' expects a List"));
    }

    #[test]
    fn optional_uses_the_ambient_constructors() {
        assert!(
            matches!(&optional(None), Value::Adt { tag, args } if &**tag == "⦰" && args.is_empty())
        );
        assert!(matches!(&optional(Some(Value::Unit)), Value::Adt { tag, .. } if &**tag == "⧫"));
    }

    #[test]
    fn count_arg_rejects_negative_and_oversized_counts() {
        assert_eq!(count_arg("op", "length", 3).unwrap(), 3);
        assert!(count_arg("op", "length", -1).is_err());
        assert!(count_arg("op", "length", MAX_COUNT as i64 + 1).is_err());
        assert_eq!(
            count_arg("op", "length", MAX_COUNT as i64).unwrap(),
            MAX_COUNT
        );
    }

    #[test]
    fn argument_accessors_reject_the_wrong_variant_without_panicking() {
        assert!(str_arg("op", &Value::Int(1)).is_err());
        assert!(int_arg("op", &str_value("x")).is_err());
        assert!(bytes_arg("op", &Value::Unit).is_err());
        assert!(unit_arg("op", &Value::Int(0)).is_err());
        assert_eq!(str_arg("op", &str_value("x")).unwrap(), "x");
        assert_eq!(int_arg("op", &Value::Int(7)).unwrap(), 7);
        assert!(unit_arg("op", &Value::Unit).is_ok());
    }

    #[test]
    fn os_permission_errors_are_distinguished_from_other_io_errors() {
        use std::io::{Error, ErrorKind};
        assert_eq!(
            io_error_tag(&Error::from(ErrorKind::PermissionDenied)),
            "PermissionDenied"
        );
        assert_eq!(io_error_tag(&Error::from(ErrorKind::NotFound)), "IOError");
    }

    #[test]
    fn io_fail_prefixes_the_operation() {
        let e = io_fail("readFile", &std::io::Error::other("boom"));
        assert!(format!("{e:?}").contains("readFile: boom"));
    }

    #[test]
    fn type_helpers_build_the_ambient_adts() {
        assert_eq!(
            list_of(Type::Int),
            Type::Adt("List".into(), vec![Type::Int])
        );
        assert_eq!(
            map_of(Type::Str, Type::Int),
            Type::Adt("Map".into(), vec![Type::Str, Type::Int])
        );
        assert_eq!(unit_to(Type::Str), func(Type::Unit, Type::Str));
    }
}
