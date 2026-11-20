//! Storage for top-level names.

use crate::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A top-level (or prelude) name's storage.
///
/// A cell, not a plain value, so a recursive group can be compiled before
/// any of its members has run: a closure compiled early holds the cell, and
/// sees the value once its binding evaluates (`SEMANTIC_CORE.md` §11).
#[derive(Debug)]
pub struct GlobalCell {
    /// The name, for the "unbound variable" error if it is read too early.
    pub name: Rc<str>,
    /// `None` until the binding has been evaluated.
    pub value: RefCell<Option<Value>>,
}

impl GlobalCell {
    /// A shared cell for `name`, filled or not yet.
    #[must_use]
    pub fn new(name: &str, value: Option<Value>) -> Rc<GlobalCell> {
        Rc::new(GlobalCell {
            name: Rc::from(name),
            value: RefCell::new(value),
        })
    }
}

/// The top-level names visible to the code being compiled.
///
/// Later bindings of the same name shadow earlier ones; a closure compiled
/// against the earlier cell keeps resolving to it.
pub type Globals = HashMap<String, Rc<GlobalCell>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_cell_holds_what_it_was_given() {
        assert!(GlobalCell::new("x", None).value.borrow().is_none());
        let filled = GlobalCell::new("x", Some(Value::Int(1)));
        assert!(matches!(&*filled.value.borrow(), Some(Value::Int(1))));
        assert_eq!(&*filled.name, "x");
    }
}
