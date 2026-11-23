//! The evaluator over the resolved IR (`spec/SEMANTIC_CORE.md` §12–§15).
//!
//! **Shape.** [`eval`] is a trampoline. Every node in *tail position* (a
//! function body reached by `Apply`, a `Match` arm's result, a `Let` body)
//! makes the loop continue in place, so tail recursion runs in constant Rust
//! stack (§14.1). Every other sub-evaluation is a genuine recursive call,
//! which is also what makes `Catch`'s TCO carve-out fall out for free: a
//! `Raised` unwinding through the Rust frames of `body` is caught right
//! there.
//!
//! | Module | Provides |
//! |---|---|
//! | `context` | recursion limit, call trace and raise location (per thread) |
//! | `raise` | [`Raised`], [`EvalResult`] and the runtime's own exceptions |
//! | `arith` | the operator table |
//! | `access` | field read/update and array indexing |
//! | `pattern` | matching a value against a compiled pattern |

mod access;
mod arith;
mod context;
mod pattern;
mod raise;

pub use context::{TraceFrame, set_recursion_limit, take_trace};
pub use raise::{EvalResult, Raised};

use crate::env::Env;
use crate::ir::{Arm, GlobalCell, Node};
use crate::persistent::PVec;
use crate::value::{ClosureData, NativeFn, Value};
use access::{eval_field, eval_index, eval_update};
use arith::{eval_binop, eval_unop};
use context::{Ctx, mark_raise, take_raise_span};
use obfusku_core::ast::{BinOp, UnOp};
use obfusku_diagnostics::{Diagnostic, Span};
use pattern::bind_pattern;
use raise::{err_value, limit_error, non_exhaustive_match};
use std::cell::RefCell;
use std::rc::Rc;

/// Evaluates `node`, converting an uncaught `raise` into a [`Diagnostic`].
///
/// The call trace of that raise stays available through [`take_trace`].
/// `fallback` locates the error when the raise recorded no location of its
/// own (the enclosing top-level binding).
pub fn eval_top(node: &Node, env: &Env, fallback: Span) -> Result<Value, Diagnostic> {
    match eval(node, env) {
        Ok(v) => Ok(v),
        Err(Raised(payload)) => Err(Diagnostic::error(
            format!("uncaught exception: {}", crate::ops::show(&payload)),
            take_raise_span().unwrap_or(fallback),
        )),
    }
}

/// Applies an already-evaluated function value to an already-evaluated
/// argument: the same dispatch `Node::Apply` performs, for a host native
/// (`ADR-021`) calling a caller-supplied function.
///
/// Deliberately not trampolined: a native calling this is never in tail
/// position, and the combinators that need it iterate in an ordinary Rust
/// loop.
pub fn apply_value(fv: &Value, av: Value) -> EvalResult {
    let mut cx = Ctx::acquire();
    let out = match fv {
        Value::Closure(c) => eval_in(&c.body, &c.env.push_one(av), &mut cx),
        Value::Native(f) => call_native(f, av, &mut cx),
        other => Err(Raised(err_value(
            Span::default(),
            format!("attempted to call a non-function value: {other:?}"),
        ))),
    };
    cx.release();
    out
}

/// Depth units one native activation is charged: its Rust frames (the
/// native, its closure, the callback entry) are several times an `eval`
/// level, and the limit is sized for `eval` frames.
const NATIVE_DEPTH_COST: usize = 16;

/// Runs a native with the context parked where its callbacks find it.
fn call_native(f: &NativeFn, av: Value, cx: &mut Ctx) -> EvalResult {
    if cx.depth.saturating_add(NATIVE_DEPTH_COST) > cx.limit {
        return Err(Raised(limit_error(cx)));
    }
    cx.depth += NATIVE_DEPTH_COST;
    std::mem::take(cx).release();
    let out = (f.func)(av);
    *cx = Ctx::acquire();
    cx.depth = cx.depth.saturating_sub(NATIVE_DEPTH_COST);
    out
}

/// What one evaluation step produced: a finished value, or a node in tail
/// position to continue with in its own environment. The `TraceFrame` is set
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

/// The node being run: borrowed from the caller's tree on entry, owned
/// (an `Rc` clone) once the trampoline has followed a tail call.
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

