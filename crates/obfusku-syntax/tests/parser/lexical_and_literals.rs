//! Literals, string escapes, comments and the lexical rules.

use super::support::*;

#[test]
fn smallest_legal_program_is_an_empty_sealed_module() {
    let module = parse_ok("❧");
    assert!(module.declarations.is_empty());
}

#[test]
fn one_binding_with_an_int_literal() {
    let module = parse_ok("x ≔ 5\n❧");
    assert_eq!(module.declarations.len(), 1);
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert_eq!(decl.name, "x");
    assert!(!decl.is_mutable);
    assert!(!decl.is_exported);
    assert!(matches!(decl.value, Expression::Int(5, _)));
}

#[test]
fn real_string_and_bool_literals() {
    let module = parse_ok(
        r#"
        a ≔ 3.14
        b ≔ "hello"
        c ≔ ◉
        d ≔ ◎
        ❧
        "#,
    );
    assert_eq!(module.declarations.len(), 4);
}

#[test]
fn unit_literal_in_expression_position_parses_as_expression_unit() {
    // `∅` in expression position is the one Unit value — the same
    // glyph that's `TypeRef::Base(BaseType::Unit, _)` in type position,
    // disambiguated purely by where it occurs.
    let module = parse_ok("x ≔ ∅\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Unit(_)));
}

#[test]
fn mutable_and_export_modifiers_compose() {
    let module = parse_ok(
        r"
        w ≔ 1
        x ≔˚ 1
        y ≔⟳ 1
        z ≔˚⟳ 1
        ❧
        ",
    );
    let flags: Vec<(bool, bool)> = module
        .declarations
        .iter()
        .map(|decl| {
            let Declaration::Value(d) = decl else {
                panic!("expected a Value declaration")
            };
            (d.is_mutable, d.is_exported)
        })
        .collect();
    assert_eq!(
        flags,
        vec![(false, false), (true, false), (false, true), (true, true)]
    );
}

#[test]
fn string_escapes_match_the_closed_grammar_exactly() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §5's EscapeSequence production:
    // exactly \" \\ \n \t \r \0.
    let module = parse_ok(
        r#"x ≔ "a\nb\tc\rd\0e\"f\\g"
❧"#,
    );
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Str(s, _) => assert_eq!(s, "a\nb\tc\rd\0e\"f\\g"),
        other => panic!("expected a Str, got {other:?}"),
    }
}

#[test]
fn raw_newline_is_permitted_inside_a_string_literal() {
    let module = parse_ok("x ≔ \"line one\nline two\"\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Str(s, _) if s == "line one\nline two"));
}

#[test]
fn line_comment_is_skipped() {
    let module = parse_ok("x ≔ 1 // this is a comment, not part of the value\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Int(1, _)));
}

#[test]
fn nested_block_comments_are_skipped() {
    // §2.1: block comments nest — this is only well-defined if the
    // parser sees straight through a comment-inside-a-comment to the
    // real outermost closer.
    let module = parse_ok("x ≔ ⌈ outer ⌈ inner ⌉ still outer ⌉ 1\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Int(1, _)));
}

#[test]
fn comment_glyphs_inside_a_string_are_literal_content_not_comments() {
    let module = parse_ok(
        r#"x ≔ "// not a comment ⌈ either"
❧"#,
    );
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Str(s, _) if s == "// not a comment ⌈ either"));
}

#[test]
fn unsupported_escape_sequence_is_rejected() {
    let msg = parse_err(
        r#"x ≔ "\q"
❧"#,
    );
    assert!(
        msg.contains("unsupported escape sequence"),
        "message was: {msg}"
    );
}

#[test]
fn unterminated_block_comment_is_rejected() {
    let msg = parse_err("x ≔ ⌈ never closed\n❧");
    assert!(
        msg.contains("unterminated block comment"),
        "message was: {msg}"
    );
}
