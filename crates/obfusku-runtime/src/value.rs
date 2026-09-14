//! Runtime value representation.
//!
//! `Optional`/`Result` are absent as variants: they're ordinary
//! user-defined ADTs (§9/§19), so [`Value::Adt`] is all they need.

use crate::env::Env;
use obfusku_core::ast::Expr;
use std::cell::RefCell;
use std::rc::Rc;

/// Code plus the environment captured when the `Lambda` was evaluated
/// (§13) — captured uniformly for every free variable, `MutCell`-bound
/// or not.
#[derive(Debug)]
pub struct ClosureData {
    pub param: String,
    pub body: Rc<Expr>,
    pub env: Env,
}

/// A host-provided function value. Multi-argument natives compose via
/// currying (`Value -> EvalResult`), sharing the same calling convention
/// as closures. `name` is for [`std::fmt::Debug`] only.
pub struct NativeFn {
    pub name: String,
    pub func: Box<dyn Fn(Value) -> crate::eval::EvalResult>,
}

impl std::fmt::Debug for NativeFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<native fn {}>", self.name)
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Real(f64),
    Str(Rc<str>),
    Bool(bool),
    Unit,
    Closure(Rc<ClosureData>),
    /// Never structurally equal to anything, same as `Closure` — falls
    /// through to `structural_eq`'s catch-all `_ => false`.
    Native(Rc<NativeFn>),
    /// §12 — `Rc<RefCell<_>>` gives cell identity: cells from the same
    /// allocation/capture clone the `Rc`, never the contents, so
    /// mutation through one is visible through the other.
    Cell(Rc<RefCell<Value>>),
    /// §9 — an ordinary ADT value. `Rc<[Value]>`, not `Vec`: values are
    /// persistent, so cloning (an ordinary `Var` lookup) must be O(1)
    /// structural sharing, not a deep copy.
    Adt {
        tag: Rc<str>,
        args: Rc<[Value]>,
    },
    /// §9.2 — a persistent, immutable sequence; same `Rc<[Value]>`
    /// sharing reason as [`Value::Adt`]'s `args`.
    Array(Rc<[Value]>),
}

impl Value {
    /// §18 — structural equality. Cells compare by identity, not
    /// contents (§12.1). Closures are never equal to anything.
    pub fn structural_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            // §18: `==` is total — NaN == NaN is true.
            (Value::Real(a), Value::Real(b)) => a == b || (a.is_nan() && b.is_nan()),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Cell(a), Value::Cell(b)) => Rc::ptr_eq(a, b),
            (Value::Adt { tag: t1, args: a1 }, Value::Adt { tag: t2, args: a2 }) => {
                t1 == t2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(x, y)| x.structural_eq(y))
            }
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.structural_eq(y))
            }
            _ => false,
        }
    }
}
