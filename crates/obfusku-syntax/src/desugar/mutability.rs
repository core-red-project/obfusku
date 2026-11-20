//! The closure-capture rule for mutable names (`SEMANTIC_CORE.md` §13).
//!
//! An ordinary reference to a mutable name lowers to a *read* of its
//! current contents, not the cell, so a closure that rebinds one (`x ⚙︎ …`)
//! would be rebinding a value that is not a cell. The rule: a lambda that
//! rebinds a mutable free variable must capture it explicitly, with `˚`, in
//! that same lambda, so the cell is what it holds.
//!
//! The two analyses below find, in one lambda's body, the names captured with
//! `˚` and the names rebound. Neither enters a *nested* lambda or local
//! function: each of those is checked on its own when lowering reaches it.

use crate::ast::Expression;
use obfusku_diagnostics::Span;
use std::collections::HashSet;

/// Names appearing as `˚name` (or `˚name ⚙︎ …`) in `expr`.
pub(super) fn explicit_captures(expr: &Expression, out: &mut HashSet<String>) {
    match expr {
        Expression::ExplicitRef(name, _) => {
            out.insert(name.clone());
        }
        Expression::Rebind {
            name,
            explicit: true,
            ..
        } => {
            out.insert(name.clone());
        }
        // Its own captures are checked independently.
        Expression::Lambda { .. } => return,
        // Each declaration's body gets its own check; only the scope is ours.
        Expression::LocalFunctions { body, .. } => return explicit_captures(body, out),
        _ => {}
    }
    for child in expr.children() {
        explicit_captures(child, out);
    }
}

/// `(name, span)` of every rebind target in `expr`.
pub(super) fn rebind_targets(expr: &Expression, out: &mut Vec<(String, Span)>) {
    match expr {
        Expression::Rebind {
            name, name_span, ..
        } => out.push((name.clone(), *name_span)),
        Expression::Lambda { .. } => return,
        Expression::LocalFunctions { body, .. } => return rebind_targets(body, out),
        _ => {}
    }
    for child in expr.children() {
        rebind_targets(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse;
    use obfusku_diagnostics::SourceId;

    fn body(source: &str) -> Expression {
        let src = format!("v ≔ {source}\n❧");
        let tokens = tokenize(&src, SourceId(0)).unwrap();
        match parse(&tokens, SourceId(0)).unwrap().declarations.remove(0) {
            crate::ast::Declaration::Value(v) => v.value,
            other => panic!("expected a value, got {other:?}"),
        }
    }

    fn captures(source: &str) -> Vec<String> {
        let mut set = HashSet::new();
        explicit_captures(&body(source), &mut set);
        let mut names: Vec<_> = set.into_iter().collect();
        names.sort();
        names
    }

    fn targets(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        rebind_targets(&body(source), &mut out);
        out.into_iter().map(|(n, _)| n).collect()
    }

    #[test]
    fn explicit_references_and_explicit_rebinds_are_captures() {
        assert_eq!(captures("f((˚a), (˚b ⚙︎ 1))"), ["a", "b"]);
    }

    #[test]
    fn an_implicit_rebind_is_a_target_but_not_a_capture() {
        assert_eq!(captures("a ⚙︎ 1"), Vec::<String>::new());
        assert_eq!(targets("a ⚙︎ 1"), ["a"]);
    }

    #[test]
    fn nested_lambdas_are_not_entered() {
        assert_eq!(captures("f(λ(x) → (˚a))"), Vec::<String>::new());
        assert_eq!(targets("f(λ(x) → a ⚙︎ 1)"), Vec::<String>::new());
    }

    #[test]
    fn local_function_bodies_are_not_entered_but_their_scope_is() {
        let source = "λg(n: ⟁): ⟁ → (˚a) ✚ 1 ⟡ (˚b) { _ → 0 }";
        // The declaration body is skipped; the trailing scope `(˚b)` is not.
        assert_eq!(captures(source), ["b"]);
    }

    #[test]
    fn captures_are_found_through_operators_matches_and_records() {
        assert_eq!(captures("(˚a) ✚ ⟡ (˚b) { _ → (˚c) }"), ["a", "b", "c"]);
        assert_eq!(captures("P { x: (˚a) }"), ["a"]);
    }

    #[test]
    fn a_rebind_value_is_searched_too() {
        assert_eq!(targets("a ⚙︎ (b ⚙︎ 1)"), ["a", "b"]);
    }
}
