//! The tree-walking evaluator — `spec/SEMANTIC_CORE.md` §12–§15.

use crate::env::Env;
use crate::value::{ClosureData, Value};
use obfusku_core::ast::{BinOp, Expr, Literal, Pattern, UnOp};
use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::rc::Rc;

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

/// Converts an uncaught `raise` into a [`Diagnostic`].
pub fn eval_top(expr: &Expr, env: &Env) -> Result<Value, Diagnostic> {
    match eval(expr, env) {
        Ok(v) => Ok(v),
        Err(Raised(payload)) => Err(rt_error(
            expr.span(),
            format!("uncaught exception: {payload:?}"),
        )),
    }
}

/// Either borrowed from the caller, or promoted to an owned `Rc` once a
/// tail continuation needs it to outlive that borrow.
///
/// Not `Rc<Expr>` everywhere: an unconditional `Rc::new(expr.clone())`
/// at the top of every call deep-clones a `Constructor`'s whole argument
/// subtree, and every non-tail recursive call re-triggers that clone one
/// level down — O(n²) on deep nesting despite having nothing to do with
/// TCO. Borrowing by default removes it.
enum Cur<'a> {
    Borrowed(&'a Expr),
    Owned(Rc<Expr>),
}

impl<'a> Cur<'a> {
    fn get(&self) -> &Expr {
        match self {
            Cur::Borrowed(e) => e,
            Cur::Owned(rc) => rc,
        }
    }
}

/// Loops in place for every tail-position expression (a `Lambda` body
/// reached via `Apply`, a `Match` arm's result) so tail recursion runs
/// in constant Rust stack space (§14.1). Every other sub-evaluation is a
/// genuine recursive call, which is what makes `Catch`'s TCO carve-out
/// fall out for free: it needs its own frame to intercept a `Raised`
/// unwinding through it, so its body is never a trampoline continuation.
/// Applies an already-evaluated `Value` (a `Closure` or `Native`) to an
/// already-evaluated argument — the same dispatch `Expr::Apply` performs
/// below, factored out for a host-provided native (`ADR-021`) to invoke
/// a caller-supplied function value without going through the Core AST
/// at all. Deliberately not trampolined: a native calling this is never
/// itself in tail position from the evaluator's point of view, and the
/// combinators that need it (`Array`'s `map`/`filter`/`fold`) iterate in
/// an ordinary Rust loop rather than recursing per element, so this
/// costs one bounded Rust frame per call, not per collection size.
pub fn apply_value(fv: Value, av: Value) -> EvalResult {
    match fv {
        Value::Closure(c) => {
            let call_env = c.env.child();
            call_env.bind(c.param.clone(), av);
            eval(&c.body, &call_env)
        }
        Value::Native(f) => (f.func)(av),
        other => Err(Raised(err_value(
            Span::default(),
            format!("attempted to call a non-function value: {other:?}"),
        ))),
    }
}

