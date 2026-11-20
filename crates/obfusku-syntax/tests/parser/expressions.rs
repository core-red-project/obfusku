//! Lambdas, calls, holes, pipes, locals, arrays, indexing, `raise` and `catch`.

use super::support::*;

#[test]
fn anonymous_lambda_identity_function() {
    let module = parse_ok("f ≔ λ(x) → x\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Lambda { params, body, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(matches!(**body, Expression::Reference(ref n, _) if n == "x"));
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn application_of_a_reference() {
    let module = parse_ok("y ≔ f(5)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { callable, args, .. } => {
            assert!(matches!(**callable, Expression::Reference(ref n, _) if n == "f"));
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

// ── P1-1: `Argument ::= Expression | "•"` (§7.2/§12.1) ─────────────────

#[test]
fn a_hole_argument_parses_as_a_hole_not_an_expression() {
    let module = parse_ok("y ≔ f(1, •, 3)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { args, .. } => {
            assert_eq!(args.len(), 3);
            assert!(matches!(args[0], Argument::Expr(Expression::Int(1, _))));
            assert!(matches!(args[1], Argument::Hole(_)));
            assert!(matches!(args[2], Argument::Expr(Expression::Int(3, _))));
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

#[test]
fn multiple_holes_in_one_application_all_parse() {
    let module = parse_ok("y ≔ f(•, 2, •)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { args, .. } => {
            assert!(matches!(args[0], Argument::Hole(_)));
            assert!(matches!(args[1], Argument::Expr(Expression::Int(2, _))));
            assert!(matches!(args[2], Argument::Hole(_)));
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

#[test]
fn pipe_stage_mixing_pipe_ref_and_hole_is_a_parse_error() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §12.1/§16.1's reserved-invalid
    // combination: presence of '◈' forces expression-stage
    // interpretation, where '•' has no meaning.
    let msg = parse_err("y ≔ xs ▷ (f(◈, •))\n❧");
    assert!(msg.contains('◈') && msg.contains('•'), "message was: {msg}");
}

#[test]
fn pipeline_with_a_bare_callable_stage() {
    let module = parse_ok("z ≔ xs ▷ f\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Pipe { subject, stage, .. } => {
            assert!(matches!(**subject, Expression::Reference(ref n, _) if n == "xs"));
            assert!(
                matches!(**stage, Stage::Callable(Expression::Reference(ref n, _)) if n == "f")
            );
        }
        other => panic!("expected a Pipe, got {other:?}"),
    }
}

#[test]
fn nested_pipeline_with_pipe_reference_inside_parens() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §12/§16.1's hardest case: the inner
    // "◈" belongs to the inner pipe's stage, never the outer one.
    let module = parse_ok("z ≔ xs ▷ (◈ ▷ g)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Pipe { stage, .. } => match &**stage {
            Stage::Expr(Expression::Pipe {
                subject,
                stage: inner,
                ..
            }) => {
                assert!(matches!(**subject, Expression::PipeRef(_)));
                assert!(
                    matches!(**inner, Stage::Callable(Expression::Reference(ref n, _)) if n == "g")
                );
            }
            other => panic!("expected the outer stage to be a nested Pipe, got {other:?}"),
        },
        other => panic!("expected a Pipe, got {other:?}"),
    }
}

#[test]
fn declaration_boundary_falls_out_of_ordinary_parsing() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §15's worked example, using pipe
    // instead of "✚" (not implemented this slice): a newline before a
    // valid continuation token does not end the declaration.
    let module = parse_ok(
        r"
        x ≔ a
          ▷ f
        y ≔ b
        ❧
        ",
    );
    assert_eq!(module.declarations.len(), 2);
    let Declaration::Value(x) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(x.value, Expression::Pipe { .. }));
    let Declaration::Value(y) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(y.value, Expression::Reference(ref n, _) if n == "b"));
}

#[test]
fn lambda_body_extends_across_a_pipe_before_the_next_declaration() {
    let module = parse_ok("f ≔ λ(x) → x ▷ g\ny ≔ 1\n❧");
    assert_eq!(module.declarations.len(), 2);
    let Declaration::Value(f) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &f.value {
        Expression::Lambda { body, .. } => assert!(matches!(**body, Expression::Pipe { .. })),
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

// ── invalid programs ────────────────────────────────────────────────────

#[test]
fn raise_parses_its_operand() {
    let module = parse_ok("result ≔ ☄ DivisionByZero\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Raise(value, _) => {
            assert!(matches!(**value, Expression::Reference(ref n, _) if n == "DivisionByZero"));
        }
        other => panic!("expected a Raise, got {other:?}"),
    }
}

#[test]
fn catch_parses_body_and_lambda_handler() {
    let module = parse_ok("result ≔ ☊ (☄ DivisionByZero) λ(e) → 0\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Catch { body, handler, .. } => {
            assert!(matches!(**body, Expression::Raise(..)));
            assert!(matches!(**handler, Expression::Lambda { .. }));
        }
        other => panic!("expected a Catch, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// BaseType sigils vs Tag — `CONCRETE_SYMBOLIC_GRAMMAR.md` §9,
// `GLYPH_SYSTEM_DESIGN.md` §227-245. `BaseType` and `Tag` are genuinely
// distinct productions — `Int` written as a bare word is a `Tag`
// (an ADT reference), never an alias for `⟁`.
// ---------------------------------------------------------------------

#[test]
fn local_value_binding_parses_as_localvalue_with_its_continuation_as_body() {
    let module = parse_ok("result ≔ x ≔ 5\n x ✚ 1\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::LocalValue {
            decl: inner, body, ..
        } => {
            assert_eq!(inner.name, "x");
            assert!(matches!(inner.value, Expression::Int(5, _)));
            assert!(matches!(**body, Expression::BinaryOp { .. }));
        }
        other => panic!("expected a LocalValue, got {other:?}"),
    }
}

#[test]
fn nested_local_value_bindings_chain_through_body() {
    let module = parse_ok("result ≔ x ≔ 1\n y ≔ 2\n x ✚ y\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::LocalValue {
        decl: outer,
        body: outer_body,
        ..
    } = &decl.value
    else {
        panic!("expected an outer LocalValue, got {:?}", decl.value)
    };
    assert_eq!(outer.name, "x");
    match &**outer_body {
        Expression::LocalValue {
            decl: inner, body, ..
        } => {
            assert_eq!(inner.name, "y");
            assert!(matches!(**body, Expression::BinaryOp { .. }));
        }
        other => panic!("expected a nested LocalValue, got {other:?}"),
    }
}

#[test]
fn local_function_declaration_parses_as_localfunctions_with_one_member() {
    let module = parse_ok("result ≔ λf(n: ⟁): ⟁ → n\n f(5)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::LocalFunctions { decls, body, .. } => {
            assert_eq!(decls.len(), 1);
            assert_eq!(decls[0].name, "f");
            assert!(matches!(**body, Expression::Application { .. }));
        }
        other => panic!("expected a LocalFunctions, got {other:?}"),
    }
}

#[test]
fn adjacent_local_function_declarations_form_one_localfunctions_group() {
    let module = parse_ok("result ≔ λisEven(n: ⟁): ○ → n\n λisOdd(n: ⟁): ○ → n\n isEven(4)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::LocalFunctions { decls, .. } => {
            assert_eq!(decls.len(), 2);
            assert_eq!(decls[0].name, "isEven");
            assert_eq!(decls[1].name, "isOdd");
        }
        other => panic!("expected a LocalFunctions, got {other:?}"),
    }
}

#[test]
fn bare_lambda_followed_by_lparen_is_still_an_anonymous_lambda_not_a_local_function() {
    // The ambiguity this slice had to resolve: `λ(` (anonymous Lambda,
    // §7.3) vs `λname(` (local FunctionDeclaration, §8.2) — one token of
    // lookahead after `λ` disambiguates them.
    let module = parse_ok("result ≔ λ(n) → n\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Lambda { .. }));
}

#[test]
fn local_binding_export_modifier_is_rejected() {
    let msg = parse_err("result ≔ x ≔⟳ 5\n x\n❧");
    assert!(msg.contains("not valid on a local binding"), "{msg}");
}

// ── ArrayExpression (CONCRETE_SYMBOLIC_GRAMMAR.md §7.9) ────────────────

#[test]
fn array_literal_parses_its_elements() {
    let module = parse_ok("xs ≔ [1, 2, 3]\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::ArrayLiteral { elements, .. } => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(elements[0], Expression::Int(1, _)));
            assert!(matches!(elements[2], Expression::Int(3, _)));
        }
        other => panic!("expected an ArrayLiteral, got {other:?}"),
    }
}

#[test]
fn empty_array_literal_parses() {
    let module = parse_ok("xs ≔ []\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::ArrayLiteral { elements, .. } => assert!(elements.is_empty()),
        other => panic!("expected an ArrayLiteral, got {other:?}"),
    }
}

#[test]
fn index_expr_parses_array_and_index() {
    let module = parse_ok("xs ≔ [1, 2, 3]\nresult ≔ xs[0]\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Index { array, index, .. } => {
            assert!(matches!(**array, Expression::Reference(ref n, _) if n == "xs"));
            assert!(matches!(**index, Expression::Int(0, _)));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

#[test]
fn chained_indexing_parses_left_to_right() {
    let module = parse_ok("xs ≔ [[1, 2], [3, 4]]\nresult ≔ xs[0][1]\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Index { array, index, .. } => {
            assert!(matches!(**index, Expression::Int(1, _)));
            assert!(matches!(**array, Expression::Index { .. }));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

#[test]
fn indexing_composes_with_application() {
    let module = parse_ok("f ≔ λ(x) → x\nresult ≔ f([1, 2, 3])[0]\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Index { array, .. } => {
            assert!(matches!(**array, Expression::Application { .. }));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

// ── ImportDeclaration ────────────────────────────────────────────────
