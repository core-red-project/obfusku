//! Details a formatter can silently lose: mutability aliases, comments, escapes, parentheses and indentation.

use crate::support::{assert_behavior_preserved, format_and_reparse, parse_ok};

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

#[test]
fn escapes_keep_the_formatted_source_nfc_and_the_minimum_int_parseable() {
    let src =
        "s ≔ \"e\\u{301}\\r\\u{0}\"\nm ≔ −9223372036854775808\nh ≔ 0xFF ✚ 1_000\nresult ≔ h\n❧";
    let formatted = format_and_reparse(src);
    assert!(formatted.contains("\"e\\u{301}\\r\\0\""), "{formatted}");
    assert!(formatted.contains("−9223372036854775808"), "{formatted}");
    assert!(formatted.contains("255 ✚ 1000"), "{formatted}");
    assert_behavior_preserved(src);
}

#[test]
fn comments_survive_formatting_and_formatting_is_idempotent() {
    let source = "// header\n⟲ prelude\n\n// section\nλf(n: ⟁): ⟁ → n ✚ 1 // trailing\n\n⌈ block\n comment ⌉\nx ≔ f(1)\n❧\n";
    let once = obfusku_cli::fmt_source(source).expect("formats");
    for text in [
        "// header",
        "// section",
        "// trailing",
        "⌈ block\n comment ⌉",
    ] {
        assert!(once.contains(text), "lost {text:?} in:\n{once}");
    }
    assert_eq!(once.matches("// trailing").count(), 1);
    // the trailing comment stays on its declaration's line
    assert!(
        once.lines()
            .any(|l| l.contains("n ✚ 1") && l.contains("// trailing")),
        "{once}"
    );
    let twice = obfusku_cli::fmt_source(&once).expect("formats again");
    assert_eq!(once, twice);
}

#[test]
fn a_comment_inside_a_declaration_is_moved_above_it_not_dropped() {
    let source = "λf(n: ⟁): ⟁ →\n    // why\n    n ✚ 1\n❧\n";
    let out = obfusku_cli::fmt_source(source).expect("formats");
    assert!(out.contains("// why"), "{out}");
    assert!(out.find("// why") < out.find('λ'), "{out}");
}

#[test]
fn an_applied_type_argument_keeps_its_parentheses() {
    let source = "λf(o: Optional ▷ (List ▷ ⟁)): ⟁ → ⟡ o {\n  ⧫(xs) → 1\n  ⟢ ⦰ → 0\n}\n❧\n";
    let out = obfusku_cli::fmt_source(source).expect("formats");
    assert!(out.contains("Optional ▷ (List ▷ ⟁)"), "{out}");
    assert_eq!(out, obfusku_cli::fmt_source(&out).expect("formats again"));
}

#[test]
fn local_bindings_print_as_a_parenthesized_indented_block_that_reparses() {
    let source = "λf(n: ⟁): ⟁ → (\n  a ≔ n ✚ 1\n  b ≔ a ✱ 2\n  ⟡ (b > 3) {\n    ◉ → a ⟢\n    ◎ → b\n  }\n)\n❧\n";
    let out = obfusku_cli::fmt_source(source).expect("formats");
    assert_eq!(out, obfusku_cli::fmt_source(&out).expect("formats again"));
    assert!(out.contains("→ (\n  a ≔ n ✚ 1\n  b ≔ a ✱ 2\n"), "{out}");
}
