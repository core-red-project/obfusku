//! Formatting each construct preserves meaning: the output re-parses and behaves identically.

use crate::support::{assert_behavior_preserved, format_and_reparse};

#[test]
fn empty_module_round_trips() {
    assert_behavior_preserved("\u{2767}\n");
}

#[test]
fn literals_round_trip() {
    assert_behavior_preserved("x \u{2254} 5\n\u{2767}\n");
    assert_behavior_preserved("x \u{2254} 3.5\n\u{2767}\n");
    assert_behavior_preserved("x \u{2254} \"hello\"\n\u{2767}\n");
    assert_behavior_preserved("x \u{2254} \u{25C9}\n\u{2767}\n"); // ◉
    assert_behavior_preserved("x \u{2254} \u{25CE}\n\u{2767}\n"); // ◎
}

#[test]
fn arithmetic_precedence_round_trips() {
    // Must NOT collapse to `(1 + 2) * 3` — the parenthesization here is
    // exactly the property under test.
    assert_behavior_preserved("result \u{2254} 1 \u{271A} 2 \u{2731} 3\n\u{2767}\n");
    assert_behavior_preserved("result \u{2254} (1 \u{271A} 2) \u{2731} 3\n\u{2767}\n");
}

#[test]
fn right_associative_subtraction_round_trips() {
    // `a - (b - c)` must stay parenthesized on the right, or it would
    // re-parse as `(a - b) - c` — a different program (10 vs 6 here).
    let sub = "\u{2620}\u{FE0E}";
    let source = format!("result \u{2254} 10 {sub} (2 {sub} 4)\n\u{2767}\n");
    assert_behavior_preserved(&source);
}

#[test]
fn comparison_and_equality_round_trip() {
    assert_behavior_preserved("result \u{2254} (1 < 2) \u{2227} (2 \u{2261} 2)\n\u{2767}\n");
}

#[test]
fn logical_operators_round_trip() {
    assert_behavior_preserved("result \u{2254} \u{25C9} \u{2228} \u{25CE}\n\u{2767}\n");
    assert_behavior_preserved("result \u{2254} \u{25C9} \u{22BB} \u{25C9}\n\u{2767}\n");
}

#[test]
fn unary_operators_round_trip() {
    assert_behavior_preserved("result \u{2254} \u{00AC}\u{25C9}\n\u{2767}\n");
    assert_behavior_preserved("result \u{2254} \u{2212}5\n\u{2767}\n");
}

#[test]
fn lambda_and_application_round_trip() {
    assert_behavior_preserved("f \u{2254} \u{3bb}(x) \u{2192} x\nresult \u{2254} f(5)\n\u{2767}\n");
}

#[test]
fn function_declaration_round_trips() {
    let source = "\u{3bb}f(n: \u{27C1}): \u{27C1} \u{2192} n\nresult \u{2254} f(5)\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn self_recursive_function_declaration_round_trips() {
    let source = "\u{3bb}countdown(n: \u{27C1}): \u{27C1} \u{2192} \u{27E1} n {\n  \
        0 \u{2192} 0\n  \u{27E2} m \u{2192} countdown(m \u{2620}\u{FE0E} 1)\n}\n\
        result \u{2254} countdown(10)\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn adt_sum_declaration_and_match_round_trip() {
    let source = "Option t \u{2254} { Some(t) \u{27E2} None }\n\
        x \u{2254} Some(5)\n\
        result \u{2254} \u{27E1} x {\n  \
        Some(n) \u{2192} n\n  \u{27E2} None \u{2192} 0\n}\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn record_type_and_named_construction_round_trip() {
    let source = "Point \u{2254} { x: \u{29C6} \u{27E2} y: \u{29C6} }\n\
        p \u{2254} Point { x: 1.0 \u{27E2} y: 2.0 }\n\
        result \u{2254} \u{27E1} p {\n  \
        Point { x: a \u{27E2} y: b } \u{2192} a\n}\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn tuple_type_and_positional_pattern_round_trip() {
    let source = "Pair \u{2254} (\u{27C1}, \u{2318})\n\
        p \u{2254} Pair(1, \"a\")\n\
        result \u{2254} \u{27E1} p {\n  \
        Pair(n, s) \u{2192} n\n}\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn local_value_binding_round_trips() {
    let source = "result \u{2254} x \u{2254} 5\n x \u{271A} 1\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn local_function_declaration_round_trips() {
    let source = "result \u{2254} \u{3bb}f(n: \u{27C1}): \u{27C1} \u{2192} n\n f(5)\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn raise_and_catch_round_trip() {
    let source =
        "result \u{2254} \u{260A} (\u{2604} DivisionByZero) \u{3bb}(e) \u{2192} 0\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn array_literal_and_index_round_trip() {
    let source = "xs \u{2254} [1, 2, 3]\nresult \u{2254} xs[1]\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn chained_indexing_round_trips() {
    let source = "xs \u{2254} [[1, 2], [3, 4]]\nresult \u{2254} xs[1][0]\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn pipeline_round_trips() {
    let source = "f \u{2254} \u{3bb}(x) \u{2192} x\nxs \u{2254} 5\nresult \u{2254} xs \u{25B7} f\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn mutation_round_trips() {
    let source = "counter \u{2254}\u{2DA} 0\n\
        bump \u{2254} \u{3bb}(step) \u{2192} \u{2DA}counter \u{2699}\u{FE0E} step\n\
        _called \u{2254} bump(5)\n\
        result \u{2254} counter\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn function_type_annotation_round_trips() {
    let source = "\u{3bb}applyTwice(f: (\u{27C1} \u{2192} \u{27C1}), x: \u{27C1}): \u{27C1} \u{2192} f(f(x))\n\
        inc \u{2254} \u{3bb}(n) \u{2192} n\n\
        result \u{2254} applyTwice(inc, 5)\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn type_application_annotation_round_trips() {
    let source = "Array t \u{2254} { Empty \u{27E2} Cons(t, Array \u{25B7} t) }\n\
        \u{3bb}g(x: Array \u{25B7} \u{27C1}): \u{27C1} \u{2192} 1\n\
        result \u{2254} g(Empty)\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn ill_typed_program_still_formats_and_reparses() {
    // §13's whole point: formatting must not require type-checking to
    // succeed first.
    format_and_reparse("result \u{2254} nope\n\u{2767}\n");
}

#[test]
fn syntactically_valid_but_ill_typed_arithmetic_still_formats() {
    format_and_reparse("result \u{2254} 1 \u{271A} \u{25C9}\n\u{2767}\n");
}
