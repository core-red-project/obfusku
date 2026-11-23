//! Match typing, exhaustiveness and unreachable-arm warnings.

use super::support::*;

#[test]
fn match_over_option_binds_the_pattern_variable_at_the_correct_type() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn match_arms_with_incompatible_result_types_are_rejected() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("None", vec![]), string("zero")),
            ],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("match arms have incompatible types"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn wildcard_arm_alone_is_always_exhaustive() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(var("x"), vec![arm(pwild(), int(1))]),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn exhaustive_match_over_closed_adt_succeeds() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    assert!(check(&module_with_adts(vec![option_type_decl()], bindings)).is_ok());
}

#[test]
fn non_exhaustive_match_names_the_missing_case() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![arm(pctor("Some", vec![pvar("n")]), var("n"))],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for None"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn nested_exhaustiveness_succeeds_when_every_position_is_covered() {
    // Option<Option<Int>> — the nested case SEMANTIC_CORE.md §17
    // requires recursing into, not just checking the top level.
    let mut bindings = option_constructor_bindings();
    bindings.push(binding(
        "xx",
        apply(var("Some"), apply(var("Some"), int(5))),
    ));
    bindings.push(binding(
        "y",
        match_expr(
            var("xx"),
            vec![
                arm(
                    pctor("Some", vec![pctor("Some", vec![pvar("n")])]),
                    var("n"),
                ),
                arm(pctor("Some", vec![pctor("None", vec![])]), int(0)),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    assert!(check(&module_with_adts(vec![option_type_decl()], bindings)).is_ok());
}

#[test]
fn nested_exhaustiveness_catches_a_missing_inner_case() {
    // Top level covers Some/None; the INNER position (Some's own field)
    // does not cover None — the recursive check, not the top-level one,
    // must be what fires here.
    let mut bindings = option_constructor_bindings();
    bindings.push(binding(
        "xx",
        apply(var("Some"), apply(var("Some"), int(5))),
    ));
    bindings.push(binding(
        "y",
        match_expr(
            var("xx"),
            vec![
                arm(
                    pctor("Some", vec![pctor("Some", vec![pvar("n")])]),
                    var("n"),
                ),
                arm(pctor("None", vec![]), int(0)),
                // Missing: Some(None) — the inner Option's None case.
            ],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for None"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn bool_exhaustiveness_requires_both_true_and_false() {
    let bindings = vec![binding(
        "y",
        match_expr(boolean(true), vec![arm(plit_bool(true), int(1))]),
    )];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("both 'true' and 'false'"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn bool_exhaustiveness_succeeds_when_both_covered() {
    let bindings = vec![binding(
        "y",
        match_expr(
            boolean(true),
            vec![arm(plit_bool(true), int(1)), arm(plit_bool(false), int(0))],
        ),
    )];
    assert!(check(&module(bindings)).is_ok());
}

#[test]
fn open_type_match_requires_a_wildcard() {
    let bindings = vec![binding(
        "y",
        match_expr(
            int(5),
            vec![arm(plit_int(0), int(0)), arm(plit_int(1), int(1))],
        ),
    )];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("infinitely many values"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn open_type_match_succeeds_with_a_wildcard() {
    let bindings = vec![binding(
        "y",
        match_expr(int(5), vec![arm(plit_int(0), int(0)), arm(pwild(), int(1))]),
    )];
    assert!(check(&module(bindings)).is_ok());
}

#[test]
fn duplicate_tag_arm_produces_an_unreachable_warning() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("Some", vec![pvar("m")]), var("m")),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert!(
        typed
            .warnings
            .iter()
            .any(|w| w.message.contains("unreachable") && w.message.contains("Some")),
        "{:?}",
        typed.warnings
    );
}

#[test]
fn arm_after_a_catchall_produces_an_unreachable_warning() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pvar("anything"), int(1)),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert!(
        typed
            .warnings
            .iter()
            .any(|w| w.message.contains("unreachable")),
        "{:?}",
        typed.warnings
    );
}
