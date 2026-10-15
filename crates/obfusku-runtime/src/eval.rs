//! The evaluator over the resolved IR — `spec/SEMANTIC_CORE.md` §12–§15.

use crate::env::Env;
use crate::ir::{Arm, Node, Pat};
use crate::ops::cmp_operator;
use crate::pvec::PVec;
use crate::text::Text;
use crate::types;
use crate::value::{ClosureData, Value};
use obfusku_core::ast::{BinOp, UnOp};
use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::rc::Rc;

/// Innermost call-trace frames kept at a raise.
const TRACE_DEPTH: usize = 12;

/// One function activation seen from a raise: what was called, and where.
#[derive(Debug, Clone)]
pub struct TraceFrame {
    pub function: Rc<str>,
    pub call_site: Span,
}

thread_local! {
    static LIMIT: Cell<usize> = const { Cell::new(usize::MAX) };
    /// Source location of the most recent raise, for `eval_top`'s diagnostic.
    static RAISE_SPAN: Cell<Option<Span>> = const { Cell::new(None) };
    /// `CALLS`' innermost entries at the most recent raise.
    static RAISE_TRACE: RefCell<Vec<TraceFrame>> = const { RefCell::new(Vec::new()) };
    /// The active context while a native runs, so a callback into
    /// `apply_value` continues the same depth count and call trace.
    static STASH: RefCell<Option<Ctx>> = const { RefCell::new(None) };
}

/// Per-evaluation state, threaded explicitly: nesting depth against the
/// recursion limit, and the live function activations for traces.
#[derive(Default)]
struct Ctx {
    depth: usize,
    limit: usize,
    calls: Vec<TraceFrame>,
    /// The current raise already has its trace.
    captured: bool,
}

impl Ctx {
    fn fresh() -> Ctx {
        Ctx {
            limit: LIMIT.with(Cell::get),
            ..Ctx::default()
        }
    }

    fn capture(&mut self) {
        if self.captured {
            return;
        }
        self.captured = true;
        let from = self.calls.len().saturating_sub(TRACE_DEPTH);
        RAISE_TRACE.with(|t| {
            let mut t = t.borrow_mut();
            t.clear();
            t.extend(self.calls[from..].iter().rev().cloned());
        });
    }
}

fn mark_raise(span: Span) {
    RAISE_SPAN.with(|s| s.set(Some(span)));
}

/// The call trace of the most recent uncaught raise on this thread,
/// innermost first; cleared by taking it.
#[must_use]
pub fn take_trace() -> Vec<TraceFrame> {
    RAISE_TRACE.with(|t| std::mem::take(&mut *t.borrow_mut()))
}

/// Caps nested `eval` frames on this thread; exceeding it raises a catchable
/// `InvalidOperation` instead of overflowing the Rust stack. The caller must
/// size its stack for the limit it sets.
pub fn set_recursion_limit(limit: usize) {
    LIMIT.with(|l| l.set(limit));
}

/// A §15 `raise` in flight, propagated via `Result::Err` through Rust's
/// own call stack — a `Catch` frame is "active" for exactly the Rust
/// frames nested inside its (non-tail) evaluation of `body`.
#[derive(Debug)]
pub struct Raised(pub Value);

pub type EvalResult = Result<Value, Raised>;

fn rt_error(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: message.into(),
        primary: span,
    }
}

/// Converts an uncaught `raise` into a [`Diagnostic`]; the call trace
/// stays available through [`take_trace`].
pub fn eval_top(node: &Node, env: &Env, fallback: Span) -> Result<Value, Diagnostic> {
    match eval(node, env) {
        Ok(v) => Ok(v),
        Err(Raised(payload)) => Err(rt_error(
            RAISE_SPAN.with(Cell::take).unwrap_or(fallback),
            format!("uncaught exception: {}", crate::ops::show(&payload)),
        )),
    }
}

