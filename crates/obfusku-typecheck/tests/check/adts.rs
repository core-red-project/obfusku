//! Constructors, patterns over declared types, `Optional` and `Result`.

use super::support::*;

#[test]
fn constructor_is_polymorphic_and_reusable_at_different_types() {
    // Some : ∀t. t → Option<t> — each application instantiates fresh,
    // exactly like `id`.
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("a", apply(var("Some"), int(5))));
    bindings.push(binding("b", apply(var("Some"), string("hi"))));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert_eq!(
        monomorphic_type(&typed, "a"),
        CoreType::Adt("Option".into(), vec![CoreType::Int])
    );
    assert_eq!(
        monomorphic_type(&typed, "b"),
        CoreType::Adt("Option".into(), vec![CoreType::Str])
    );
}

#[test]
fn nullary_constructor_has_the_adt_type_directly_no_function_wrapper() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("n", var("None")));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    // None : ∀t. Option<t> — a polymorphic *value*, not a function.
    match &typed.bindings.iter().find(|b| b.name == "n").unwrap().ty {
        TypeResult::Polymorphic(scheme) => {
            assert!(!scheme.vars.is_empty());
            assert!(matches!(scheme.ty, obfusku_typecheck::Type::Adt(ref n, _) if n == "Option"));
        }
        other => panic!("expected 'n' to be polymorphic (via None), got {other:?}"),
    }
}

