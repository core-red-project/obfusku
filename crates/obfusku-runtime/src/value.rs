//! What evaluation produces: [`Value`] and its function representations.
//!
//! `Optional` and `Result` are not variants here: they are ordinary
//! user-declared ADTs (`SEMANTIC_CORE.md` §9, §19), so [`Value::Adt`] is all
//! they need. Keeping the set of variants closed and small is what lets the
//! generated equality, ordering and rendering (`ops`) be total functions.

use crate::env::Env;
use crate::eval::EvalResult;
use crate::ir::Node;
use crate::persistent::{PMap, PVec, Text};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// A function written in Obfusku: code plus the environment it was created
/// in (`SEMANTIC_CORE.md` §13).
///
/// The environment is captured uniformly for every free variable, mutable
/// cell or not. Whether a mutation is *seen* by the closure is decided by
/// whether the captured value is the cell itself or a read of it, not by
/// how capture works.
pub struct ClosureData {
    /// The compiled body, run with the argument pushed onto `env`.
    pub body: Rc<Node>,
    /// The environment active where the `λ` was evaluated.
    pub env: Env,
    /// The name it was bound to, for call traces; `λ` if anonymous.
    pub name: Rc<str>,
}

impl fmt::Debug for ClosureData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<function {}>", self.name)
    }
}

/// A function provided by the host (`ADR-021`): effects and combinators the
/// language cannot express itself.
///
/// It takes one argument and returns a value, exactly like a closure. A
/// multi-argument native is a native returning a native (currying), so the
/// evaluator has one calling convention. `name` is for `Debug` and `show`
/// only.
pub struct NativeFn {
    /// Display name, e.g. in `<native name>`.
    pub name: String,
    /// The implementation. Raise an exception by returning `Err(Raised(..))`.
    pub func: Box<dyn Fn(Value) -> EvalResult>,
}

impl fmt::Debug for NativeFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<native fn {}>", self.name)
    }
}

/// A runtime value.
///
/// `Clone` is cheap by construction: scalars are `Copy`-sized and every
/// compound payload is behind an `Rc` (or is itself a persistent structure),
/// because evaluating a variable clones its value and must not be O(size).
#[derive(Debug, Clone)]
pub enum Value {
    /// `⟁`: a 64-bit signed integer.
    Int(i64),
    /// `⧆`: an IEEE-754 double.
    Real(f64),
    /// `⌘`: an immutable string, stored as a rope.
    Str(Text),
    /// `○`: `◉` or `◎`.
    Bool(bool),
    /// `∅`.
    Unit,
    /// A function written in Obfusku. Never equal to anything.
    Closure(Rc<ClosureData>),
    /// A host function. Never equal to anything, like `Closure`.
    Native(Rc<NativeFn>),
    /// `SEMANTIC_CORE.md` §12: a mutable cell.
    ///
    /// `Rc<RefCell<_>>` gives cell *identity*: copies of the `Rc` alias one
    /// allocation, so a write through one is visible through the others.
    Cell(Rc<RefCell<Value>>),
    /// §9: an ordinary ADT value, `tag(args…)`.
    ///
    /// `Rc<[Value]>` rather than `Vec`: values are persistent, so cloning
    /// (every `Var` lookup) must share structure, not copy it.
    Adt {
        /// The constructor name, interned so equal tags are usually pointer
        /// equal.
        tag: Rc<str>,
        /// The constructor's fields, in declaration order.
        args: Rc<[Value]>,
    },
    /// §9.2: a persistent, immutable sequence.
    Array(PVec),
    /// A persistent ordered map; keys ordered by the generated total order.
    Map(PMap),
    /// A persistent ordered set (a map whose values are all `Unit`).
    Set(PMap),
    /// An immutable byte string (`⌭`, `ADR-027`).
    Bytes(Rc<[u8]>),
}

/// Frees a long `Cons` chain (or any deep ADT spine) without recursing.
///
/// The default drop would recurse once per element and overflow the stack on
/// a long list. The uniquely owned spine is instead unlinked iteratively, so
/// every node is dropped shallow.
impl Drop for Value {
    fn drop(&mut self) {
        fn take_nested(args: &mut Rc<[Value]>, out: &mut Vec<Value>) {
            if let Some(items) = Rc::get_mut(args) {
                for item in items {
                    if matches!(item, Value::Adt { .. }) {
                        out.push(std::mem::replace(item, Value::Unit));
                    }
                }
            }
        }
        let Value::Adt { args, .. } = self else {
            return;
        };
        let mut pending = Vec::new();
        take_nested(args, &mut pending);
        while let Some(mut node) = pending.pop() {
            if let Value::Adt { args, .. } = &mut node {
                take_nested(args, &mut pending);
            }
        }
    }
}