/// Applies an already-evaluated `Value` (a `Closure` or `Native`) to an
/// already-evaluated argument — the same dispatch `Node::Apply` performs
/// below, for a host-provided native (`ADR-021`) invoking a
/// caller-supplied function value. Deliberately not trampolined: a
/// native calling this is never in tail position, and the combinators
/// that need it iterate in an ordinary Rust loop.
pub fn apply_value(fv: &Value, av: Value) -> EvalResult {
    let mut cx = STASH
        .with(|s| s.borrow_mut().take())
        .unwrap_or_else(Ctx::fresh);
    let out = match fv {
        Value::Closure(c) => eval_in(&c.body, &c.env.push_one(av), &mut cx),
        Value::Native(f) => call_native(f, av, &mut cx),
        other => Err(Raised(err_value(
            Span::default(),
            format!("attempted to call a non-function value: {other:?}"),
        ))),
    };
    STASH.with(|s| *s.borrow_mut() = Some(cx));
    out
}

/// Depth units one native activation is charged: its Rust frames (the
/// native, its closure, the callback entry) are several times an `eval`
/// level, and the limit is sized for `eval` frames.
const NATIVE_DEPTH_COST: usize = 16;

/// Runs a native with the context parked where its callbacks find it.
fn call_native(f: &crate::value::NativeFn, av: Value, cx: &mut Ctx) -> EvalResult {
    if cx.depth.saturating_add(NATIVE_DEPTH_COST) > cx.limit {
        return Err(Raised(limit_error(cx)));
    }
    cx.depth += NATIVE_DEPTH_COST;
    STASH.with(|s| *s.borrow_mut() = Some(std::mem::take(cx)));
    let out = (f.func)(av);
    *cx = STASH
        .with(|s| s.borrow_mut().take())
        .unwrap_or_else(Ctx::fresh);
    cx.depth = cx.depth.saturating_sub(NATIVE_DEPTH_COST);
    out
}

/// What one evaluation step produced: a finished value, or a node in
/// tail position to continue with in its own environment. `call` is set
/// when the continuation is a function body.
enum Step {
    Done(Value),
    Tail(Rc<Node>, Env, Option<TraceFrame>),
}

// Same signature as the other `step_*` helpers, so call sites stay uniform.
#[allow(clippy::unnecessary_wraps)]
fn done(v: Value) -> Result<Step, Raised> {
    Ok(Step::Done(v))
}

enum Cur<'a> {
    Borrowed(&'a Node),
    Owned(Rc<Node>),
}

impl Cur<'_> {
    fn get(&self) -> &Node {
        match self {
            Cur::Borrowed(n) => n,
            Cur::Owned(rc) => rc,
        }
    }
}

/// Loops in place for every tail-position node (a function body reached
/// via `Apply`, a `Match` arm's result) so tail recursion runs in
/// constant Rust stack space (§14.1). Every other sub-evaluation is a
/// genuine recursive call, which is what makes `Catch`'s TCO carve-out
/// fall out for free.
pub fn eval(node: &Node, env: &Env) -> EvalResult {
    let mut cx = STASH
        .with(|s| s.borrow_mut().take())
        .unwrap_or_else(Ctx::fresh);
    cx.captured = false;
    let out = eval_in(node, env, &mut cx);
    STASH.with(|s| *s.borrow_mut() = Some(cx));
    out
}

/// Leaves inline at the call site; everything else enters [`eval_in`].
// Leaf fast path of the evaluator: forcing the inline was measured faster.
#[allow(clippy::inline_always)]
#[inline(always)]
fn ev(node: &Node, env: &Env, cx: &mut Ctx) -> EvalResult {
    match node {
        Node::Local { depth, slot } => Ok(env.get(*depth, *slot)),
        Node::Lit(v) => Ok(v.clone()),
        _ => eval_in(node, env, cx),
    }
}

fn eval_in(node: &Node, env: &Env, cx: &mut Ctx) -> EvalResult {
    // Leaves are most evaluations and cannot recurse.
    match node {
        Node::Local { depth, slot } => return Ok(env.get(*depth, *slot)),
        Node::Lit(v) => return Ok(v.clone()),
        Node::Global(cell, span) => return step_global(cell, *span),
        _ => {}
    }
    cx.depth += 1;
    let depth = cx.depth;
    // The limit is checked every 16 levels: the stack bound it protects
    // is coarse, and the check is a hot-path cost.
    if depth.trailing_zeros() >= 4 && depth > cx.limit {
        cx.depth -= 1;
        return Err(Raised(limit_error(cx)));
    }
    let (out, pushed) = run(node, env, cx);
    cx.depth -= 1;
    if pushed {
        cx.calls.pop();
    }
    out
}

