//! Built-in exceptions, `raise`/`catch`, records and tuples.

use super::support::*;

#[test]
fn exception_constructors_are_synthesized_and_referenceable() {
    let module = lower_ok("result ≔ Failure(\"tag\", \"payload\")\n❧");
    match &flat(&module)[0].value {
        Expr::Apply { func, arg, .. } => {
            assert!(matches!(**arg, Expr::Lit(Literal::Str(ref s), _) if s == "payload"));
            match &**func {
                Expr::Apply {
                    func: inner,
                    arg: tag,
                    ..
                } => {
                    assert!(matches!(**tag, Expr::Lit(Literal::Str(ref s), _) if s == "tag"));
                    assert!(matches!(**inner, Expr::Var(ref n, _) if n == "Failure"));
                }
                other => panic!("expected inner Apply, got {other:?}"),
            }
        }
        other => panic!("expected an Apply, got {other:?}"),
    }
}

#[test]
fn raise_lowers_directly_to_core_raise() {
    let module = lower_ok("result ≔ ☄ DivisionByZero\n❧");
    assert!(matches!(&flat(&module)[0].value, Expr::Raise { .. }));
}

#[test]
fn catch_lowers_to_core_catch_extracting_handler_param_and_body() {
    let module = lower_ok("result ≔ ☊ (☄ DivisionByZero) λ(e) → 0\n❧");
    match &flat(&module)[0].value {
        Expr::Catch {
            body,
            handler_param,
            handler_body,
            ..
        } => {
            assert!(matches!(**body, Expr::Raise { .. }));
            assert_eq!(handler_param, "e");
            assert!(matches!(**handler_body, Expr::Lit(Literal::Int(0), _)));
        }
        other => panic!("expected a Catch, got {other:?}"),
    }
}

#[test]
fn catch_handler_must_be_a_lambda_not_an_arbitrary_expression() {
    let msg = lower_err("result ≔ ☊ (☄ DivisionByZero) 0\n❧");
    assert!(msg.contains("single-parameter Lambda"), "{msg}");
}

#[test]
fn catch_handler_must_take_exactly_one_parameter() {
    let msg = lower_err("result ≔ ☊ (☄ DivisionByZero) λ(a, b) → 0\n❧");
    assert!(msg.contains("exactly one parameter"), "{msg}");
}

// ── RecordBody / TupleBody lowering (ABSTRACT_GRAMMAR.md §4: "named
//    syntax at the surface, positional underneath, zero new Core
//    mechanism") ───────────────────────────────────────────────────────

#[test]
fn named_construction_reorders_fields_into_declared_order() {
    let module = lower_ok("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { y: 2.0 ⟢ x: 1.0 }\n❧");
    let bindings = flat(&module);
    let p = bindings.iter().find(|b| b.name == "p").unwrap();
    match &p.value {
        Expr::Constructor { tag, args, .. } => {
            assert_eq!(tag, "Point");
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0], Expr::Lit(Literal::Real(v), _) if v == 1.0));
            assert!(matches!(args[1], Expr::Lit(Literal::Real(v), _) if v == 2.0));
        }
        other => panic!("expected a Constructor, got {other:?}"),
    }
}

#[test]
fn named_construction_of_unknown_record_type_is_rejected() {
    let msg = lower_err("p ≔ Point { x: 1.0 }\n❧");
    assert!(msg.contains("not a known record type"), "{msg}");
}

#[test]
fn named_construction_missing_a_field_is_rejected() {
    let msg = lower_err("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1.0 }\n❧");
    assert!(msg.contains("missing field 'y'"), "{msg}");
}

#[test]
fn named_construction_with_an_extra_unknown_field_is_rejected() {
    let msg = lower_err("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1.0 ⟢ y: 2.0 ⟢ z: 3.0 }\n❧");
    assert!(msg.contains("no field named 'z'"), "{msg}");
}

#[test]
fn named_constructor_pattern_reorders_fields_into_declared_order() {
    let module = lower_ok(
        r"
        Point ≔ { x: ⧆ ⟢ y: ⧆ }
        p ≔ Point { x: 1.0 ⟢ y: 2.0 }
        q ≔ ⟡ p {
          Point { y: b ⟢ x: a } → a
        }
        ❧
        ",
    );
    let bindings = flat(&module);
    let q = bindings.iter().find(|b| b.name == "q").unwrap();
    match &q.value {
        Expr::Match { arms, .. } => match &arms[0].pattern {
            obfusku_core::ast::Pattern::Constructor { tag, args, .. } => {
                assert_eq!(tag, "Point");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0], obfusku_core::ast::Pattern::Var(n, _) if n == "a"));
                assert!(matches!(&args[1], obfusku_core::ast::Pattern::Var(n, _) if n == "b"));
            }
            other => panic!("expected a Constructor pattern, got {other:?}"),
        },
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn tuple_body_declaration_lowers_positional_construction_unchanged() {
    // ABSTRACT_GRAMMAR.md §4: tuples need zero new expression/pattern
    // surface forms — ordinary positional Application/Constructor
    // machinery, already correct, needs no changes.
    let module = lower_ok("Pair ≔ (⟁, ⌘)\np ≔ Pair(1, \"a\")\n❧");
    let bindings = flat(&module);
    let p = bindings.iter().find(|b| b.name == "p").unwrap();
    // Ordinary Application curries left-to-right into nested Apply nodes
    // whose innermost callable is the synthesized `Pair` constructor
    // binding — no NamedConstruction/reordering machinery involved at
    // all, exactly as ABSTRACT_GRAMMAR.md §4 predicts for tuples.
    match &p.value {
        Expr::Apply { func, arg, .. } => {
            assert!(matches!(&**arg, Expr::Lit(Literal::Str(s), _) if s == "a"));
            match &**func {
                Expr::Apply { func, arg, .. } => {
                    assert!(matches!(&**func, Expr::Var(n, _) if n == "Pair"));
                    assert!(matches!(&**arg, Expr::Lit(Literal::Int(1), _)));
                }
                other => panic!("expected the inner Apply, got {other:?}"),
            }
        }
        other => panic!("expected an Apply, got {other:?}"),
    }
}

// ── ArrayLiteral / Index lowering (SEMANTIC_CORE.md §9.2) ──────────────
