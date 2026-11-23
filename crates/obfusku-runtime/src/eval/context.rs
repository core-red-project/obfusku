//! Per-thread evaluation state: recursion depth, call trace, raise location.
//!
//! **Why thread-local.** A host native is a plain `Fn(Value) -> EvalResult`
//! with no evaluator handle (`ADR-021`), yet it may call back into
//! [`apply_value`](super::apply_value) (a `map` over a list calling the
//! user's function). That nested evaluation must continue the *same* depth
//! count, or recursion through a native would escape the limit, and the same
//! call trace, or the trace would lose every frame above the native. The
//! context is therefore parked in a thread-local while a native runs and
//! picked back up by whichever evaluation entry the native calls.

use obfusku_diagnostics::Span;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Innermost call-trace frames kept at a raise.
const TRACE_DEPTH: usize = 12;

/// One function activation seen from a raise: what was called, and where.
#[derive(Debug, Clone)]
pub struct TraceFrame {
    /// The callee's name; `λ` if anonymous.
    pub function: Rc<str>,
    /// The call expression that entered it.
    pub call_site: Span,
}

thread_local! {
    static LIMIT: Cell<usize> = const { Cell::new(usize::MAX) };
    /// Source location of the most recent raise, for `eval_top`'s diagnostic.
    static RAISE_SPAN: Cell<Option<Span>> = const { Cell::new(None) };
    /// The innermost call frames at the most recent raise.
    static RAISE_TRACE: RefCell<Vec<TraceFrame>> = const { RefCell::new(Vec::new()) };
    /// The active context while a native runs, so a callback continues the
    /// same depth count and call trace.
    static PARKED: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// The state of one evaluation, threaded explicitly through the evaluator.
#[derive(Default)]
pub(super) struct Ctx {
    /// Nested evaluation frames so far.
    pub(super) depth: usize,
    /// The recursion cap, fixed when the evaluation started.
    pub(super) limit: usize,
    /// Live function activations, outermost first.
    pub(super) calls: Vec<TraceFrame>,
    /// The raise in flight already has its trace recorded.
    pub(super) is_captured: bool,
}

impl Ctx {
    fn fresh() -> Ctx {
        Ctx {
            limit: LIMIT.with(Cell::get),
            ..Ctx::default()
        }
    }

    /// The context to evaluate with: the one a native parked if this is a
    /// callback from inside it, otherwise a fresh one.
    pub(super) fn acquire() -> Ctx {
        PARKED
            .with(|p| p.borrow_mut().take())
            .unwrap_or_else(Ctx::fresh)
    }

    /// Parks the context for the next entry, or for a native about to run.
    pub(super) fn release(self) {
        PARKED.with(|p| *p.borrow_mut() = Some(self));
    }

    /// Records the call trace of the raise now propagating, once.
    ///
    /// The raise unwinds through every enclosing frame, each of which would
    /// try to capture; only the innermost (first) sees the full stack.
    pub(super) fn capture(&mut self) {
        if self.is_captured {
            return;
        }
        self.is_captured = true;
        let from = self.calls.len().saturating_sub(TRACE_DEPTH);
        RAISE_TRACE.with(|t| {
            let mut t = t.borrow_mut();
            t.clear();
            t.extend(self.calls[from..].iter().rev().cloned());
        });
    }
}

/// Remembers where the most recent raise happened.
pub(super) fn mark_raise(span: Span) {
    RAISE_SPAN.with(|s| s.set(Some(span)));
}

/// The location of the most recent raise, clearing it.
pub(super) fn take_raise_span() -> Option<Span> {
    RAISE_SPAN.with(Cell::take)
}

/// The call trace of the most recent uncaught raise on this thread,
/// innermost first; cleared by taking it.
#[must_use]
pub fn take_trace() -> Vec<TraceFrame> {
    RAISE_TRACE.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

/// Caps nested evaluation frames on this thread; exceeding it raises a
/// catchable `InvalidOperation` instead of overflowing the Rust stack.
///
/// The caller must size its stack for the limit it sets: the evaluator
/// recurses on the Rust stack for everything that is not a tail call.
pub fn set_recursion_limit(limit: usize) {
    LIMIT.with(|l| l.set(limit));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(name: &str) -> TraceFrame {
        TraceFrame {
            function: Rc::from(name),
            call_site: Span::default(),
        }
    }

    #[test]
    fn a_fresh_context_takes_the_threads_limit() {
        set_recursion_limit(77);
        let cx = Ctx::acquire();
        assert_eq!(cx.limit, 77);
        set_recursion_limit(usize::MAX);
    }

    #[test]
    fn a_released_context_is_the_next_one_acquired() {
        let mut cx = Ctx::acquire();
        cx.depth = 5;
        cx.release();
        assert_eq!(Ctx::acquire().depth, 5);
    }

    #[test]
    fn capture_keeps_the_innermost_frames_first_and_only_once() {
        let mut cx = Ctx {
            calls: vec![frame("outer"), frame("inner")],
            ..Ctx::default()
        };
        cx.capture();
        cx.calls.push(frame("later"));
        cx.capture();
        let names: Vec<_> = take_trace()
            .iter()
            .map(|f| f.function.to_string())
            .collect();
        assert_eq!(names, ["inner", "outer"]);
        assert!(take_trace().is_empty(), "taking clears the trace");
    }

    #[test]
    fn capture_is_bounded_to_the_innermost_frames() {
        let mut cx = Ctx {
            calls: (0..100).map(|i| frame(&i.to_string())).collect(),
            ..Ctx::default()
        };
        cx.capture();
        let trace = take_trace();
        assert_eq!(trace.len(), TRACE_DEPTH);
        assert_eq!(&*trace[0].function, "99");
    }

    #[test]
    fn the_raise_span_is_read_once() {
        let span = Span::new(obfusku_diagnostics::SourceId(0), 1, 2);
        mark_raise(span);
        assert_eq!(take_raise_span(), Some(span));
        assert_eq!(take_raise_span(), None);
    }
}
