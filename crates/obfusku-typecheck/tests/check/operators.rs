//! The closed operator table (`SEMANTIC_CORE.md` §20.2).

use super::support::*;

#[test]
fn int_plus_int_is_int() {
    let typed = check_ok(vec![binding("result", binop(BinOp::Add, int(1), int(2)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn int_plus_real_is_rejected_no_coercion() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Add, int(1), real(2.0)),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn lambda_equality_is_a_static_type_error_not_runtime() {
    // λ(x) → x == λ(x) → x — comparing two function values.
    let f = lambda("x", var("x"));
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Eq, f.clone(), lambda("y", var("y"))),
    )]);
    assert!(describe(&ds).contains("no equality"), "{}", describe(&ds));
}

#[test]
fn and_is_already_match_by_the_time_typecheck_sees_it_no_special_short_circuit_rule() {
    // `∧` is already Core `Match` by the time this reaches typecheck —
    // ordinary Match typing handles it, nothing operator-specific.
    let expensive = apply(var("expensive"), int(1));
    let core_and = Expr::Match {
        scrutinee: Box::new(boolean(true)),
        arms: vec![
            MatchArm {
                pattern: Pattern::Lit(Literal::Bool(true), sp()),
                result: Rc::new(expensive),
                span: sp(),
            },
            MatchArm {
                pattern: Pattern::Lit(Literal::Bool(false), sp()),
                result: Rc::new(boolean(false)),
                span: sp(),
            },
        ],
        span: sp(),
    };
    let typed = check_ok(vec![
        binding("expensive", lambda("ignored", boolean(true))),
        binding("result", core_and),
    ]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);
}

#[test]
fn add_real_real_is_real() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Add, real(1.0), real(2.0)),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Real);
}

#[test]
fn add_str_str_is_str_concatenation() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Add, string("a"), string("b")),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Str);
}

#[test]
fn add_bool_bool_is_rejected_bool_not_in_the_closed_set() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Add, boolean(true), boolean(false)),
    )]);
    assert!(
        describe(&ds).contains("closed operator table"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn sub_mul_div_accept_int_or_real_symmetrically() {
    for op in [BinOp::Sub, BinOp::Mul, BinOp::Div] {
        let typed_int = check_ok(vec![binding("result", binop(op, int(4), int(2)))]);
        assert_eq!(monomorphic_type(&typed_int, "result"), CoreType::Int);
        let typed_real = check_ok(vec![binding("result", binop(op, real(4.0), real(2.0)))]);
        assert_eq!(monomorphic_type(&typed_real, "result"), CoreType::Real);
    }
}

#[test]
fn sub_mixed_int_real_is_rejected() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Sub, int(4), real(2.0)),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn sub_on_str_is_rejected_str_only_valid_for_add() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Sub, string("a"), string("b")),
    )]);
    assert!(
        describe(&ds).contains("closed operator table"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn modulo_is_int_only() {
    let typed = check_ok(vec![binding("result", binop(BinOp::Mod, int(7), int(3)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);

    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Mod, real(7.0), real(3.0)),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn division_by_the_literal_zero_still_type_checks_fine() {
    // The specific "typecheck does not evaluate constants" property:
    // 1 ÷ 0 is perfectly well-typed. DivisionByZero belongs to runtime.
    let typed = check_ok(vec![binding("result", binop(BinOp::Div, int(1), int(0)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn comparison_operators_accept_int_or_real_and_produce_bool() {
    for op in [BinOp::Lt, BinOp::Gt, BinOp::Le, BinOp::Ge] {
        let typed = check_ok(vec![binding("result", binop(op, int(1), int(2)))]);
        assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);
    }
}

#[test]
fn comparison_rejects_mixed_int_real() {
    let ds = check_err(vec![binding("result", binop(BinOp::Lt, int(1), real(2.0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn comparison_orders_strings_and_rejects_functions() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Lt, string("a"), string("b")),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);

    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Lt, lambda("x", var("x")), lambda("y", var("y"))),
    )]);
    assert!(describe(&ds).contains("ordering"), "{}", describe(&ds));
}

#[test]
fn equality_works_for_any_single_matching_equatable_type() {
    let typed_int = check_ok(vec![binding("result", binop(BinOp::Eq, int(1), int(1)))]);
    assert_eq!(monomorphic_type(&typed_int, "result"), CoreType::Bool);

    let typed_str = check_ok(vec![binding(
        "result",
        binop(BinOp::Eq, string("a"), string("a")),
    )]);
    assert_eq!(monomorphic_type(&typed_str, "result"), CoreType::Bool);

    let typed_bool = check_ok(vec![binding(
        "result",
        binop(BinOp::NotEq, boolean(true), boolean(false)),
    )]);
    assert_eq!(monomorphic_type(&typed_bool, "result"), CoreType::Bool);
}

#[test]
fn equality_rejects_cross_type_comparison_no_coercion() {
    let ds = check_err(vec![binding("result", binop(BinOp::Eq, int(1), real(1.0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn xor_is_bool_only() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Xor, boolean(true), boolean(false)),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);

    let ds = check_err(vec![binding("result", binop(BinOp::Xor, int(1), int(0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn unary_not_is_bool_only() {
    let typed = check_ok(vec![binding("result", unop(UnOp::Not, boolean(true)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);

    let ds = check_err(vec![binding("result", unop(UnOp::Not, int(1)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn unary_negation_accepts_int_or_real_preserving_the_operand_type() {
    let typed_int = check_ok(vec![binding("result", unop(UnOp::Neg, int(5)))]);
    assert_eq!(monomorphic_type(&typed_int, "result"), CoreType::Int);

    let typed_real = check_ok(vec![binding("result", unop(UnOp::Neg, real(5.0)))]);
    assert_eq!(monomorphic_type(&typed_real, "result"), CoreType::Real);

    let ds = check_err(vec![binding("result", unop(UnOp::Neg, boolean(true)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn operator_type_error_carries_the_source_span_from_the_offending_expression() {
    let bad = binop(BinOp::Add, int(1), boolean(true));
    let ds = check_err(vec![binding("result", bad)]);
    assert_eq!(ds[0].primary, sp());
}
