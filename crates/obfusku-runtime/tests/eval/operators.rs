//! The operator table: arithmetic, overflow, ordering and equality.

use super::support::*;

#[test]
fn arithmetic_int() {
    assert_eq!(as_int(run(binop(BinOp::Add, int(3), int(4)))), 7);
    assert_eq!(as_int(run(binop(BinOp::Sub, int(3), int(4)))), -1);
    assert_eq!(as_int(run(binop(BinOp::Mul, int(3), int(4)))), 12);
    assert_eq!(as_int(run(binop(BinOp::Div, int(12), int(4)))), 3);
}

#[test]
fn arithmetic_real() {
    let as_real = |v: Value| match v {
        Value::Real(r) => r,
        other => panic!("expected Real, got {other:?}"),
    };
    assert_eq!(as_real(run(binop(BinOp::Add, real(1.5), real(2.5)))), 4.0);
    assert_eq!(as_real(run(binop(BinOp::Sub, real(1.5), real(2.5)))), -1.0);
    assert_eq!(as_real(run(binop(BinOp::Mul, real(2.0), real(3.0)))), 6.0);
    assert_eq!(as_real(run(binop(BinOp::Div, real(6.0), real(2.0)))), 3.0);
}

#[test]
fn string_concatenation() {
    let v = run(binop(BinOp::Add, str_("foo"), str_("bar")));
    assert!(matches!(v, Value::Str(ref s) if &**s == "foobar"));
}

#[test]
fn int_division_by_zero_raises_a_distinctly_tagged_division_by_zero_exception() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Div, int(1), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
}

#[test]
fn int_modulo_by_zero_also_raises() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Mod, int(7), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
}

// ---------------------------------------------------------------------
// P0-D: `Int` is fixed-width; an unrepresentable arithmetic result
// raises `IntegerOverflow`, deterministically, in every build profile —
// never a debug-only panic, never a release-only silent wrap (the
// previous, accidental behavior of bare Rust operators on `i64`).
// `DivisionByZero` stays a separate condition, confirmed unchanged
// above and again below for `MIN_INT`-specific cases.
// ---------------------------------------------------------------------

fn assert_raises_integer_overflow(expr: Expr) {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad", expr,
        ))],
    };
    let err = evaluate(&module).expect_err("expected an IntegerOverflow raise, not success");
    assert!(err.message.contains("IntegerOverflow"), "{}", err.message);
}

#[test]
fn int_max_plus_one_raises_integer_overflow_not_a_panic() {
    assert_raises_integer_overflow(binop(BinOp::Add, int(i64::MAX), int(1)));
}

#[test]
fn int_min_minus_one_raises_integer_overflow() {
    assert_raises_integer_overflow(binop(BinOp::Sub, int(i64::MIN), int(1)));
}

#[test]
fn int_max_times_two_raises_integer_overflow() {
    assert_raises_integer_overflow(binop(BinOp::Mul, int(i64::MAX), int(2)));
}

#[test]
fn int_min_divided_by_negative_one_raises_integer_overflow_not_division_by_zero() {
    assert_raises_integer_overflow(binop(BinOp::Div, int(i64::MIN), int(-1)));
}

#[test]
fn int_min_modulo_negative_one_raises_integer_overflow() {
    assert_raises_integer_overflow(binop(BinOp::Mod, int(i64::MIN), int(-1)));
}

#[test]
fn negating_int_min_raises_integer_overflow() {
    assert_raises_integer_overflow(unop(UnOp::Neg, int(i64::MIN)));
}

#[test]
fn division_by_zero_is_still_division_by_zero_not_integer_overflow() {
    // Regression: confirms the two conditions were never merged —
    // `IntegerOverflow` only covers `MIN_INT / -1`, not the zero case.
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Div, int(1), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
    assert!(!err.message.contains("IntegerOverflow"), "{}", err.message);
}

#[test]
fn modulo_by_zero_is_still_division_by_zero_not_integer_overflow() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Mod, int(1), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
    assert!(!err.message.contains("IntegerOverflow"), "{}", err.message);
}

#[test]
fn ordinary_in_range_arithmetic_is_completely_unaffected() {
    assert_eq!(
        as_int(run(binop(BinOp::Add, int(i64::MAX - 1), int(1)))),
        i64::MAX
    );
    assert_eq!(
        as_int(run(binop(BinOp::Sub, int(i64::MIN + 1), int(1)))),
        i64::MIN
    );
    assert_eq!(as_int(run(unop(UnOp::Neg, int(i64::MAX)))), -i64::MAX);
}