#[test]
fn constructor_field_type_mismatch_is_rejected() {
    let intbox = type_decl("IntBox", vec![], vec![variant("Box", vec![CoreType::Int])]);
    let box_ctor = Binding {
        name: "Box".to_string(),
        value: Expr::Lambda {
            param: "$field0".to_string(),
            param_ty: None,
            body: Rc::new(Expr::Constructor {
                tag: "Box".to_string(),
                args: vec![var("$field0")],
                span: sp(),
            }),
            span: sp(),
        },
        exported: true,
        span: sp(),
        declared_type: None,
    };
    // Caught at the ordinary Apply boundary: Box's type is already the
    // concrete `Int → IntBox`, so a String argument fails like any
    // wrongly-typed call.
    let ds = check_err_adt(
        vec![intbox],
        vec![box_ctor, binding("bad", apply(var("Box"), string("nope")))],
    );
    assert!(
        describe(&ds).contains("argument type mismatch: expected Int, found String"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn constructor_node_itself_rejects_a_mismatched_field_type() {
    // The Constructor-typing code path directly, bypassing the Lambda/
    // Apply layer entirely — this is what a malformed constructor body
    // would hit (the Lambda/Apply check above can never reach this,
    // since it always catches the mismatch first for an ordinary call).
    let intbox = type_decl("IntBox", vec![], vec![variant("Box", vec![CoreType::Int])]);
    let bindings = vec![binding(
        "bad",
        Expr::Constructor {
            tag: "Box".to_string(),
            args: vec![string("nope")],
            span: sp(),
        },
    )];
    let ds = check_err_adt(vec![intbox], bindings);
    assert!(
        describe(&ds).contains("field type mismatch: expected Int, found String"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn unknown_constructor_in_pattern_is_rejected() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(var("x"), vec![arm(pctor("Bogus", vec![]), int(0))]),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("unknown constructor 'Bogus'"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn wrong_arity_constructor_pattern_is_rejected() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("a"), pvar("b")]), int(0)),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("expects 1 argument(s), found 2"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn constructor_used_as_a_syntactic_value_is_recursively_checked() {
    // Some(5) is Apply-shaped at the point of use — an Apply-shaped
    // binding is never generalized regardless of what's inside (§19.1).
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("a", apply(var("Some"), int(5))));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    match &typed.bindings.iter().find(|b| b.name == "a").unwrap().ty {
        TypeResult::Monomorphic(_) => {}
        other => panic!("expected 'a' to be monomorphic (Apply-shaped), got {other:?}"),
    }
}

#[test]
fn optional_under_its_real_name_behaves_identically_to_the_earlier_option_fixture() {
    // Deliberately a *different* tag ("Optional", not "Option") from the
    // fixture used throughout the ADT slice — proving the mechanism
    // isn't accidentally keyed to that one specific name.
    let optional = type_decl(
        "Optional",
        vec!["t"],
        vec![
            variant("Some", vec![CoreType::Param("t".to_string())]),
            variant("None", vec![]),
        ],
    );
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("a", apply(var("Some"), int(5))));
    let typed = check_ok_adt(vec![optional], bindings);
    assert_eq!(
        monomorphic_type(&typed, "a"),
        CoreType::Adt("Optional".into(), vec![CoreType::Int])
    );
}

#[test]
fn ok_and_err_construct_and_independently_instantiate_their_type_parameter() {
    // Ok(5)/Err("boom") are Apply-shaped, so each is monomorphic with
    // its other parameter free until pinned by an actual later use.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("success", apply(var("Ok"), int(5))));
    bindings.push(binding("failure", apply(var("Err"), string("boom"))));
    // Pin success's E to String, and failure's T to Int, via ordinary
    // Match usage — independently, in different arms, of each other.
    bindings.push(binding(
        "pin_success",
        match_expr(
            var("success"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    bindings.push(binding(
        "pin_failure",
        match_expr(
            var("failure"),
            vec![
                arm(pctor("Ok", vec![pvar("t")]), var("t")),
                arm(pctor("Err", vec![pwild()]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(
        monomorphic_type(&typed, "success"),
        CoreType::Adt("Result".into(), vec![CoreType::Int, CoreType::Str])
    );
    assert_eq!(
        monomorphic_type(&typed, "failure"),
        CoreType::Adt("Result".into(), vec![CoreType::Int, CoreType::Str])
    );
}

#[test]
fn result_int_string_infers_both_parameters_together_when_both_are_used() {
    // A function using a Result at both arms simultaneously pins both T
    // and E to concrete types from ordinary unification — no
    // Result-specific inference rule involved.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding(
        "describe",
        lambda(
            "r",
            match_expr(
                var("r"),
                vec![
                    arm(pctor("Ok", vec![pvar("n")]), var("n")),
                    arm(pctor("Err", vec![pvar("msg")]), int(0)),
                ],
            ),
        ),
    ));
    bindings.push(binding(
        "y",
        apply(var("describe"), apply(var("Ok"), int(5))),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn nested_result_type_checks_and_exhausts_recursively() {
    // Result<Result<Int, Int>, Int> — nested exhaustiveness across both
    // the outer and inner Ok/Err at once.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("inner_ok", apply(var("Ok"), int(5))));
    bindings.push(binding("outer", apply(var("Ok"), var("inner_ok"))));
    bindings.push(binding(
        "y",
        match_expr(
            var("outer"),
            vec![
                arm(pctor("Ok", vec![pctor("Ok", vec![pvar("n")])]), var("n")),
                arm(
                    pctor("Ok", vec![pctor("Err", vec![pvar("msg")])]),
                    var("msg"),
                ),
                arm(pctor("Err", vec![pvar("b")]), var("b")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn nested_result_exhaustiveness_catches_a_missing_inner_case() {
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("inner_ok", apply(var("Ok"), int(5))));
    bindings.push(binding("outer", apply(var("Ok"), var("inner_ok"))));
    bindings.push(binding(
        "y",
        match_expr(
            var("outer"),
            vec![
                arm(pctor("Ok", vec![pctor("Ok", vec![pvar("n")])]), var("n")),
                // Missing: Ok(Err(_)) — the inner Result's Err case.
                arm(pctor("Err", vec![pvar("b")]), int(0)),
            ],
        ),
    ));
    let ds = check_err_adt(vec![result_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for Err"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn incomplete_result_match_names_the_missing_case() {
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("r", apply(var("Ok"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(var("r"), vec![arm(pctor("Ok", vec![pvar("n")]), var("n"))]),
    ));
    let ds = check_err_adt(vec![result_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for Err"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn result_payload_type_mismatch_is_rejected() {
    // A fixed (non-generic) context forcing Result<Int, String> — Err's
    // payload here is an Int, not the required String.
    let pair = type_decl(
        "Pair",
        vec![],
        vec![variant(
            "MkPair",
            vec![CoreType::Adt(
                "Result".to_string(),
                vec![CoreType::Int, CoreType::Str],
            )],
        )],
    );
    let mkpair = unary_constructor_binding("MkPair");
    let mut bindings = result_constructor_bindings();
    bindings.push(mkpair);
    bindings.push(binding(
        "bad",
        apply(var("MkPair"), apply(var("Err"), int(42))),
    ));
    let ds = check_err_adt(vec![result_type_decl(), pair], bindings);
    assert!(
        describe(&ds).contains("expected String, found Int")
            || describe(&ds).contains("expected Result<Int, String>"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn result_composed_with_optional_both_directions_type_check() {
    // Two independently-declared ADTs composing with no special glue.
    let mut bindings = option_constructor_bindings();
    bindings.extend(result_constructor_bindings());
    bindings.push(binding(
        "a", // Result<Optional<Int>, ?>
        apply(var("Ok"), apply(var("Some"), int(5))),
    ));
    bindings.push(binding(
        "pin_a",
        match_expr(
            var("a"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    bindings.push(binding(
        "b", // Optional<Result<Int, ?>>
        apply(var("Some"), apply(var("Ok"), int(5))),
    ));
    bindings.push(binding(
        "pin_b",
        match_expr(
            var("b"),
            vec![
                arm(pctor("Some", vec![pctor("Ok", vec![pwild()])]), string("")),
                arm(pctor("Some", vec![pctor("Err", vec![pvar("e")])]), var("e")),
                arm(pctor("None", vec![]), string("")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl(), result_type_decl()], bindings);
    match &typed.bindings.iter().find(|b| b.name == "a").unwrap().ty {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Result");
            assert!(matches!(&args[0], CoreType::Adt(n, _) if n == "Option"));
            assert_eq!(args[1], CoreType::Str);
        }
        other => panic!("unexpected type for 'a': {other:?}"),
    }
    match &typed.bindings.iter().find(|b| b.name == "b").unwrap().ty {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Option");
            assert!(matches!(&args[0], CoreType::Adt(n, _) if n == "Result"));
        }
        other => panic!("unexpected type for 'b': {other:?}"),
    }
}

#[test]
fn result_constructor_binding_generalizes_like_any_other_lambda() {
    // `Ok` itself generalizes fully (∀t,e. t → Result<t,e>), reusable
    // like `id`; each use is Apply-shaped and individually monomorphic.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("a", apply(var("Ok"), int(5))));
    bindings.push(binding("b", apply(var("Ok"), string("hi"))));
    bindings.push(binding(
        "pin_a",
        match_expr(
            var("a"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    bindings.push(binding(
        "pin_b",
        match_expr(
            var("b"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(
        monomorphic_type(&typed, "a"),
        CoreType::Adt("Result".into(), vec![CoreType::Int, CoreType::Str])
    );
    assert_eq!(
        monomorphic_type(&typed, "b"),
        CoreType::Adt("Result".into(), vec![CoreType::Str, CoreType::Str])
    );
}

#[test]
fn apply_shaped_result_expression_does_not_generalize() {
    // Mirrors `wrapped` exactly, with Result substituted for the
    // identity-function example — no special-casing needed to make the
    // value restriction hold here too. Asserted directly against the
    // TypeResult representation, not indirectly via a second call site.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding(
        "wrapped",
        apply(lambda("x", apply(var("Ok"), var("x"))), int(5)),
    ));
    bindings.push(binding(
        "pin",
        match_expr(
            var("wrapped"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    match &typed
        .bindings
        .iter()
        .find(|b| b.name == "wrapped")
        .unwrap()
        .ty
    {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Result");
            assert_eq!(args[0], CoreType::Int);
            assert_eq!(args[1], CoreType::Str);
        }
        other => {
            panic!("expected 'wrapped' to be a monomorphic Result<Int, String>, got {other:?}")
        }
    }
}

#[test]
fn err_value_has_no_special_type_representation_distinct_from_any_other_adt() {
    // Comes back through the exact same
    // `TypeResult::Monomorphic(CoreType::Adt(...))` shape any ordinary
    // ADT value uses — no separate "exceptional" type representation.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("e", apply(var("Err"), string("boom"))));
    bindings.push(binding(
        "pin",
        match_expr(
            var("e"),
            vec![
                arm(pctor("Ok", vec![pvar("t")]), var("t")),
                arm(pctor("Err", vec![pwild()]), string("")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    match &typed.bindings.iter().find(|b| b.name == "e").unwrap().ty {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Result");
            assert_eq!(args[1], CoreType::Str);
        }
        other => panic!("expected an ordinary monomorphic Adt type, got {other:?}"),
    }
}

#[test]
fn result_value_is_not_implicitly_convertible_to_its_payload_type() {
    // No auto-unwrap: a context requiring a concrete Int rejects a
    // Result<Int, String> outright.
    let intbox = type_decl("IntBox", vec![], vec![variant("Box", vec![CoreType::Int])]);
    let mut bindings = result_constructor_bindings();
    bindings.push(unary_constructor_binding("Box"));
    bindings.push(binding("bad", apply(var("Box"), apply(var("Ok"), int(5)))));
    let ds = check_err_adt(vec![result_type_decl(), intbox], bindings);
    assert!(
        describe(&ds).contains("argument type mismatch: expected Int, found Result"),
        "{}",
        describe(&ds)
    );
}