/// The trampoline: returns the result and whether it left a trace entry
/// for the caller to pop.
fn run(node: &Node, env: &Env, cx: &mut Ctx) -> (EvalResult, bool) {
    let mut pushed = false;
    let mut cur: Cur = Cur::Borrowed(node);
    let mut cur_env: Env = env.clone();

    // Every arm that does real work is its own `#[inline(never)]`
    // function, so this recursive frame stays small even unoptimized.
    loop {
        let result = match cur.get() {
            Node::Local { depth, slot } => return (Ok(cur_env.get(*depth, *slot)), pushed),
            Node::Lit(v) => return (Ok(v.clone()), pushed),
            Node::Global(cell, span) => return (step_global(cell, *span), pushed),
            Node::Unbound(name, span) => Err(Raised(err_value(
                *span,
                format!("unbound variable '{name}'"),
            ))),
            Node::Lambda { body, name } => {
                return (
                    Ok(Value::Closure(Rc::new(ClosureData {
                        body: Rc::clone(body),
                        env: cur_env.clone(),
                        name: Rc::clone(name),
                    }))),
                    pushed,
                );
            }
            Node::Apply { func, arg, span } => step_apply(func, arg, *span, &cur_env, cx),
            Node::MutCell(initial) => step_mut_cell(initial, &cur_env, cx),
            Node::MutRead(cell, span) => step_mut_read(cell, *span, &cur_env, cx),
            Node::MutRebind { cell, value, span } => {
                step_mut_rebind(cell, value, *span, &cur_env, cx)
            }
            Node::Constructor { tag, args } => step_constructor(tag, args, &cur_env, cx),
            Node::Match { scrutinee, arms } => step_match(scrutinee, arms, &cur_env, cx),
            Node::Raise(value, span) => step_raise(value, *span, &cur_env, cx),
            Node::Catch { body, handler } => step_catch(body, handler, &cur_env, cx),
            Node::Binary { op, lhs, rhs, span } => step_binop(*op, lhs, rhs, *span, &cur_env, cx),
            Node::Unary { op, operand, span } => step_unop(*op, operand, *span, &cur_env, cx),
            Node::Let { value, body } => step_let(value, body, &cur_env, cx),
            Node::LetRec { values, body } => step_letrec(values, body, &cur_env, cx),
            Node::Array(elements) => step_array(elements, &cur_env, cx),
            Node::Index { array, index, span } => step_index(array, index, *span, &cur_env, cx),
            Node::Field { base, name, span } => step_field(base, name, *span, &cur_env, cx),
            Node::Update { base, fields, span } => step_update(base, fields, *span, &cur_env, cx),
        };
        match result {
            Ok(Step::Done(v)) => return (Ok(v), pushed),
            Ok(Step::Tail(next, env, call)) => {
                if let Some(frame) = call {
                    if pushed {
                        *cx.calls.last_mut().expect("pushed") = frame;
                    } else {
                        cx.calls.push(frame);
                        pushed = true;
                    }
                }
                cur = Cur::Owned(next);
                cur_env = env;
            }
            Err(e) => {
                cx.capture();
                return (Err(e), pushed);
            }
        }
    }
}

#[inline(never)]
fn step_global(cell: &crate::ir::GlobalCell, span: Span) -> EvalResult {
    match &*cell.value.borrow() {
        Some(v) => Ok(v.clone()),
        None => Err(Raised(err_value(
            span,
            format!("unbound variable '{}'", cell.name),
        ))),
    }
}

#[inline(never)]
fn step_apply(
    func: &Node,
    arg: &Node,
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let fv = ev(func, env, cx)?;
    let av = ev(arg, env, cx)?;
    match &fv {
        Value::Closure(c) => Ok(Step::Tail(
            Rc::clone(&c.body),
            c.env.push_one(av),
            Some(TraceFrame {
                function: Rc::clone(&c.name),
                call_site: span,
            }),
        )),
        // A host-provided native — no body to trampoline into, so this
        // is an ordinary (non-tail) Rust call.
        Value::Native(f) => call_native(f, av, cx)
            .inspect_err(|_| mark_raise(span))
            .map(Step::Done),
        other => Err(Raised(err_value(
            span,
            format!("attempted to call a non-function value: {other:?}"),
        ))),
    }
}

