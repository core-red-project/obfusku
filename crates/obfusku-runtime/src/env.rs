//! Lexical environments — parent-chain scoping. A closure captures the
//! [`Env`] active at its own definition site (`spec/SEMANTIC_CORE.md`
//! §13); calling it extends *that* captured chain with one new frame for
//! the parameter, never the caller's chain.

use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

struct Frame {
    vars: RefCell<HashMap<String, Value>>,
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
    pub fn root() -> Env {
        Env(Rc::new(Frame {
            vars: RefCell::new(HashMap::new()),
            parent: None,
        }))
    }

    /// A fresh frame lexically nested under `self` — used both for
    /// ordinary application (one frame per call, binding the parameter)
    /// and for `Match` arm bindings.
    pub fn child(&self) -> Env {
        Env(Rc::new(Frame {
            vars: RefCell::new(HashMap::new()),
            parent: Some(self.clone()),
        }))
    }

    pub fn bind(&self, name: String, value: Value) {
        self.0.vars.borrow_mut().insert(name, value);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.0.vars.borrow().get(name) {
            return Some(v.clone());
        }
        self.0.parent.as_ref().and_then(|p| p.lookup(name))
    }
}