/// Evaluates `node` in `env`, starting a fresh evaluation (or continuing the
/// one a native parked).
pub fn eval(node: &Node, env: &Env) -> EvalResult {
    let mut cx = Ctx::acquire();
    cx.is_captured = false;
    let out = eval_in(node, env, &mut cx);
    cx.release();
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
    // The limit is checked every 16 levels: the stack bound it protects is
    // coarse, and the check is a hot-path cost.
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

/// The trampoline: returns the result and whether it left a trace entry for
/// the caller to pop.
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
fn step_global(cell: &GlobalCell, span: Span) -> EvalResult {
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
    Err(Raised(non_exhaustive_match()))
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
            cx.is_captured = false;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Compiler, Globals};
    use obfusku_core::ast::{Expr, Literal};
    use std::rc::Rc;

    fn sp() -> Span {
        Span::default()
    }

    fn int(v: i64) -> Expr {
        Expr::Lit(Literal::Int(v), sp())
    }

    fn lambda(param: &str, body: Expr) -> Expr {
        Expr::Lambda {
            param: param.into(),
            param_ty: None,
            body: Rc::new(body),
            span: sp(),
        }
    }

    fn apply(f: Expr, a: Expr) -> Expr {
        Expr::Apply {
            func: Box::new(f),
            arg: Box::new(a),
            span: sp(),
        }
    }

    fn run_expr(e: &Expr) -> EvalResult {
        let node = Compiler::new(&mut Globals::new()).expr(e, None);
        eval(&node, &Env::root())
    }

    #[test]
    fn a_lambda_applied_to_an_argument_returns_it() {
        let got = run_expr(&apply(lambda("x", Expr::Var("x".into(), sp())), int(5)));
        assert!(matches!(got, Ok(Value::Int(5))));
    }

    #[test]
    fn applying_a_non_function_is_an_error_value_not_a_panic() {
        let got = run_expr(&apply(int(1), int(2)));
        assert!(
            matches!(got, Err(Raised(Value::Adt { ref tag, .. })) if &**tag == "InvalidOperation")
        );
    }

    #[test]
    fn apply_value_calls_a_closure_and_a_native() {
        let closure = run_expr(&lambda("x", Expr::Var("x".into(), sp()))).unwrap();
        assert!(matches!(
            apply_value(&closure, Value::Int(3)),
            Ok(Value::Int(3))
        ));
        let native = Value::Native(Rc::new(NativeFn {
            name: "inc".into(),
            func: Box::new(|v| match v {
                Value::Int(i) => Ok(Value::Int(i + 1)),
                other => Ok(other),
            }),
        }));
        assert!(matches!(
            apply_value(&native, Value::Int(3)),
            Ok(Value::Int(4))
        ));
    }

    #[test]
    fn apply_value_on_a_non_function_raises() {
        assert!(apply_value(&Value::Int(1), Value::Unit).is_err());
    }

    #[test]
    fn an_uncaught_raise_becomes_a_diagnostic_at_its_span() {
        let at = Span::new(obfusku_diagnostics::SourceId(0), 4, 9);
        let node = Compiler::new(&mut Globals::new()).expr(
            &Expr::Raise {
                value: Box::new(Expr::Constructor {
                    tag: "Boom".into(),
                    args: vec![],
                    span: sp(),
                }),
                span: at,
            },
            None,
        );
        let d = eval_top(&node, &Env::root(), sp()).unwrap_err();
        assert_eq!(d.primary, at);
        assert_eq!(d.message, "uncaught exception: Boom");
    }

    #[test]
    fn deep_native_recursion_hits_the_limit_instead_of_the_stack() {
        set_recursion_limit(10_000);
        let native = Rc::new(RefCell::new(None::<Value>));
        let slot = Rc::clone(&native);
        let recurse = Value::Native(Rc::new(NativeFn {
            name: "recurse".into(),
            func: Box::new(move |v| {
                let me = slot.borrow().clone().unwrap();
                apply_value(&me, v)
            }),
        }));
        *native.borrow_mut() = Some(recurse.clone());
        let got = apply_value(&recurse, Value::Unit);
        set_recursion_limit(usize::MAX);
        *native.borrow_mut() = None;
        assert!(
            matches!(got, Err(Raised(Value::Adt { ref tag, .. })) if &**tag == "InvalidOperation")
        );
    }
}