#[inline(never)]
fn step_mut_cell(initial: &Node, env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let v = ev(initial, env, cx)?;
    done(Value::Cell(Rc::new(RefCell::new(v))))
}

#[inline(never)]
fn step_mut_read(cell: &Node, span: Span, env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    match &ev(cell, env, cx)? {
        Value::Cell(c) => done(c.borrow().clone()),
        other => Err(Raised(err_value(
            span,
            format!("attempted to read a non-cell value: {other:?}"),
        ))),
    }
}

#[inline(never)]
fn step_mut_rebind(
    cell: &Node,
    new_value: &Node,
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let cv = ev(cell, env, cx)?;
    let nv = ev(new_value, env, cx)?;
    match &cv {
        Value::Cell(c) => {
            *c.borrow_mut() = nv;
            done(Value::Unit)
        }
        other => Err(Raised(err_value(
            span,
            format!("attempted to rebind a non-cell value: {other:?}"),
        ))),
    }
}

#[inline(never)]
fn step_constructor(tag: &Rc<str>, args: &[Node], env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let mut values = Vec::with_capacity(args.len());
    for a in args {
        values.push(ev(a, env, cx)?);
    }
    done(Value::Adt {
        tag: Rc::clone(tag),
        args: Rc::from(values),
    })
}

#[inline(never)]
fn step_match(scrutinee: &Node, arms: &[Arm], env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let sv = ev(scrutinee, env, cx)?;
    for arm in arms {
        if arm.vars == 0 {
            if bind_pattern(&arm.pattern, &sv, &mut Vec::new()) {
                return Ok(Step::Tail(Rc::clone(&arm.result), env.clone(), None));
            }
        } else {
            let mut slots = Vec::with_capacity(arm.vars as usize);
            if bind_pattern(&arm.pattern, &sv, &mut slots) {
                let arm_env = if slots.len() == 1 {
                    env.push_one(slots.pop().expect("one"))
                } else {
                    env.push_many(slots)
                };
                return Ok(Step::Tail(Rc::clone(&arm.result), arm_env, None));
            }
        }
    }
    // Defensive: unreachable once typecheck's exhaustiveness check (§17)
    // has run.
    Err(Raised(Value::Adt {
        tag: Rc::from("NonExhaustiveMatch"),
        args: Rc::from(vec![]),
    }))
}

#[inline(never)]
fn step_raise(value: &Node, span: Span, env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let v = ev(value, env, cx)?;
    mark_raise(span);
    Err(Raised(v))
}

/// Non-tail: `body` needs its own frame so a `Raised` unwinding through it
/// is caught here, not skipped by a trampoline continuation (§14.1's TCO
/// carve-out). The handler itself runs in tail position.
#[inline(never)]
fn step_catch(body: &Node, handler: &Rc<Node>, env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    match ev(body, env, cx) {
        Ok(v) => done(v),
        Err(Raised(payload)) => {
            cx.captured = false;
            Ok(Step::Tail(Rc::clone(handler), env.push_one(payload), None))
        }
    }
}

#[inline(never)]
fn step_binop(
    op: BinOp,
    lhs: &Node,
    rhs: &Node,
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let lv = ev(lhs, env, cx)?;
    let rv = ev(rhs, env, cx)?;
    eval_binop(op, lv, rv, span).map(Step::Done)
}

#[inline(never)]
fn step_unop(
    op: UnOp,
    operand: &Node,
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let v = ev(operand, env, cx)?;
    eval_unop(op, v, span).map(Step::Done)
}

#[inline(never)]
fn step_let(value: &Node, body: &Rc<Node>, env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let v = ev(value, env, cx)?;
    Ok(Step::Tail(Rc::clone(body), env.push_one(v), None))
}

/// Allocates the group's frame first, filling it as each member
/// evaluates: a member's `Lambda` safely captures a reference to a
/// still-filling frame, since a slot is only read when its closure is
/// later applied (§11).
#[inline(never)]
fn step_letrec(values: &[Node], body: &Rc<Node>, env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let group = env.push_many(vec![Value::Unit; values.len()]);
    for (i, v) in values.iter().enumerate() {
        let value = ev(v, &group, cx)?;
        group.set(i as u32, value);
    }
    Ok(Step::Tail(Rc::clone(body), group, None))
}

