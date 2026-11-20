//! Type checking: declaration annotations and letrec-group inference, including the limits that are documented rather than fixed.

use crate::support::{as_int, run_err, run_ok};
use obfusku_runtime::Value;

#[test]
fn mutual_recursion_with_genuinely_different_types_in_each_leg_executes_end_to_end() {
    // f: Nat -> Bool and g: Nat -> Int call each other; a naive letrec
    // could force both directions to agree on one type, but needn't.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         intToBool ≔ λ(x) → ◉\n\
         boolToInt ≔ λ(b) → 0\n\
         λf(n: Nat): ○ → ⟡ n {\n  Zero → ◉\n  ⟢ Succ(k) → intToBool(g(k))\n}\n\
         λg(n: Nat): ⟁ → ⟡ n {\n  Zero → 0\n  ⟢ Succ(k) → boolToInt(f(k))\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ f(three)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn incompatible_types_at_a_recursive_call_site_are_rejected() {
    // f's recursive call passes a Bool where its own annotation already
    // pins the slot to Int — must still be a hard type error.
    let source = "λf(x: ⟁): ⟁ → f(◉)\na ≔ f(5)\nresult ≔ a\n❧";
    let diags = run_err(source);
    assert!(diags[0].message.contains("argument type mismatch"));
}

#[test]
fn lambda_never_using_its_own_recursive_slot_stays_fully_polymorphic() {
    // An ordinary ValueDeclaration must still generalize to a
    // polymorphic identity usable at two different types.
    let source = "f ≔ λ(x) → x\na ≔ f(5)\nresult ≔ f(◉)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn earlier_polymorphic_binding_used_at_two_types_from_a_later_lambda_is_not_collapsed() {
    // No FunctionDeclaration here, so no LetRec group at all — basic
    // ordinary-Let-polymorphism regression; see
    // `polymorphism_survives_around_an_actual_letrec_group` for the
    // version that exercises a real LetRec group.
    let source = "id ≔ λ(x) → x\n\
         useTwice ≔ λ(ignored) → id(◉)\n\
         a ≔ id(5)\n\
         result ≔ useTwice(◉)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn polymorphism_survives_around_an_actual_letrec_group() {
    // `id`'s polymorphism must survive being referenced from inside a
    // LetRec group's member, not only from an ordinary ValueDeclaration.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         id ≔ λ(x) → x\n\
         a ≔ id(5)\n\
         λuseId(n: Nat): ○ → ⟡ n {\n  Zero → id(◉)\n  ⟢ Succ(k) → useId(k)\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ useId(three)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn lambda_forward_referencing_a_non_lambda_binding_is_a_documented_limitation_not_a_silent_wrong_answer()
 {
    // `f ≔ λ(ignored) → laterVal` is an ordinary ValueDeclaration, never
    // part of any LetRec group — `laterVal` is simply an unbound
    // reference and must be rejected.
    let source = "f ≔ λ(ignored) → laterVal\nlaterVal ≔ 5\nresult ≔ f(◉)\n❧";
    let diags = run_err(source);
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("unknown variable") || d.message.contains("ambiguous")),
        "{diags:?}"
    );
}

#[test]
fn function_declaration_annotation_matching_the_body_is_accepted() {
    let v = run_ok("λf(x: ⟁): ⟁ → x\nresult ≔ f(5)\n❧");
    assert_eq!(as_int(v), 5);
}

#[test]
fn function_declaration_annotation_that_contradicts_the_body_is_rejected() {
    // Declared to return ⟁ (Int), but the body actually returns ○ (Bool)
    // — this must be a static error, not silently accepted (annotations
    // are no longer discarded once parsed).
    let diags = run_err("λf(x: ⟁): ⟁ → ◉\nresult ≔ f(5)\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn function_declaration_annotation_wrong_parameter_type_is_rejected() {
    // Declared to take ⟁ (Int), called with ○ (Bool).
    let diags = run_err("λf(x: ⟁): ⟁ → x\nresult ≔ f(◉)\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn function_type_and_type_application_annotations_work_end_to_end() {
    // A higher-order function declared with an explicit function-typed
    // parameter (parens required, per the return-type/body arrow
    // disambiguation) and Array's own TypeApplication annotation.
    let source = "Array t ≔ { Empty ⟢ Cons(t, Array ▷ t) }\n\
         λapplyTwice(f: (⟁ → ⟁), x: ⟁): ⟁ → f(f(x))\n\
         inc ≔ λ(n) → n\n\
         result ≔ applyTwice(inc, 5)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 5);
}
