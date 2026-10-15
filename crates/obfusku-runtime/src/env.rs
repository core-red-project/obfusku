//! Evaluation frames — a parent chain of slot vectors. A closure
//! captures the [`Env`] active at its definition site
//! (`spec/SEMANTIC_CORE.md` §13); applying it adds one frame for the
//! parameter on that captured chain, never on the caller's.

use crate::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

enum Slots {
    One(Value),
    Many(Vec<Value>),
}

struct Frame {
    slots: RefCell<Slots>,
    parent: Option<Env>,
}

/// Cheaply cloneable handle to one frame in a lexical chain.
#[derive(Clone)]
pub struct Env(Rc<Frame>);

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

    #[must_use]
    pub fn push_one(&self, v: Value) -> Env {
        Env(Rc::new(Frame {
            slots: RefCell::new(Slots::One(v)),
            parent: Some(self.clone()),
        }))
    }

    #[must_use]
    pub fn push_many(&self, vs: Vec<Value>) -> Env {
        Env(Rc::new(Frame {
            slots: RefCell::new(Slots::Many(vs)),
            parent: Some(self.clone()),
        }))
    }

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

    /// Fills slot `slot` of this (innermost) frame — a letrec group
    /// filling its own frame as each member evaluates.
    pub fn set(&self, slot: u32, v: Value) {
        match &mut *self.0.slots.borrow_mut() {
            Slots::One(x) => *x = v,
            Slots::Many(vs) => vs[slot as usize] = v,
        }
    }
}
