//! Constructor-tag interning.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static TAGS: RefCell<HashMap<String, Rc<str>>> = RefCell::new(HashMap::new());
}

/// One shared `Rc<str>` per distinct tag.
///
/// Matching compares a pattern's tag with a value's tag on every arm; with
/// interning those are usually the same allocation, so the comparison is a
/// pointer check (the evaluator falls back to comparing text when they
/// differ, e.g. for a tag built by a native).
#[must_use]
pub fn intern(tag: &str) -> Rc<str> {
    TAGS.with(|t| {
        let mut t = t.borrow_mut();
        if let Some(rc) = t.get(tag) {
            return Rc::clone(rc);
        }
        let rc: Rc<str> = Rc::from(tag);
        t.insert(tag.to_string(), Rc::clone(&rc));
        rc
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_tags_share_one_allocation() {
        assert!(Rc::ptr_eq(&intern("Cons"), &intern("Cons")));
    }

    #[test]
    fn different_tags_stay_distinct() {
        assert!(!Rc::ptr_eq(&intern("Cons"), &intern("Nil")));
        assert_eq!(&*intern("Nil"), "Nil");
    }
}