#[test]
fn real_division_by_zero_follows_ieee_754_never_raises() {
    let pos_inf = run(binop(BinOp::Div, real(1.0), real(0.0)));
    match pos_inf {
        Value::Real(r) => assert!(r.is_infinite() && r > 0.0),
        other => panic!("expected Real(Infinity), got {other:?}"),
    }
    let nan = run(binop(BinOp::Div, real(0.0), real(0.0)));
    match nan {
        Value::Real(r) => assert!(r.is_nan()),
        other => panic!("expected Real(NaN), got {other:?}"),
    }
}

#[test]
fn modulo_is_truncating_sign_of_dividend_matching_the_frozen_worked_examples() {
    // Exactly SEMANTIC_CORE.md §20.2's four frozen examples.
    assert_eq!(as_int(run(binop(BinOp::Mod, int(7), int(3)))), 1);
    assert_eq!(as_int(run(binop(BinOp::Mod, int(-7), int(3)))), -1);
    assert_eq!(as_int(run(binop(BinOp::Mod, int(7), int(-3)))), 1);
    assert_eq!(as_int(run(binop(BinOp::Mod, int(-7), int(-3)))), -1);
}

#[test]
fn comparison_operators() {
    assert!(matches!(
        run(binop(BinOp::Lt, int(1), int(2))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Gt, int(1), int(2))),
        Value::Bool(false)
    ));
    assert!(matches!(
        run(binop(BinOp::Le, int(2), int(2))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Ge, int(1), int(2))),
        Value::Bool(false)
    ));
}

#[test]
fn real_ordering_with_nan_is_always_false_ieee_754_not_total() {
    // §18: ordering and equality are allowed to disagree about NaN.
    let nan = real(f64::NAN);
    assert!(matches!(
        run(binop(BinOp::Lt, nan.clone(), real(1.0))),
        Value::Bool(false)
    ));
    assert!(matches!(
        run(binop(BinOp::Gt, nan, real(1.0))),
        Value::Bool(false)
    ));
}

#[test]
fn equality_is_total_nan_equals_nan_unlike_ordering() {
    let v = run(binop(BinOp::Eq, real(f64::NAN), real(f64::NAN)));
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn equality_and_inequality_on_ordinary_values() {
    assert!(matches!(
        run(binop(BinOp::Eq, int(1), int(1))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::NotEq, int(1), int(2))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Eq, str_("a"), str_("a"))),
        Value::Bool(true)
    ));
}

#[test]
fn equality_on_adt_values_is_structural() {
    let a = ctor("Some", vec![int(5)]);
    let b = ctor("Some", vec![int(5)]);
    assert!(matches!(run(binop(BinOp::Eq, a, b)), Value::Bool(true)));
    let c = ctor("Some", vec![int(6)]);
    let d = ctor("Some", vec![int(5)]);
    assert!(matches!(run(binop(BinOp::Eq, c, d)), Value::Bool(false)));
}

#[test]
fn xor_is_eager_and_bool_only() {
    assert!(matches!(
        run(binop(BinOp::Xor, boolean(true), boolean(false))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Xor, boolean(true), boolean(true))),
        Value::Bool(false)
    ));
}

#[test]
fn unary_not_and_negation() {
    assert!(matches!(
        run(unop(UnOp::Not, boolean(true))),
        Value::Bool(false)
    ));
    assert_eq!(as_int(run(unop(UnOp::Neg, int(5)))), -5);
    let v = run(unop(UnOp::Neg, real(2.5)));
    assert!(matches!(v, Value::Real(r) if r == -2.5));
}

#[test]
fn both_operands_are_always_evaluated_strictly_left_to_right() {
    // No accidental short-circuit in BinaryOp evaluation: both sides of
    // ⊻ must be evaluated even though it has no such shortcut anyway.
    let expr = binop(
        BinOp::Xor,
        boolean(true),
        apply(lambda("x", var("x")), boolean(false)),
    );
    assert!(matches!(run(expr), Value::Bool(true)));
}

// ── LocalBinding: `Expr::Let`/`Expr::LetRec` (`SEMANTIC_CORE.md` §11) ──
