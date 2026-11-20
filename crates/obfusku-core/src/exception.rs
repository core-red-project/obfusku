//! The exceptions every program has (`SEMANTIC_CORE.md` §15.2).
//!
//! `Exception` is an *open* type: programs add variants with `☈`
//! (`ADR-026`). These five exist before any program is read, because the
//! runtime itself raises them (overflow, division, a failed match, a runtime
//! error) and the checker must know their shape to type a `☊` handler.
//!
//! The table is here, in the core, so the three stages that depend on it
//! (the desugarer synthesizes their constructors, the checker registers
//! their payload types, the runtime numbers them for the generated order)
//! read one definition and cannot drift apart.

/// `(tag, arity)` of each built-in `Exception` variant, in declaration order.
///
/// The order is observable: it is the generated ordering of the type
/// (`ADR-025`), so an exception sorts by its position here.
pub const BUILTIN_EXCEPTIONS: [(&str, usize); 5] = [
    ("DivisionByZero", 0),
    ("NonExhaustiveMatch", 0),
    ("InvalidOperation", 1),
    ("Failure", 2),
    ("IntegerOverflow", 0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_unique() {
        let mut tags: Vec<_> = BUILTIN_EXCEPTIONS.iter().map(|(t, _)| *t).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), BUILTIN_EXCEPTIONS.len());
    }

    #[test]
    fn the_runtime_raised_errors_are_present_with_their_payloads() {
        let arity = |tag| {
            BUILTIN_EXCEPTIONS
                .iter()
                .find(|(t, _)| *t == tag)
                .map(|(_, a)| *a)
        };
        assert_eq!(arity("DivisionByZero"), Some(0));
        assert_eq!(arity("IntegerOverflow"), Some(0));
        assert_eq!(arity("InvalidOperation"), Some(1));
    }
}