#[inline(never)]
fn step_field(
    base: &Node,
    name: &str,
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let bv = ev(base, env, cx)?;
    eval_field(&bv, name, span).map(Step::Done)
}

#[inline(never)]
fn step_update(
    base: &Node,
    fields: &[(String, Node)],
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let bv = ev(base, env, cx)?;
    let mut new_values = Vec::with_capacity(fields.len());
    for (name, e) in fields {
        new_values.push((name, ev(e, env, cx)?));
    }
    eval_update(&bv, new_values, span).map(Step::Done)
}

#[inline(never)]
fn step_array(elements: &[Node], env: &Env, cx: &mut Ctx) -> Result<Step, Raised> {
    let mut values = Vec::with_capacity(elements.len());
    for e in elements {
        values.push(ev(e, env, cx)?);
    }
    done(Value::Array(PVec::from_vec(values)))
}

#[inline(never)]
fn step_index(
    array: &Node,
    index: &Node,
    span: Span,
    env: &Env,
    cx: &mut Ctx,
) -> Result<Step, Raised> {
    let av = ev(array, env, cx)?;
    let iv = ev(index, env, cx)?;
    eval_index(&av, &iv, span).map(Step::Done)
}

fn eval_field(bv: &Value, name: &str, span: Span) -> EvalResult {
    match bv {
        Value::Adt { tag, args } => match types::field_index(tag, name) {
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

fn eval_update(bv: &Value, fields: Vec<(&String, Value)>, span: Span) -> EvalResult {
    let Value::Adt { tag, args } = bv else {
        return Err(Raised(err_value(
            span,
            format!("attempted to update a non-record value: {bv:?}"),
        )));
    };
    let mut new_args = args.to_vec();
    for (name, v) in fields {
        match types::field_index(tag, name) {
            Some(i) if i < new_args.len() => new_args[i] = v,
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

fn eval_index(av: &Value, iv: &Value, span: Span) -> EvalResult {
    let arr = match av {
        Value::Array(a) => a,
        other => {
            return Err(Raised(err_value(
                span,
                format!("attempted to index a non-array value: {other:?}"),
            )));
        }
    };
    let i = match iv {
        Value::Int(i) => *i,
        other => {
            return Err(Raised(err_value(
                span,
                format!("array index must be Int, found: {other:?}"),
            )));
        }
    };
    match usize::try_from(i).ok().and_then(|i| arr.get(i)) {
        Some(v) => Ok(v.clone()),
        None => Err(Raised(Value::Adt {
            tag: Rc::from("InvalidOperation"),
            args: Rc::from(vec![Value::Str(Text::from(format!(
                "array index out of bounds: index {i}, length {}",
                arr.len()
            )))]),
        })),
    }
}

/// §20.2's closed operator table. No `Value::Number` fallback, no
/// Int↔Real coercion — a pair not recognized here is a caller error
/// (Core that never passed typecheck).
fn eval_binop(op: BinOp, lv: Value, rv: Value, span: Span) -> EvalResult {
    use BinOp::{Add, Div, Eq, Ge, Gt, Le, Lt, Mod, Mul, NotEq, Sub, Xor};
    match (op, lv, rv) {
        // Fixed-width `i64`: an unrepresentable result raises `IntegerOverflow`,
        // deterministic and catchable across all build profiles. `Real` has no
        // equivalent case — IEEE-754 is total (§15.3).
        (Add, Value::Int(a), Value::Int(b)) => a
            .checked_add(b)
            .map(Value::Int)
            .ok_or_else(|| Raised(integer_overflow(span))),
        (Add, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a + b)),
        (Add, Value::Str(ref a), Value::Str(ref b)) => Ok(Value::Str(Text::concat(a, b))),

        (Sub, Value::Int(a), Value::Int(b)) => a
            .checked_sub(b)
            .map(Value::Int)
            .ok_or_else(|| Raised(integer_overflow(span))),
        (Sub, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a - b)),

        (Mul, Value::Int(a), Value::Int(b)) => a
            .checked_mul(b)
            .map(Value::Int)
            .ok_or_else(|| Raised(integer_overflow(span))),
        (Mul, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a * b)),

        // §15.3: Int division by zero raises `DivisionByZero`; the one
        // other unrepresentable case (`MIN_INT / -1`) is `IntegerOverflow`
        // instead — genuinely a different condition, not a second zero
        // check. Real follows IEEE-754.
        (Div, Value::Int(a), Value::Int(b)) => {
            if b == 0 {
                Err(Raised(division_by_zero(span)))
            } else {
                a.checked_div(b)
                    .map(Value::Int)
                    .ok_or_else(|| Raised(integer_overflow(span)))
            }
        }
        (Div, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a / b)),

        // Truncating, sign of dividend (Rust's own `%`); raises on zero,
        // or `IntegerOverflow` for the same `MIN_INT % -1` case `÷` has.
        (Mod, Value::Int(a), Value::Int(b)) => {
            if b == 0 {
                Err(Raised(division_by_zero(span)))
            } else {
                a.checked_rem(b)
                    .map(Value::Int)
                    .ok_or_else(|| Raised(integer_overflow(span)))
            }
        }

        // `Int` is the common case; keep it off the generic path.
        (Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        // Generated ordering over any orderable type (`ADR-025`); a bare
        // `Real` follows IEEE-754, so `NaN` makes all four `false`.
        (Lt, a, b) => Ok(Value::Bool(matches!(
            cmp_operator(&a, &b),
            Some(Ordering::Less)
        ))),
        (Gt, a, b) => Ok(Value::Bool(matches!(
            cmp_operator(&a, &b),
            Some(Ordering::Greater)
        ))),
        (Le, a, b) => Ok(Value::Bool(matches!(
            cmp_operator(&a, &b),
            Some(Ordering::Less | Ordering::Equal)
        ))),
        (Ge, a, b) => Ok(Value::Bool(matches!(
            cmp_operator(&a, &b),
            Some(Ordering::Greater | Ordering::Equal)
        ))),

        // Total structural equality, including NaN == NaN (§18/§20.2).
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

fn eval_unop(op: UnOp, v: Value, span: Span) -> EvalResult {
    match (op, v) {
        (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
        // `-MIN_INT` is the one unrepresentable case (§20.2).
        (UnOp::Neg, Value::Int(n)) => n
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| Raised(integer_overflow(span))),
        (UnOp::Neg, Value::Real(n)) => Ok(Value::Real(-n)),
        (op, v) => Err(Raised(err_value(
            span,
            format!(
                "invalid operand type for operator (typecheck should have rejected this): {op:?} on {v:?}"
            ),
        ))),
    }
}

fn division_by_zero(span: Span) -> Value {
    mark_raise(span);
    Value::Adt {
        tag: Rc::from("DivisionByZero"),
        args: Rc::from(Vec::new()),
    }
}

fn integer_overflow(span: Span) -> Value {
    mark_raise(span);
    Value::Adt {
        tag: Rc::from("IntegerOverflow"),
        args: Rc::from(Vec::new()),
    }
}

/// Runtime errors are raised as ordinary `Exception::InvalidOperation`
/// values, not a separate control channel — lets a surface `Catch`
/// intercept them the same way as a user `raise` (§15).
/// Anchored at the innermost call in flight rather than at nothing.
fn limit_error(cx: &Ctx) -> Value {
    let span = cx.calls.last().map_or_else(Span::default, |f| f.call_site);
    err_value(span, "recursion limit exceeded")
}

fn err_value(span: Span, message: impl Into<String>) -> Value {
    mark_raise(span);
    Value::Adt {
        tag: Rc::from("InvalidOperation"),
        args: Rc::from(vec![Value::Str(Text::from(message.into().as_str()))]),
    }
}

/// Matches `pattern` against `scrutinee`, pushing each variable's value
/// in the order the resolver numbered its slots.
fn bind_pattern(pattern: &Pat, scrutinee: &Value, out: &mut Vec<Value>) -> bool {
    match pattern {
        Pat::Wild => true,
        Pat::Var => {
            out.push(scrutinee.clone());
            true
        }
        Pat::Lit(lit) => lit.structural_eq(scrutinee),
        Pat::Con { tag, args } => match scrutinee {
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
