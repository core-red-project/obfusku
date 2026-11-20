//! Constructors, `⟡` matches and patterns.

use super::support::*;

#[test]
fn constructor_application_parses_as_ordinary_application() {
    // §7.6: PositionalConstruction is syntactically identical to
    // Application with an uppercase Callable — no special grammar.
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\nx ≔ Some(5)\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { callable, args, .. } => {
            assert!(matches!(**callable, Expression::Reference(ref n, _) if n == "Some"));
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

#[test]
fn bare_nullary_constructor_reference_parses_as_an_ordinary_reference() {
    // §7.1's fix: an uppercase bare identifier is an ordinary Reference,
    // which is what makes `None` (no parens) parseable at all.
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\nx ≔ None\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Reference(n, _) if n == "None"));
}

#[test]
fn match_over_an_option_like_adt() {
    let module = parse_ok(
        r"
        Option t ≔ { Some(t) ⟢ None }
        x ≔ Some(5)
        y ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        ",
    );
    let Declaration::Value(decl) = &module.declarations[2] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expression::Reference(ref n, _) if n == "x"));
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                obfusku_syntax::ast::Pattern::Constructor { tag, args, .. } => {
                    assert_eq!(tag, "Some");
                    assert_eq!(args.len(), 1);
                    assert!(
                        matches!(&args[0], obfusku_syntax::ast::Pattern::Var(n, _) if n == "n")
                    );
                }
                other => panic!("expected a Constructor pattern, got {other:?}"),
            }
            assert!(matches!(
                &arms[1].pattern,
                obfusku_syntax::ast::Pattern::Constructor { tag, args, .. }
                    if tag == "None" && args.is_empty()
            ));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn wildcard_and_literal_patterns_parse() {
    let module = parse_ok(
        r#"
        y ≔ ⟡ 5 {
          0 → "zero"
          ⟢ _ → "other"
        }
        ❧
        "#,
    );
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Match { arms, .. } => {
            assert!(matches!(
                arms[0].pattern,
                obfusku_syntax::ast::Pattern::Int(0, _)
            ));
            assert!(matches!(
                arms[1].pattern,
                obfusku_syntax::ast::Pattern::Wildcard(_)
            ));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn nested_constructor_pattern_parses() {
    let module = parse_ok(
        r"
        Option t ≔ { Some(t) ⟢ None }
        y ≔ ⟡ x {
          Some(Some(n)) → n
          ⟢ Some(None) → 0
          ⟢ None → 0
        }
        ❧
        ",
    );
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Match { arms, .. } => match &arms[0].pattern {
            obfusku_syntax::ast::Pattern::Constructor { tag, args, .. } => {
                assert_eq!(tag, "Some");
                assert!(
                    matches!(&args[0], obfusku_syntax::ast::Pattern::Constructor { tag, .. } if tag == "Some")
                );
            }
            other => panic!("expected a Constructor pattern, got {other:?}"),
        },
        other => panic!("expected a Match, got {other:?}"),
    }
}

// ── invalid ADT/Match programs ──────────────────────────────────────────

#[test]
fn optional_and_result_are_ordinary_adt_declarations() {
    // Same SumBody production as Option; constructor names aren't
    // spec-mandated, chosen here to avoid colliding with Exception's tags.
    let module = parse_ok("Optional t ≔ { Some(t) ⟢ None }\nResult t e ≔ { Ok(t) ⟢ Err(e) }\n❧");
    let Declaration::Type(opt) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(opt.type_params, vec!["t".to_string()]);
    let Declaration::Type(res) = &module.declarations[1] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(res.type_params, vec!["t".to_string(), "e".to_string()]);
    assert_eq!(res.variants[0].tag, "Ok");
    assert_eq!(res.variants[1].tag, "Err");
}

#[test]
fn result_match_parses_with_two_independent_constructors() {
    let module = parse_ok(
        r"
        Result t e ≔ { Ok(t) ⟢ Err(e) }
        y ≔ ⟡ r {
          Ok(n) → n
          ⟢ Err(msg) → 0
        }
        ❧
        ",
    );
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Match { .. }));
}
