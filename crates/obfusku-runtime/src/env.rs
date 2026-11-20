//! Evaluation environments: [`Env`].

use crate::value::Value;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// One frame's slots. A single-binder frame (a lambda parameter, a `let`, a
/// one-variable match arm) is by far the most common, so it avoids the `Vec`.
enum Slots {
    One(Value),
    Many(Vec<Value>),
}

struct Frame {
    slots: RefCell<Slots>,
    parent: Option<Env>,
}

/// A lexical environment: a chain of frames, innermost first
/// (`SEMANTIC_CORE.md` §13).
///
/// **Why indices, not names.** The compiler (`ir`) resolves every local
/// variable to `(depth, slot)` once, so a lookup walks `depth` parents and
/// indexes a slot: no hashing, no string comparison, on the hottest path of
/// the evaluator.
///
/// A closure captures the `Env` active where its `λ` was evaluated; applying
/// it pushes one frame for the parameter onto *that* chain, never onto the
/// caller's. That is lexical scoping.
///
/// Cloning is a reference-count bump, so capturing is cheap.
#[derive(Clone)]
pub struct Env(Rc<Frame>);

impl fmt::Debug for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Env(..)")
    }
}

impl Env {
    /// The empty chain a top-level closure captures.
    #[must_use]
    pub fn root() -> Env {
        Env(Rc::new(Frame {
            slots: RefCell::new(Slots::Many(Vec::new())),
            parent: None,
        }))
    }

    /// A child frame holding one value.
    #[must_use]
    pub fn push_one(&self, v: Value) -> Env {
        Env(Rc::new(Frame {
            slots: RefCell::new(Slots::One(v)),
            parent: Some(self.clone()),
        }))
    }

    /// A child frame holding several values, slot `i` for the `i`-th.
    #[must_use]
    pub fn push_many(&self, vs: Vec<Value>) -> Env {
        Env(Rc::new(Frame {
            slots: RefCell::new(Slots::Many(vs)),
            parent: Some(self.clone()),
        }))
    }

    /// The value in `slot` of the frame `depth` parents up.
    ///
    /// The compiler only emits `(depth, slot)` pairs it resolved against its
    /// own scopes, so both are in range; a violation is a compiler bug, not a
    /// user error, and panics.
    #[must_use]
    pub fn get(&self, depth: u32, slot: u32) -> Value {
        let mut frame = &self.0;
        for _ in 0..depth {
            frame = &frame.parent.as_ref().expect("resolved depth").0;
        }
        match &*frame.slots.borrow() {
            Slots::One(v) => v.clone(),
            Slots::Many(vs) => vs[slot as usize].clone(),
        }
    }

    /// Fills `slot` of this (innermost) frame: a `letrec` group fills its own
    /// frame as each member evaluates, after closures that reference the
    /// frame have already captured it.
    pub fn set(&self, slot: u32, v: Value) {
        match &mut *self.0.slots.borrow_mut() {
            Slots::One(x) => *x = v,
            Slots::Many(vs) => vs[slot as usize] = v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(env: &Env, depth: u32, slot: u32) -> i64 {
        match env.get(depth, slot) {
            Value::Int(i) => i,
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn a_pushed_value_is_found_at_depth_zero() {
        let env = Env::root().push_one(Value::Int(7));
        assert_eq!(int(&env, 0, 0), 7);
    }

    #[test]
    fn outer_frames_are_reached_by_depth() {
        let env = Env::root().push_one(Value::Int(1)).push_one(Value::Int(2));
        assert_eq!((int(&env, 0, 0), int(&env, 1, 0)), (2, 1));
    }

    #[test]
    fn a_many_frame_is_indexed_by_slot() {
        let env = Env::root().push_many(vec![Value::Int(10), Value::Int(20)]);
        assert_eq!((int(&env, 0, 0), int(&env, 0, 1)), (10, 20));
    }

    #[test]
    fn set_fills_a_slot_visible_through_every_handle() {
        let group = Env::root().push_many(vec![Value::Unit, Value::Unit]);
        let captured = group.clone();
        group.set(1, Value::Int(5));
        assert_eq!(int(&captured, 0, 1), 5);
    }

    #[test]
    fn set_on_a_one_frame_replaces_its_value() {
        let env = Env::root().push_one(Value::Int(1));
        env.set(0, Value::Int(2));
        assert_eq!(int(&env, 0, 0), 2);
    }

    #[test]
    fn pushing_does_not_disturb_the_parent() {
        let parent = Env::root().push_one(Value::Int(1));
        let _child = parent.push_one(Value::Int(2));
        assert_eq!(int(&parent, 0, 0), 1);
    }

    #[test]
    #[should_panic(expected = "resolved depth")]
    fn an_unresolved_depth_is_a_compiler_bug() {
        let _ = Env::root().get(1, 0);
    }
}
