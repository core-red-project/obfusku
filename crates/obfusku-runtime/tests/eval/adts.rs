//! Constructors, pattern matching and structural equality of ADTs.

use super::support::*;

#[test]
fn nullary_constructor_matches_by_tag() {
    let expr = match_(
        ctor("None", vec![]),
        vec![(pctor("None", vec![]), int(0)), (pwild(), int(1))],
    );
    assert_eq!(as_int(run(expr)), 0);
}

#[test]
fn payload_bearing_constructor_binds_its_field() {
    let expr = match_(
        ctor("Some", vec![int(7)]),
        vec![
            (pctor("Some", vec![pvar("x")]), var("x")),
            (pwild(), int(-1)),
        ],
    );
    assert_eq!(as_int(run(expr)), 7);
}

#[test]
fn nested_constructors_destructure_recursively() {
    // Some(Some(9)) matched against Some(Some(x))
    let expr = match_(
        ctor("Some", vec![ctor("Some", vec![int(9)])]),
        vec![
            (
                pctor("Some", vec![pctor("Some", vec![pvar("x")])]),
                var("x"),
            ),
            (pwild(), int(-1)),
        ],
    );
    assert_eq!(as_int(run(expr)), 9);
}

#[test]
fn exhaustive_match_over_two_variants_picks_the_matching_arm() {
    let ok_case = match_(
        ctor("Ok", vec![int(1)]),
        vec![
            (pctor("Ok", vec![pvar("v")]), var("v")),
            (pctor("Err", vec![pvar("_e")]), int(-1)),
        ],
    );
    assert_eq!(as_int(run(ok_case)), 1);

    let err_case = match_(
        ctor("Err", vec![int(1)]),
        vec![
            (pctor("Ok", vec![pvar("v")]), var("v")),
            (pctor("Err", vec![pvar("_e")]), int(-1)),
        ],
    );
    assert_eq!(as_int(run(err_case)), -1);
}

#[test]
fn constructor_equality_is_structural_not_identity() {
    // Two independently-built Some(5) values compare equal via a match
    // against a literal-shaped arm — proven indirectly since Value has
    // no public equality operator surfaced to Core yet; instead prove it
    // via structural_eq directly.
    use obfusku_runtime::Value as V;
    let a = V::Adt {
        tag: std::rc::Rc::from("Some"),
        args: std::rc::Rc::from(vec![V::Int(5)]),
    };
    let b = V::Adt {
        tag: std::rc::Rc::from("Some"),
        args: std::rc::Rc::from(vec![V::Int(5)]),
    };
    assert!(a.structural_eq(&b));
    let c = V::Adt {
        tag: std::rc::Rc::from("Some"),
        args: std::rc::Rc::from(vec![V::Int(6)]),
    };
    assert!(!a.structural_eq(&c));
}

#[test]
fn no_matching_arm_at_runtime_is_reported_as_an_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: match_(ctor("Foo", vec![]), vec![(pctor("Bar", vec![]), int(1))]),
                is_exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

// ---------------------------------------------------------------------
// Recursion / tail-call optimization
// ---------------------------------------------------------------------
