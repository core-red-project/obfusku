//! Exceptions in flight and the runtime's own raises.

use super::context::{Ctx, mark_raise};
use crate::persistent::Text;
use crate::value::Value;
use obfusku_diagnostics::Span;
use std::rc::Rc;

/// A `raise` propagating (`SEMANTIC_CORE.md` §15).
///
/// It travels as `Result::Err` through Rust's own call stack, so a `Catch`
/// is "active" for exactly the Rust frames nested inside its (non-tail)
/// evaluation of `body`. That makes the TCO carve-out for `Catch` (§14.1)
/// fall out of the structure instead of needing a handler stack.
#[derive(Debug)]
pub struct Raised(pub Value);

/// The result of evaluating an expression.
pub type EvalResult = Result<Value, Raised>;

/// An `Exception` value with the given constructor and fields.
fn exception(tag: &str, args: Vec<Value>) -> Value {
    Value::Adt {
        tag: Rc::from(tag),
        args: Rc::from(args),
    }
}

/// The exception for a failure of the runtime itself, raised as an ordinary
/// `InvalidOperation` rather than a separate control channel, so a surface
/// `Catch` intercepts it exactly like a user `raise` (§15). Anchored at
/// `span`.
pub(super) fn err_value(span: Span, message: impl AsRef<str>) -> Value {
    mark_raise(span);
    exception(
        "InvalidOperation",
        vec![Value::Str(Text::from(message.as_ref()))],
    )
}

/// `DivisionByZero`, for integer `÷` and `⌗` by zero.
pub(super) fn division_by_zero(span: Span) -> Value {
    mark_raise(span);
    exception("DivisionByZero", Vec::new())
}

/// `IntegerOverflow`, for a result outside `i64`.
pub(super) fn integer_overflow(span: Span) -> Value {
    mark_raise(span);
    exception("IntegerOverflow", Vec::new())
}

/// `NonExhaustiveMatch`. Defensive: the checker's exhaustiveness pass (§17)
/// makes it unreachable for checked programs.
pub(super) fn non_exhaustive_match() -> Value {
    exception("NonExhaustiveMatch", Vec::new())
}

/// The recursion-limit error, anchored at the innermost call in flight
/// rather than at nothing.
pub(super) fn limit_error(cx: &Ctx) -> Value {
    let span = cx.calls.last().map_or_else(Span::default, |f| f.call_site);
    err_value(span, "recursion limit exceeded")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(v: &Value) -> &str {
        match v {
            Value::Adt { tag, .. } => tag,
            other => panic!("expected an exception, got {other:?}"),
        }
    }

    #[test]
    fn runtime_errors_are_invalid_operation_with_a_message() {
        let v = err_value(Span::default(), "boom");
        assert_eq!(tag(&v), "InvalidOperation");
        assert!(
            matches!(&v, Value::Adt { args, .. } if matches!(&args[0], Value::Str(s) if s == "boom"))
        );
    }

    #[test]
    fn the_arithmetic_exceptions_carry_no_payload() {
        for v in [
            division_by_zero(Span::default()),
            integer_overflow(Span::default()),
            non_exhaustive_match(),
        ] {
            assert!(matches!(&v, Value::Adt { args, .. } if args.is_empty()));
        }
        assert_eq!(tag(&division_by_zero(Span::default())), "DivisionByZero");
        assert_eq!(tag(&integer_overflow(Span::default())), "IntegerOverflow");
        assert_eq!(tag(&non_exhaustive_match()), "NonExhaustiveMatch");
    }

    #[test]
    fn the_limit_error_is_anchored_at_the_innermost_call() {
        let site = Span::new(obfusku_diagnostics::SourceId(0), 3, 4);
        let mut cx = Ctx::default();
        cx.calls.push(super::super::TraceFrame {
            function: Rc::from("f"),
            call_site: site,
        });
        let _ = limit_error(&cx);
        assert_eq!(super::super::context::take_raise_span(), Some(site));
    }
}
