//! Records, generated ordering and rendering, `Map`/`Set`, and open user exceptions (`ADR-025`, `ADR-026`).

use crate::support::{as_int, run_full_err, run_full_ok, run_str};
use obfusku_runtime::Value;

#[test]
fn record_field_access_and_update() {
    let v = run_full_ok(
        "Point ≔ { x: ⟁ ⟢ y: ⟁ }\np ≔ Point { x: 1 ⟢ y: 2 }\nq ≔ p.{ x: 10 }\n\
         result ≔ q.x ✚ q.y ✚ p.x\n❧",
    );
    assert_eq!(as_int(v), 13);
}

#[test]
fn nested_record_access_chains() {
    let v = run_full_ok(
        "P ≔ { x: ⟁ }\nS ≔ { a: P ⟢ b: P }\ns ≔ S { a: P { x: 1 } ⟢ b: P { x: 5 } }\n\
         result ≔ s.b.x ☠︎ s.a.x\n❧",
    );
    assert_eq!(as_int(v), 4);
}

#[test]
fn unknown_record_field_is_a_type_error() {
    let ds = run_full_err("P ≔ { x: ⟁ }\np ≔ P { x: 1 }\nresult ≔ p.z\n❧");
    assert!(ds[0].message.contains("no field") || ds[0].message.contains("field named"));
}

#[test]
fn update_with_the_wrong_field_type_is_a_type_error() {
    let ds = run_full_err("P ≔ { x: ⟁ }\np ≔ P { x: 1 }\nresult ≔ p.{ x: \"s\" }\n❧");
    assert!(ds[0].message.contains("field 'x'"), "{:?}", ds[0]);
}

#[test]
fn ordering_covers_strings_adts_and_nested_values() {
    assert!(matches!(
        run_full_ok("result ≔ \"apple\" < \"banana\"\n❧"),
        Value::Bool(true)
    ));
    assert!(matches!(
        run_full_ok("S ≔ { A(⟁) ⟢ B }\nresult ≔ A(9) < B\n❧"),
        Value::Bool(true)
    ));
    assert!(matches!(
        run_full_ok("S ≔ { A(⟁) ⟢ B }\nresult ≔ A(1) < A(2)\n❧"),
        Value::Bool(true)
    ));
    // IEEE-754 still governs a bare Real.
    assert!(matches!(
        run_full_ok("n ≔ 0.0 ÷ 0.0\nresult ≔ n < 1.0\n❧"),
        Value::Bool(false)
    ));
}

#[test]
fn ordering_a_function_is_a_type_error_even_through_a_generic() {
    let ds = run_full_err(
        "λmax2(a: t, b: t): t → ⟡ (a > b) { ◉ → a ⟢ ◎ → b }\nλf(n: ⟁): ⟁ → n\n\
         result ≔ max2(f, f)\n❧",
    );
    assert!(ds[0].message.contains("ordering"), "{:?}", ds[0]);
}

#[test]
fn generic_function_with_an_ordering_constraint_works_at_many_types() {
    let v = run_full_ok(
        "λmax2(a: t, b: t): t → ⟡ (a > b) { ◉ → a ⟢ ◎ → b }\n\
         s ≔ max2(\"a\", \"b\")\nresult ≔ max2(3, 2)\n❧",
    );
    assert_eq!(as_int(v), 3);
}

#[test]
fn show_renders_records_variants_and_lists_in_source_form() {
    assert_eq!(
        run_str("P ≔ { x: ⟁ ⟢ y: ⟁ }\nresult ≔ ⍕(⁝(P { x: 1 ⟢ y: 2 }, ⊘))\n❧"),
        "⁝(P { x: 1 ⟢ y: 2 }, ⊘)"
    );
    assert_eq!(run_str("S ≔ { C(⧆) }\nresult ≔ ⍕(C(1.5))\n❧"), "C(1.5)");
    assert_eq!(run_str("result ≔ ⍕(\"a\\\"b\")\n❧"), "\"a\\\"b\"");
}

#[test]
fn rendering_a_function_is_a_type_error() {
    let ds = run_full_err("λf(n: ⟁): ⟁ → n\nresult ≔ ⍕(f)\n❧");
    assert!(ds[0].message.contains("rendering"), "{:?}", ds[0]);
}

