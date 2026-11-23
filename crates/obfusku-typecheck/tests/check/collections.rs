//! Arrays and record fields.

use super::support::*;

#[test]
fn array_literal_infers_int_array_from_homogeneous_elements() {
    let typed = check_ok(vec![binding("xs", array_lit(vec![int(1), int(2), int(3)]))]);
    assert_eq!(
        monomorphic_type(&typed, "xs"),
        CoreType::Adt("Array".to_string(), vec![CoreType::Int])
    );
}

#[test]
fn array_literal_with_mixed_element_types_is_rejected() {
    let ds = check_err(vec![binding("xs", array_lit(vec![int(1), boolean(true)]))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn index_into_an_int_array_produces_int() {
    let typed = check_ok(vec![binding(
        "result",
        index(array_lit(vec![int(1), int(2)]), int(0)),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn index_with_a_non_int_index_is_rejected() {
    let ds = check_err(vec![binding(
        "result",
        index(array_lit(vec![int(1)]), string("nope")),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn indexing_a_non_array_value_is_rejected() {
    let ds = check_err(vec![binding("result", index(int(5), int(0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn empty_array_literal_type_checks_with_an_ambiguous_element_type_error() {
    // No context here at all, so this must be the same "ambiguous"
    // diagnostic any other never-pinned type variable gets.
    let ds = check_err(vec![binding("xs", array_lit(vec![]))]);
    assert!(describe(&ds).contains("ambiguous"), "{}", describe(&ds));
}

#[test]
fn empty_array_literal_element_type_resolves_from_later_context() {
    let typed = check_ok(vec![
        binding("xs", array_lit(vec![])),
        binding("result", index(var("xs"), int(0))),
        binding("pin", binop(BinOp::Add, int(1), var("result"))),
    ]);
    assert_eq!(monomorphic_type(&typed, "pin"), CoreType::Int);
}

#[test]
fn letrec_group_does_not_leak_its_members_past_the_local_body() {
    let ds = check_err(vec![
        binding(
            "result",
            letrec_expr(vec![binding("loop", lambda("n", var("n")))], var("loop")),
        ),
        binding("outside", var("loop")),
    ]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn record_update_rejects_a_mistyped_value_and_an_unknown_field() {
    let wrong_type = module_with(
        vec![point_decl()],
        vec![
            binding("p", point(1, 2)),
            binding("q", update(var("p"), vec![("x", string("s"))])),
        ],
    );
    let ds = check(&wrong_type).expect_err("expected a type error");
    assert!(describe(&ds).contains("field 'x'"), "{}", describe(&ds));

    let unknown = module_with(
        vec![point_decl()],
        vec![
            binding("p", point(1, 2)),
            binding("q", field(var("p"), "z")),
        ],
    );
    let ds = check(&unknown).expect_err("expected a type error");
    assert!(describe(&ds).contains("'z'"), "{}", describe(&ds));
}
