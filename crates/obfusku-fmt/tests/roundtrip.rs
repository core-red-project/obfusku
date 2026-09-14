//! Correctness contract for `obfusku_fmt::format`: reformatting a
//! program must not change what it means. Two layers, deliberately —
//! `format(m)` re-parsing at all doesn't prove the *tree* is right (a
//! formatter could drop an argument and still emit syntactically valid
//! nonsense), so most fixtures below are also run through the real
//! `obfusku-cli` pipeline before and after formatting and checked for
//! an identical runtime result. `obfusku-fmt` itself has no dependency
//! on the typecheck/runtime stack (`IMPLEMENTATION_ARCHITECTURE.md`
//! §13); these are dev-only dependencies, used solely to verify the
//! formatter from the outside.

use obfusku_syntax::{lexer, parser};

fn parse_ok(source: &str) -> obfusku_syntax::ast::Module {
    let mut map = obfusku_diagnostics::SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s) in {source:?}: {ds:?}"))
}

/// Formats `source`, asserts the result re-parses without error, and
/// returns the formatted text.
fn format_and_reparse(source: &str) -> String {
    let module = parse_ok(source);
    let formatted = obfusku_fmt::format(&module);
    parse_ok(&formatted); // must not panic
    formatted
}

/// The stronger check: runs `source` and `format(parse(source))` both
/// through the real pipeline and asserts identical runtime results —
/// proving the formatter didn't change program *behavior*, not merely
/// that its output happens to parse.
fn assert_behavior_preserved(source: &str) {
    let formatted = format_and_reparse(source);
    let before = obfusku_cli::run_source(source);
    let after = obfusku_cli::run_source(&formatted);
    match (before, after) {
        (Ok(b), Ok(a)) => assert_eq!(
            format!("{:?}", b.0),
            format!("{:?}", a.0),
            "formatting changed the result of:\n{source}\n---formatted to---\n{formatted}"
        ),
        (Err(be), Err(ae)) => assert_eq!(
            be.len(),
            ae.len(),
            "formatting changed the error count of:\n{source}\n---formatted to---\n{formatted}"
        ),
        (b, a) => panic!(
            "formatting changed success/failure of:\n{source}\n---formatted to---\n{formatted}\n\
             before: {b:?}\nafter: {a:?}"
        ),
    }
}

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
    assert_behavior_preserved("result \u{2254} (1 < 2) \u{2227} (2 == 2)\n\u{2767}\n");
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

// ---------------------------------------------------------------------
// P0-C: §8.1's own documented `≔`/`˚` ambiguity ("`≔` immediately
// followed by `˚` is always the `Mutability` modifier... a binding
// whose value is a bare `ReferenceForm` at the top level must be
// parenthesized"). The formatter used to print an immutable binding's
// `ExplicitRef`/explicit-capture-`Rebind` value with no parens, so
// re-parsing silently turned it into a *mutable* binding with a
// completely different value — `assert_behavior_preserved` alone (an
// `==` on the pretty-printed `Value::Debug` string) would have caught
// this, but it's worth a dedicated regression precisely because a
// "re-parses without error" check (`format_and_reparse` alone) would
// NOT have: the corrupted output is syntactically valid, just wrong.
// ---------------------------------------------------------------------

#[test]
fn mutable_cell_alias_via_explicit_ref_round_trips() {
    // `alias` must stay an *immutable* binding holding the same `Cell`
    // as `c` — mutating `c` afterward must still be observable through
    // `alias`. Before the fix this reformatted to `alias ≔˚ c` (a
    // brand-new, independent cell seeded with `c`'s value at that
    // point), silently breaking the aliasing this test exists to prove.
    let source = "c \u{2254}\u{2da} 1\n\
        alias \u{2254} (\u{2da}c)\n\
        unused \u{2254} c \u{2699}\u{fe0e} 2\n\
        result \u{2254} alias\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn immutable_binding_holding_an_explicit_capture_rebind_round_trips() {
    let source = "y \u{2254}\u{2da} 1\n\
        x \u{2254} (\u{2da}y \u{2699}\u{fe0e} 5)\n\
        result \u{2254} y\n\u{2767}\n";
    assert_behavior_preserved(source);
}

#[test]
fn an_ordinary_mutable_binding_with_no_alias_round_trips() {
    // The baseline this bug could have been confused with: an actually
    // mutable binding's own `˚` is never ambiguous and needs no parens.
    assert_behavior_preserved("c \u{2254}\u{2da} 1\nresult \u{2254} c\n\u{2767}\n");
}

#[test]
fn explicit_ref_alias_round_trips_to_an_identical_ast_not_just_an_identical_value() {
    // `assert_behavior_preserved` only compares the final runtime
    // value — confirm the underlying AST fact directly too: `alias`
    // must stay `mutable: false` with an `ExplicitRef` value, not
    // silently become `mutable: true` with a plain `Reference`.
    let source =
        "c \u{2254}\u{2da} 1\nalias \u{2254} (\u{2da}c)\nresult \u{2254} alias\n\u{2767}\n";
    let formatted = format_and_reparse(source);
    let reparsed = parse_ok(&formatted);
    let obfusku_syntax::ast::Declaration::Value(alias_decl) = reparsed
        .declarations
        .iter()
        .find(|d| matches!(d, obfusku_syntax::ast::Declaration::Value(v) if v.name == "alias"))
        .expect("alias binding must exist")
    else {
        unreachable!()
    };
    assert!(
        !alias_decl.mutable,
        "alias must stay immutable: {formatted}"
    );
    assert!(
        matches!(
            alias_decl.value,
            obfusku_syntax::ast::Expression::ExplicitRef(..)
        ),
        "alias's value must stay an ExplicitRef: {formatted}"
    );
}
