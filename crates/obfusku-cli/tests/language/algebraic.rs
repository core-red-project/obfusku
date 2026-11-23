//! Data definitions: ADTs and pattern matching, `Optional`/`Result`, records and tuples, `List` and `Array`.

use crate::support::{as_int, run_err, run_ok};
use obfusku_runtime::Value;

#[test]
fn adt_constructor_and_exhaustive_match_end_to_end() {
    let v = run_ok(
        r"
        Option t ≔ { Some(t) ⟢ None }
        x ≔ Some(5)
        result ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        ",
    );
    assert_eq!(as_int(v), 5);
}

#[test]
fn adt_none_case_of_the_same_match_end_to_end() {
    let v = run_ok(
        r"
        Option t ≔ { Some(t) ⟢ None }
        x ≔ None
        result ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        ",
    );
    assert_eq!(as_int(v), 0);
}

#[test]
fn optional_end_to_end_no_special_runtime_or_typecheck_path() {
    let v = run_ok(
        r"
        Optional t ≔ { Some(t) ⟢ None }
        x ≔ Some(42)
        result ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        ",
    );
    assert_eq!(as_int(v), 42);
}

#[test]
fn result_end_to_end_no_special_runtime_or_typecheck_path() {
    // Err's arm returns `msg`, not a throwaway literal, so both arms
    // unify and pin `e = Int` — `Ok(7)` alone leaves `e` ambiguous.
    let v = run_ok(
        r"
        Result t e ≔ { Ok(t) ⟢ Err(e) }
        r ≔ Ok(7)
        result ≔ ⟡ r {
          Ok(n) → n
          ⟢ Err(msg) → msg
        }
        ❧
        ",
    );
    assert_eq!(as_int(v), 7);
}

#[test]
fn record_construction_and_named_pattern_match_end_to_end() {
    let source = "Point ≔ { x: ⧆ ⟢ y: ⧆ }\n\
         p ≔ Point { y: 2.0 ⟢ x: 1.0 }\n\
         result ≔ ⟡ p {\n\
           Point { x: a ⟢ y: b } → a ✚ b\n\
         }\n❧";
    let v = run_ok(source);
    match v {
        Value::Real(r) => assert_eq!(r, 3.0),
        other => panic!("expected Real, got {other:?}"),
    }
}

#[test]
fn record_construction_via_positional_syntax_also_works() {
    // ABSTRACT_GRAMMAR.md §4: Record/Tuple share the same Core
    // Constructor machinery as ordinary sum variants, so a record's
    // single self-tagged variant is still constructible positionally.
    let source = "Point ≔ { x: ⧆ ⟢ y: ⧆ }\n\
         p ≔ Point(1.0, 2.0)\n\
         result ≔ ⟡ p {\n\
           Point(a, b) → a ✚ b\n\
         }\n❧";
    let v = run_ok(source);
    match v {
        Value::Real(r) => assert_eq!(r, 3.0),
        other => panic!("expected Real, got {other:?}"),
    }
}

#[test]
fn tuple_construction_and_positional_pattern_match_end_to_end() {
    let source = "Pair ≔ (⟁, ⌘)\n\
         p ≔ Pair(1, \"a\")\n\
         result ≔ ⟡ p {\n\
           Pair(n, s) → n\n\
         }\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn named_construction_of_an_unknown_record_type_is_a_static_error() {
    let diags = run_err("p ≔ Point { x: 1.0 }\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn named_construction_with_a_missing_field_is_a_static_error() {
    let diags = run_err("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1.0 }\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn named_construction_field_type_mismatch_is_rejected_by_typecheck() {
    // `x` is declared ⧆ (Real); passing an Int must be a static error —
    // records reuse ordinary Constructor typing, no separate rule needed.
    let diags = run_err("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1 ⟢ y: 2.0 }\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn list_adt_end_to_end_via_explicit_type_application() {
    let source = "List t ≔ { Nil ⟢ Cons(t, List ▷ t) }\n\
         xs ≔ Cons(1, Cons(2, Nil))\n\
         result ≔ ⟡ xs {\n\
           Nil → 0\n\
           ⟢ Cons(h, t) → h\n\
         }\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn list_open_structural_recursion_end_to_end() {
    // List supports open head/rest recursion via ordinary Match/
    // Constructor recursion. `h ✚ sum(t)` needs no literal-leads
    // workaround: `h` is pattern-bound against `xs`'s already-concrete
    // element type, and `sum`'s own declared return type is already
    // known at its recursive call site — neither is an unconstrained
    // Lambda parameter, so operand order here was never the issue P0-A
    // fixed elsewhere.
    let source = "List t ≔ { Nil ⟢ Cons(t, List ▷ t) }\n\
         λsum(xs: List ▷ ⟁): ⟁ → ⟡ xs {\n\
           Nil → 0\n\
           ⟢ Cons(h, t) → h ✚ sum(t)\n\
         }\n\
         result ≔ sum(Cons(1, Cons(2, Cons(3, Nil))))\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 6);
}

#[test]
fn bare_generic_tag_with_wrong_arity_is_a_static_arity_error() {
    // A bare 'List' (zero args) used where 'List' is declared with one
    // type parameter — must be a real type error, never silently
    // accepted as a zero-argument reference to the same type.
    let source = "List t ≔ { Nil ⟢ Cons(t, List) }\n\
         xs ≔ Cons(1, Nil)\n❧";
    let diags = run_err(source);
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("expects 1 type argument"))
    );
}

#[test]
fn array_literal_and_index_end_to_end() {
    let v = run_ok("xs ≔ [10, 20, 30]\nresult ≔ xs[1]\n❧");
    assert_eq!(as_int(v), 20);
}

#[test]
fn empty_array_literal_end_to_end_context_pins_element_type() {
    // `[]`'s element type is pinned via `Match`'s "every arm shares one
    // type" rule against a concrete `Array<⟁>` sibling, without indexing
    // into (and evaluating) the empty array itself.
    let source = "xs ≔ []\nys ≔ [1, 2, 3]\nresult ≔ ⟡ ◉ {\n  ◉ → xs\n  ⟢ ◎ → ys\n}\n❧";
    let v = run_ok(source);
    match &v {
        Value::Array(elems) => assert!(elems.is_empty()),
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
fn out_of_bounds_index_is_an_uncaught_exception_end_to_end() {
    let diags = run_err("xs ≔ [1, 2]\nresult ≔ xs[9]\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn out_of_bounds_index_is_catchable_end_to_end() {
    let v = run_ok("xs ≔ [1, 2]\nresult ≔ ☊ xs[9] λ(e) → 0\n❧");
    assert_eq!(as_int(v), 0);
}

#[test]
fn array_element_type_mismatch_is_a_static_error() {
    let diags = run_err("xs ≔ [1, ◉, 3]\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn array_index_must_be_int_static_error() {
    let diags = run_err("xs ≔ [1, 2]\nresult ≔ xs[\"nope\"]\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn chained_indexing_of_nested_arrays_end_to_end() {
    let v = run_ok("xs ≔ [[1, 2], [3, 4]]\nresult ≔ xs[1][0]\n❧");
    assert_eq!(as_int(v), 3);
}

#[test]
fn array_equality_end_to_end() {
    let v = run_ok("a ≔ [1, 2, 3]\nb ≔ [1, 2, 3]\nresult ≔ ⟡ (a ≡ b) {\n  ◉ → 1\n  ⟢ ◎ → 0\n}\n❧");
    assert_eq!(as_int(v), 1);
}