#[test]
fn array_construction_and_combination() {
    assert_eq!(
        run_str("result ≔ ⍕(⊕(⌷(1)(3)(⍪(⍳(3))(⊠(2)(9))))(100))\n❧"),
        "[1, 2, 9, 100]"
    );
    assert_eq!(run_str("result ≔ ⍕(⍋([3, 1, 2]))\n❧"), "[1, 2, 3]");
}

#[test]
fn large_array_is_built_and_folded_without_quadratic_copying() {
    let v = run_full_ok(
        "big ≔ ⊡(λ(x: ⟁) → x)(⍳(200000))\n\
         result ≔ ⊞(λ(a: ⟁) → λ(x: ⟁) → a ✚ x)(0)(big)\n❧",
    );
    assert_eq!(as_int(v), 19_999_900_000);
}

#[test]
fn map_and_set_are_persistent_and_ordered() {
    assert_eq!(
        run_str(
            "m ≔ ⊎(⊎(⍬)(\"b\")(2))(\"a\")(1)\nm2 ≔ ⊖(m)(\"a\")\n\
             result ≔ ⍕(⁝(⍕(m), ⁝(⍕(m2), ⊘)))\n❧"
        ),
        "⁝(\"Map[\\\"a\\\" ↦ 1, \\\"b\\\" ↦ 2]\", ⁝(\"Map[\\\"b\\\" ↦ 2]\", ⊘))"
    );
    let v = run_full_ok("s ≔ ⊌(⊌(⍭)(3))(3)\nresult ≔ ⊚(s)\n❧");
    assert_eq!(as_int(v), 1);
}

#[test]
fn map_lookup_returns_an_optional() {
    let v = run_full_ok("m ≔ ⊎(⍬)(1)(\"one\")\nresult ≔ ⟡ ⌕(m)(1) { ⧫(s) → ⍴(s) ⟢ ⦰ → 0 }\n❧");
    assert_eq!(as_int(v), 3);
}

#[test]
fn map_keys_must_be_orderable() {
    let ds = run_full_err("λf(n: ⟁): ⟁ → n\nresult ≔ ⊎(⍬)(f)(1)\n❧");
    assert!(ds[0].message.contains("ordering"), "{:?}", ds[0]);
}

#[test]
fn user_declared_exception_is_raised_and_caught_by_variant() {
    let v = run_full_ok(
        "☈ NotFound(⌘)\n☈ Bad(⟁, ⌘)\nλfind(k: ⌘): ⟁ → ☄ NotFound(k)\n\
         result ≔ ☊ find(\"x\") λ(e) → ⟡ e { NotFound(k) → 1 ⟢ Bad(n, m) → n ⟢ _ → 0 }\n❧",
    );
    assert_eq!(as_int(v), 1);
}

#[test]
fn exception_match_without_a_wildcard_is_rejected_as_open() {
    let ds = run_full_err("☈ NotFound(⌘)\nresult ≔ ☊ 1 λ(e) → ⟡ e { NotFound(k) → 1 }\n❧");
    assert!(ds[0].message.contains("open type"), "{:?}", ds[0]);
}

#[test]
fn uncaught_user_exception_is_reported_with_its_rendering() {
    let ds = run_full_err("☈ NotFound(⌘)\nλf(n: ⟁): ⟁ → ☄ NotFound(\"k\")\nresult ≔ f(1)\n❧");
    assert!(ds[0].message.contains("NotFound(\"k\")"), "{:?}", ds[0]);
}

#[test]
fn a_second_program_on_one_thread_sees_its_own_record_layout() {
    // The runtime keeps declared-type metadata per thread; a later
    // program redeclaring the same record name must replace it, not
    // inherit the first program's field order.
    let a = run_full_ok("A ≔ { x: ⟁ ⟢ y: ⟁ }\nresult ≔ (A { x: 1 ⟢ y: 2 }).y\n❧");
    let b = run_full_ok("A ≔ { y: ⟁ ⟢ x: ⟁ }\nresult ≔ (A { y: 1 ⟢ x: 2 }).y\n❧");
    assert_eq!(as_int(a), 2);
    assert_eq!(as_int(b), 1);
}