pub fn eval(expr: &Expr, env: &Env) -> EvalResult {
    let mut cur: Cur = Cur::Borrowed(expr);
    let mut cur_env: Env = env.clone();

    loop {
        match cur.get() {
            Expr::Var(name, span) => {
                return cur_env
                    .lookup(name)
                    .ok_or_else(|| Raised(err_value(*span, format!("unbound variable '{name}'"))));
            }
            Expr::Lit(lit, _) => return Ok(literal_value(lit)),
            Expr::Lambda { param, body, .. } => {
                return Ok(Value::Closure(Rc::new(ClosureData {
                    param: param.clone(),
                    body: Rc::new((**body).clone()),
                    env: cur_env.clone(),
                })));
            }
            Expr::Apply { func, arg, span } => {
                let fv = eval(func, &cur_env)?;
                let av = eval(arg, &cur_env)?;
                match fv {
                    Value::Closure(c) => {
                        let call_env = c.env.child();
                        call_env.bind(c.param.clone(), av);
                        cur = Cur::Owned(c.body.clone());
                        cur_env = call_env;
                        continue;
                    }
                    // A host-provided native — no Core AST body to
                    // trampoline into, so this is an ordinary (non-tail)
                    // Rust call.
                    Value::Native(f) => return (f.func)(av),
                    other => {
                        return Err(Raised(err_value(
                            *span,
                            format!("attempted to call a non-function value: {other:?}"),
                        )));
                    }
                }
            }
            Expr::MutCell { initial, .. } => {
                let v = eval(initial, &cur_env)?;
                return Ok(Value::Cell(Rc::new(std::cell::RefCell::new(v))));
            }
            Expr::MutRead { cell, span } => {
                let cv = eval(cell, &cur_env)?;
                return match cv {
                    Value::Cell(c) => Ok(c.borrow().clone()),
                    other => Err(Raised(err_value(
                        *span,
                        format!("attempted to read a non-cell value: {other:?}"),
                    ))),
                };
            }
            Expr::MutRebind {
                cell,
                new_value,
                span,
            } => {
                let cv = eval(cell, &cur_env)?;
                let nv = eval(new_value, &cur_env)?;
                return match cv {
                    Value::Cell(c) => {
                        *c.borrow_mut() = nv;
                        Ok(Value::Unit)
                    }
                    other => Err(Raised(err_value(
                        *span,
                        format!("attempted to rebind a non-cell value: {other:?}"),
                    ))),
                };
            }
            Expr::Constructor { tag, args, .. } => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(eval(a, &cur_env)?);
                }
                return Ok(Value::Adt {
                    tag: Rc::from(tag.as_str()),
                    args: Rc::from(values),
                });
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let sv = eval(scrutinee, &cur_env)?;
                let mut matched = None;
                for arm in arms {
                    let arm_env = cur_env.child();
                    if bind_pattern(&arm.pattern, &sv, &arm_env) {
                        matched = Some((arm_env, arm.result.clone()));
                        break;
                    }
                }
                match matched {
                    Some((arm_env, result)) => {
                        cur = Cur::Owned(Rc::new(result));
                        cur_env = arm_env;
                        continue;
                    }
                    // Defensive: should be unreachable once typecheck's
                    // exhaustiveness check (§17) has run.
                    None => {
                        return Err(Raised(Value::Adt {
                            tag: Rc::from("NonExhaustiveMatch"),
                            args: Rc::from(vec![]),
                        }));
                    }
                }
            }
            Expr::Raise { value, .. } => {
                let v = eval(value, &cur_env)?;
                return Err(Raised(v));
            }
            Expr::Catch {
                body,
                handler_param,
                handler_body,
                ..
            } => {
                // Non-tail: `body` needs its own frame so a `Raised`
                // unwinding through it is caught here, not skipped by a
                // trampoline continuation (§14.1's TCO carve-out).
                match eval(body, &cur_env) {
                    Ok(v) => return Ok(v),
                    Err(Raised(payload)) => {
                        let handler_env = cur_env.child();
                        handler_env.bind(handler_param.clone(), payload);
                        cur = Cur::Owned(Rc::new((**handler_body).clone()));
                        cur_env = handler_env;
                        continue;
                    }
                }
            }

            Expr::BinaryOp { op, lhs, rhs, span } => {
                let lv = eval(lhs, &cur_env)?;
                let rv = eval(rhs, &cur_env)?;
                return eval_binop(*op, lv, rv, *span);
            }
            Expr::UnaryOp { op, operand, span } => {
                let v = eval(operand, &cur_env)?;
                return eval_unop(*op, v, *span);
            }

            Expr::Let {
                name, value, body, ..
            } => {
                let v = eval(value, &cur_env)?;
                let new_env = cur_env.child();
                new_env.bind(name.clone(), v);
                cur = Cur::Owned(Rc::new((**body).clone()));
                cur_env = new_env;
                continue;
            }

            // Allocates the group's frame first, filling it as each
            // member evaluates: a member's `Lambda` safely captures a
            // reference to a still-filling frame, since a slot is only
            // read when its closure is later applied (§11).
            Expr::LetRec { bindings, body, .. } => {
                let group_env = cur_env.child();
                for binding in bindings {
                    let v = eval(&binding.value, &group_env)?;
                    group_env.bind(binding.name.clone(), v);
                }
                cur = Cur::Owned(Rc::new((**body).clone()));
                cur_env = group_env;
                continue;
            }

            // Delegated to a helper to avoid inflating eval's stack frame
            // on the recursive path.
            Expr::ArrayLiteral { elements, .. } => {
                return eval_array_literal(elements, &cur_env);
            }
            Expr::Index { array, index, span } => {
                let av = eval(array, &cur_env)?;
                let iv = eval(index, &cur_env)?;
                return eval_index(av, iv, *span);
            }
        }
    }
}

