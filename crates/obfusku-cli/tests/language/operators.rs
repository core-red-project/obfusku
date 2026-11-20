//! Operators and exceptions: arithmetic and primitive operators end to end, and `Raise`/`Catch`.

use crate::support::{as_int, run_err, run_ok};
use obfusku_runtime::Value;

#[test]
fn arithmetic_precedence_end_to_end() {
    // 1 ✚ 2 ✱ 3 == 7, not 9 — multiplicative binds tighter.
    let v = run_ok("result ≔ 1 ✚ 2 ✱ 3\n❧");
    assert_eq!(as_int(v), 7);
}

#[test]
fn unary_negation_and_not_end_to_end() {
    let v = run_ok("result ≔ −3 ✱ 2\n❧");
    assert_eq!(as_int(v), -6);
    let v2 = run_ok("result ≔ ¬◎\n❧");
    assert!(matches!(v2, Value::Bool(true)));
}

#[test]
fn int_and_real_arithmetic_end_to_end() {
    let v = run_ok("result ≔ (10 ☠︎ 4) ÷ 2\n❧");
    assert_eq!(as_int(v), 3);
    let v2 = run_ok("result ≔ 1.5 ✚ 2.5\n❧");
    assert!(matches!(v2, Value::Real(r) if r == 4.0));
}

#[test]
fn string_concatenation_end_to_end() {
    let v = run_ok("result ≔ \"foo\" ✚ \"bar\"\n❧");
    assert!(matches!(v, Value::Str(ref s) if &**s == "foobar"));
}

#[test]
fn comparison_and_equality_end_to_end() {
    let v = run_ok("result ≔ (1 < 2) ∧ (2 ≡ 2)\n❧");
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn xor_end_to_end() {
    let v = run_ok("result ≔ ◉ ⊻ ◉\n❧");
    assert!(matches!(v, Value::Bool(false)));
}

#[test]
fn and_short_circuits_end_to_end_the_second_operand_never_runs() {
    // False ∧ (1 ÷ 0 == 1): if ∧ evaluated the right side, this would
    // raise DivisionByZero. It must not.
    let v = run_ok("result ≔ ◎ ∧ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(matches!(v, Value::Bool(false)));
}

#[test]
fn and_does_not_short_circuit_when_the_left_side_is_true() {
    let diags = run_err("result ≔ ◉ ∧ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(
        diags[0].message.contains("DivisionByZero"),
        "{}",
        diags[0].message
    );
}

#[test]
fn or_short_circuits_end_to_end_the_second_operand_never_runs() {
    let v = run_ok("result ≔ ◉ ∨ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn or_does_not_short_circuit_when_the_left_side_is_false() {
    let diags = run_err("result ≔ ◎ ∨ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(
        diags[0].message.contains("DivisionByZero"),
        "{}",
        diags[0].message
    );
}

#[test]
fn division_by_zero_end_to_end() {
    let diags = run_err("result ≔ 1 ÷ 0\n❧");
    assert!(
        diags[0].message.contains("DivisionByZero"),
        "{}",
        diags[0].message
    );
}

#[test]
fn negative_modulo_end_to_end() {
    let v = run_ok("result ≔ −7 ⌗ 3\n❧");
    assert_eq!(as_int(v), -1);
}

#[test]
fn nan_equals_nan_end_to_end() {
    // 0.0 ÷ 0.0 == 0.0 ÷ 0.0 — both sides NaN, must be True (§18 total
    // equality), even though NaN < NaN etc. would be False.
    let v = run_ok("result ≔ (0.0 ÷ 0.0) ≡ (0.0 ÷ 0.0)\n❧");
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn mixed_int_real_arithmetic_is_rejected_end_to_end() {
    let diags = run_err("result ≔ 1 ✚ 2.0\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn arithmetic_composes_with_functions_and_letrec_end_to_end() {
    // A self-recursive FunctionDeclaration summing 1..=n via Nat descent,
    // using ✚ in its own body — composition with LetRec/recursion.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         λtoInt(n: Nat): ⟁ → ⟡ n {\n  Zero → 0\n  ⟢ Succ(k) → 1 ✚ toInt(k)\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ toInt(three)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 3);
}

#[test]
fn raise_and_catch_round_trip_end_to_end() {
    let v = run_ok("result ≔ ☊ (☄ DivisionByZero) λ(e) → 42\n❧");
    assert_eq!(as_int(v), 42);
}

#[test]
fn uncaught_raise_end_to_end() {
    let diags = run_err("result ≔ ☄ DivisionByZero\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn catch_intercepts_a_real_division_by_zero_from_the_arithmetic_operator() {
    // Not user-raised — this is runtime's own ÷ raise, and a surface
    // ☊ handler catches it exactly like a user-raised Exception.
    let v = run_ok("result ≔ ☊ (1 ÷ 0) λ(e) → 99\n❧");
    assert_eq!(as_int(v), 99);
}

#[test]
fn raise_is_rejected_when_the_operand_is_not_an_exception() {
    let diags = run_err("result ≔ ☄ 42\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn catch_handler_receives_the_raised_exception_value() {
    // Match the caught Exception by tag inside the handler.
    let source = "result ≔ ☊ (☄ Failure(\"oops\", \"bad\")) λ(e) → ⟡ e {\n  Failure(tag, msg) → tag\n  ⟢ DivisionByZero → \"div0\"\n  ⟢ NonExhaustiveMatch → \"nem\"\n  ⟢ InvalidOperation(m) → m\n  ⟢ IntegerOverflow → \"overflow\"\n  ⟢ _ → \"other\"\n}\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Str(ref s) if &**s == "oops"));
}