impl Value {
    /// Generated structural equality (`SEMANTIC_CORE.md` §18).
    ///
    /// Total and language-defined, never user-supplied. Cells compare by
    /// identity, not contents (§12.1), and closures and natives are never
    /// equal to anything (the final `_` arm), which the type checker rules
    /// out reaching here from source. `NaN` equals itself, unlike the
    /// ordering operators, so `≡` is an equivalence relation.
    #[must_use]
    pub fn structural_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
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
                a.len() == b.len()
                    && a.to_vec()
                        .iter()
                        .zip(b.to_vec().iter())
                        .all(|(x, y)| x.structural_eq(y))
            }
            (Value::Map(a), Value::Map(b)) => {
                let (ea, eb) = (a.entries(), b.entries());
                ea.len() == eb.len()
                    && ea
                        .iter()
                        .zip(eb.iter())
                        .all(|((k1, v1), (k2, v2))| k1.structural_eq(k2) && v1.structural_eq(v2))
            }
            (Value::Set(a), Value::Set(b)) => {
                let (ea, eb) = (a.entries(), b.entries());
                ea.len() == eb.len()
                    && ea
                        .iter()
                        .zip(eb.iter())
                        .all(|((k1, _), (k2, _))| k1.structural_eq(k2))
            }
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adt(tag: &str, args: Vec<Value>) -> Value {
        Value::Adt {
            tag: Rc::from(tag),
            args: Rc::from(args),
        }
    }

    fn native() -> Value {
        Value::Native(Rc::new(NativeFn {
            name: "f".into(),
            func: Box::new(Ok),
        }))
    }

    #[test]
    fn scalars_compare_by_value() {
        assert!(Value::Int(1).structural_eq(&Value::Int(1)));
        assert!(!Value::Int(1).structural_eq(&Value::Int(2)));
        assert!(Value::Str(Text::from("a")).structural_eq(&Value::Str(Text::from("a"))));
        assert!(Value::Unit.structural_eq(&Value::Unit));
        assert!(Value::Bool(true).structural_eq(&Value::Bool(true)));
    }

    #[test]
    fn values_of_different_kinds_are_never_equal() {
        assert!(!Value::Int(1).structural_eq(&Value::Real(1.0)));
        assert!(!Value::Unit.structural_eq(&Value::Bool(false)));
    }

    #[test]
    fn nan_equals_itself_unlike_ieee() {
        assert!(Value::Real(f64::NAN).structural_eq(&Value::Real(f64::NAN)));
        assert!(Value::Real(0.0).structural_eq(&Value::Real(-0.0)));
    }

    #[test]
    fn adts_compare_tag_and_fields() {
        let a = adt("Some", vec![Value::Int(1)]);
        assert!(a.structural_eq(&adt("Some", vec![Value::Int(1)])));
        assert!(!a.structural_eq(&adt("Some", vec![Value::Int(2)])));
        assert!(!a.structural_eq(&adt("Other", vec![Value::Int(1)])));
        assert!(!a.structural_eq(&adt("Some", vec![])));
    }

    #[test]
    fn cells_compare_by_identity_not_contents() {
        let a = Value::Cell(Rc::new(RefCell::new(Value::Int(1))));
        let alias = a.clone();
        let twin = Value::Cell(Rc::new(RefCell::new(Value::Int(1))));
        assert!(a.structural_eq(&alias));
        assert!(!a.structural_eq(&twin));
    }

    #[test]
    fn functions_are_never_equal_even_to_themselves() {
        let f = native();
        assert!(!f.structural_eq(&f.clone()));
    }

    #[test]
    fn arrays_compare_elementwise() {
        let arr =
            |xs: &[i64]| Value::Array(PVec::from_vec(xs.iter().map(|&i| Value::Int(i)).collect()));
        assert!(arr(&[1, 2]).structural_eq(&arr(&[1, 2])));
        assert!(!arr(&[1, 2]).structural_eq(&arr(&[1, 3])));
        assert!(!arr(&[1, 2]).structural_eq(&arr(&[1])));
    }

    #[test]
    fn maps_compare_keys_and_values_sets_compare_keys_only() {
        let map = |v: i64| Value::Map(PMap::new().insert(Value::Int(1), Value::Int(v)));
        assert!(map(1).structural_eq(&map(1)));
        assert!(!map(1).structural_eq(&map(2)));
        let set = |v: i64| Value::Set(PMap::new().insert(Value::Int(1), Value::Int(v)));
        assert!(set(1).structural_eq(&set(2)));
    }

    #[test]
    fn bytes_compare_by_content() {
        let b = |xs: &[u8]| Value::Bytes(Rc::from(xs));
        assert!(b(&[1, 2]).structural_eq(&b(&[1, 2])));
        assert!(!b(&[1, 2]).structural_eq(&b(&[2, 1])));
    }

    #[test]
    fn debug_names_functions() {
        assert_eq!(format!("{:?}", native()), "Native(<native fn f>)");
    }

    #[test]
    fn dropping_a_long_spine_does_not_overflow() {
        let mut list = adt("Nil", vec![]);
        for i in 0..200_000 {
            list = adt("Cons", vec![Value::Int(i), list]);
        }
        drop(list);
    }
}