fn eval_array_literal(elements: &[Expr], env: &Env) -> EvalResult {
    let mut values = Vec::with_capacity(elements.len());
    for e in elements {
        values.push(eval(e, env)?);
    }
    Ok(Value::Array(Rc::from(values)))
}

fn eval_index(av: Value, iv: Value, span: Span) -> EvalResult {
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
        Value::Int(i) => i,
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
            args: Rc::from(vec![Value::Str(Rc::from(format!(
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
    use BinOp::*;
    match (op, lv, rv) {
        // Fixed-width `i64`: an unrepresentable result raises `IntegerOverflow`,
        // deterministic and catchable across all build profiles. `Real` has no
        // equivalent case — IEEE-754 is total (§15.3).
        (Add, Value::Int(a), Value::Int(b)) => a
            .checked_add(b)
            .map(Value::Int)
            .ok_or_else(|| Raised(integer_overflow(span))),
        (Add, Value::Real(a), Value::Real(b)) => Ok(Value::Real(a + b)),
        (Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(Rc::from(format!("{a}{b}").as_str()))),

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

        (Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (Lt, Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (Gt, Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a > b)),
        (Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (Le, Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a <= b)),
        (Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
        (Ge, Value::Real(a), Value::Real(b)) => Ok(Value::Bool(a >= b)),

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
            format!("invalid operand type for operator (typecheck should have rejected this): {op:?} on {v:?}"),
        ))),
    }
}

fn division_by_zero(_span: Span) -> Value {
    Value::Adt {
        tag: Rc::from("DivisionByZero"),
        args: Rc::from(Vec::new()),
    }
}

fn integer_overflow(_span: Span) -> Value {
    Value::Adt {
        tag: Rc::from("IntegerOverflow"),
        args: Rc::from(Vec::new()),
    }
}

fn literal_value(lit: &Literal) -> Value {
    match lit {
        Literal::Int(v) => Value::Int(*v),
        Literal::Real(v) => Value::Real(*v),
        Literal::Str(v) => Value::Str(Rc::from(v.as_str())),
        Literal::Bool(v) => Value::Bool(*v),
        Literal::Unit => Value::Unit,
    }
}

/// Runtime errors are raised as ordinary `Exception::InvalidOperation`
/// values, not a separate control channel — lets a surface `Catch`
/// intercept them the same way as a user `raise` (§15).
fn err_value(_span: Span, message: impl Into<String>) -> Value {
    Value::Adt {
        tag: Rc::from("InvalidOperation"),
        args: Rc::from(vec![Value::Str(Rc::from(message.into().as_str()))]),
    }
}

fn bind_pattern(pattern: &Pattern, scrutinee: &Value, env: &Env) -> bool {
    match pattern {
        Pattern::Wildcard(_) => true,
        Pattern::Var(name, _) => {
            env.bind(name.clone(), scrutinee.clone());
            true
        }
        Pattern::Lit(lit, _) => literal_value(lit).structural_eq(scrutinee),
        Pattern::Constructor { tag, args, .. } => match scrutinee {
            Value::Adt {
                tag: vtag,
                args: vargs,
            } if vtag.as_ref() == tag.as_str() && vargs.len() == args.len() => args
                .iter()
                .zip(vargs.iter())
                .all(|(p, v)| bind_pattern(p, v, env)),
            _ => false,
        },
    }
}
