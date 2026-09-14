//! Conformance tests for Surface AST → Core AST lowering (§16, §7/§12).

use obfusku_core::ast::{Binding, Expr, Literal};
use obfusku_diagnostics::SourceMap;
use obfusku_syntax::{desugar, lexer, parser};

/// Flattens every `BindingGroup` into its member `Binding`s, in order.
/// Tests that care about the `Let`/`LetRec` distinction inspect
/// `module.bindings` directly instead.
fn flat(module: &obfusku_core::ast::Module) -> Vec<&Binding> {
    user_groups(module)
        .iter()
        .flat_map(|g| g.bindings())
        .collect()
}

/// Every `desugar` output starts with five synthesized `Exception`
/// constructor bindings (§15.2, P0-D's `IntegerOverflow` included)
/// before the user's own source — skipped here so index-based
/// assertions mean "the user's first binding."
fn user_groups(module: &obfusku_core::ast::Module) -> &[obfusku_core::ast::BindingGroup] {
    &module.bindings[5..]
}

fn lower_ok(source: &str) -> obfusku_core::ast::Module {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    let surface = parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s): {ds:?}"));
    desugar::desugar(&surface).unwrap_or_else(|ds| panic!("desugar error(s): {ds:?}"))
}

fn lower_err(source: &str) -> String {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    let surface = parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s): {ds:?}"));
    match desugar::desugar(&surface) {
        Err(ds) => ds[0].message.clone(),
        Ok(m) => panic!("expected a desugar error, got a valid Core module: {m:?}"),
    }
}

#[test]
fn literal_lowers_to_lit() {
    let module = lower_ok("x ≔ 5\n❧");
    assert_eq!(user_groups(&module).len(), 1);
    assert!(matches!(
        flat(&module)[0].value,
        Expr::Lit(Literal::Int(5), _)
    ));
}

#[test]
fn reference_lowers_to_var() {
    // Every binding is lowered independently in this slice (no
    // cross-binding name resolution yet), so `y` on its own is a
    // perfectly good — if free — Var.
    let module = lower_ok("y ≔ x\n❧");
    assert!(matches!(&flat(&module)[0].value, Expr::Var(n, _) if n == "x"));
}

#[test]
fn export_flag_survives_lowering_as_plain_metadata() {
    let module = lower_ok("x ≔⟳ 5\n❧");
    assert!(flat(&module)[0].exported);
}

#[test]
fn single_param_lambda_lowers_directly() {
    let module = lower_ok("f ≔ λ(x) → x\n❧");
    match &flat(&module)[0].value {
        Expr::Lambda { param, body, .. } => {
            assert_eq!(param, "x");
            assert!(matches!(**body, Expr::Var(ref n, _) if n == "x"));
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn multi_param_lambda_curries_right_to_left() {
    // λ(x, y) → x desugars to Lambda(x, Lambda(y, Var(x))) —
    // ABSTRACT_GRAMMAR.md §3.5's curried-chain claim, checked for real.
    let module = lower_ok("f ≔ λ(x, y) → x\n❧");
    match &flat(&module)[0].value {
        Expr::Lambda {
            param: p1, body, ..
        } => {
            assert_eq!(p1, "x");
            match &**body {
                Expr::Lambda {
                    param: p2,
                    body: inner,
                    ..
                } => {
                    assert_eq!(p2, "y");
                    assert!(matches!(**inner, Expr::Var(ref n, _) if n == "x"));
                }
                other => panic!("expected the outer Lambda's body to be a Lambda, got {other:?}"),
            }
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn single_arg_application_lowers_directly() {
    let module = lower_ok("y ≔ f(5)\n❧");
    match &flat(&module)[0].value {
        Expr::Apply { func, arg, .. } => {
            assert!(matches!(**func, Expr::Var(ref n, _) if n == "f"));
            assert!(matches!(**arg, Expr::Lit(Literal::Int(5), _)));
        }
        other => panic!("expected an Apply, got {other:?}"),
    }
}

#[test]
fn multi_arg_application_curries_left_to_right() {
    // f(a, b, c) desugars to Apply(Apply(Apply(f, a), b), c) —
    // ABSTRACT_GRAMMAR.md §5's curried-chain claim, checked for real.
    let module = lower_ok("y ≔ f(1, 2, 3)\n❧");
    match &flat(&module)[0].value {
        Expr::Apply {
            func: outer_func,
            arg: c,
            ..
        } => {
            assert!(matches!(**c, Expr::Lit(Literal::Int(3), _)));
            match &**outer_func {
                Expr::Apply {
                    func: mid_func,
                    arg: b,
                    ..
                } => {
                    assert!(matches!(**b, Expr::Lit(Literal::Int(2), _)));
                    match &**mid_func {
                        Expr::Apply {
                            func: inner_func,
                            arg: a,
                            ..
                        } => {
                            assert!(matches!(**a, Expr::Lit(Literal::Int(1), _)));
                            assert!(matches!(**inner_func, Expr::Var(ref n, _) if n == "f"));
                        }
                        other => panic!("expected innermost Apply, got {other:?}"),
                    }
                }
                other => panic!("expected middle Apply, got {other:?}"),
            }
        }
        other => panic!("expected outer Apply, got {other:?}"),
    }
}

// ── P1-1: `Argument ::= Expression | "•"` desugaring (§7.2/§12.1) ──────

#[test]
fn a_single_hole_desugars_to_a_lambda_wrapping_the_curried_apply_chain() {
    // f(1, •, 3) desugars to λ(h) → Apply(Apply(Apply(f, 1), h), 3) —
    // the hole itself never opens a new lambda scope; this one Lambda
    // wraps the whole existing curried chain instead.
    let module = lower_ok("y ≔ f(1, •, 3)\n❧");
    match &flat(&module)[0].value {
        Expr::Lambda { param, body, .. } => match &**body {
            Expr::Apply {
                func: outer_func,
                arg: c,
                ..
            } => {
                assert!(matches!(**c, Expr::Lit(Literal::Int(3), _)));
                match &**outer_func {
                    Expr::Apply {
                        func: inner_func,
                        arg: hole_ref,
                        ..
                    } => {
                        assert!(matches!(**hole_ref, Expr::Var(ref n, _) if n == param));
                        match &**inner_func {
                            Expr::Apply {
                                func: f_ref,
                                arg: a,
                                ..
                            } => {
                                assert!(matches!(**a, Expr::Lit(Literal::Int(1), _)));
                                assert!(matches!(**f_ref, Expr::Var(ref n, _) if n == "f"));
                            }
                            other => panic!("expected innermost Apply, got {other:?}"),
                        }
                    }
                    other => panic!("expected middle Apply, got {other:?}"),
                }
            }
            other => panic!("expected outer Apply inside the Lambda, got {other:?}"),
        },
        other => panic!("expected a Lambda wrapping the hole, got {other:?}"),
    }
}

#[test]
fn two_holes_curry_left_to_right_the_first_hole_is_the_outer_lambda() {
    // f(•, 2, •) desugars to λ(h0) → λ(h1) → f(h0, 2, h1) — proving
    // this builds a genuine two-parameter curried closure (matching
    // ordinary multi-param Lambda desugaring), not two independent
    // one-hole-one-lambda wrappers glued together ad hoc.
    let module = lower_ok("y ≔ f(•, 2, •)\n❧");
    match &flat(&module)[0].value {
        Expr::Lambda {
            param: h0,
            body: outer_body,
            ..
        } => match &**outer_body {
            Expr::Lambda {
                param: h1,
                body: inner_body,
                ..
            } => {
                assert_ne!(h0, h1, "the two holes must bind distinct names");
                match &**inner_body {
                    Expr::Apply {
                        func: mid,
                        arg: second_hole_ref,
                        ..
                    } => {
                        assert!(matches!(**second_hole_ref, Expr::Var(ref n, _) if n == h1));
                        match &**mid {
                            Expr::Apply {
                                func: inner,
                                arg: two,
                                ..
                            } => {
                                assert!(matches!(**two, Expr::Lit(Literal::Int(2), _)));
                                match &**inner {
                                    Expr::Apply {
                                        func: f_ref,
                                        arg: first_hole_ref,
                                        ..
                                    } => {
                                        assert!(
                                            matches!(**first_hole_ref, Expr::Var(ref n, _) if n == h0)
                                        );
                                        assert!(matches!(**f_ref, Expr::Var(ref n, _) if n == "f"));
                                    }
                                    other => panic!("expected innermost Apply, got {other:?}"),
                                }
                            }
                            other => panic!("expected the literal-2 Apply, got {other:?}"),
                        }
                    }
                    other => panic!("expected the second-hole Apply, got {other:?}"),
                }
            }
            other => panic!("expected a nested Lambda for the second hole, got {other:?}"),
        },
        other => panic!("expected a Lambda for the first hole, got {other:?}"),
    }
}

#[test]
fn a_call_with_no_holes_is_completely_unaffected() {
    let module = lower_ok("y ≔ f(1, 2)\n❧");
    assert!(!matches!(&flat(&module)[0].value, Expr::Lambda { .. }));
}

#[test]
fn bare_callable_pipe_lowers_to_direct_apply() {
    // x ▷ f  ==  Apply(f, x), no lambda wrapper at all —
    // SEMANTIC_CORE.md §7's first case.
    let module = lower_ok("z ≔ xs ▷ f\n❧");
    match &flat(&module)[0].value {
        Expr::Apply { func, arg, .. } => {
            assert!(matches!(**func, Expr::Var(ref n, _) if n == "f"));
            assert!(matches!(**arg, Expr::Var(ref n, _) if n == "xs"));
        }
        other => panic!("expected a direct Apply with no Lambda wrapper, got {other:?}"),
    }
}

#[test]
fn expression_stage_pipe_introduces_a_fresh_binder() {
    // x ▷ (◈)  ==  Apply(Lambda($pipeN, Var($pipeN)), x) —
    // SEMANTIC_CORE.md §7's second case.
    let module = lower_ok("z ≔ xs ▷ (◈)\n❧");
    match &flat(&module)[0].value {
        Expr::Apply { func, arg, .. } => {
            assert!(matches!(**arg, Expr::Var(ref n, _) if n == "xs"));
            match &**func {
                Expr::Lambda { param, body, .. } => {
                    assert!(matches!(**body, Expr::Var(ref n, _) if n == param));
                }
                other => panic!("expected a Lambda wrapper, got {other:?}"),
            }
        }
        other => panic!("expected an Apply, got {other:?}"),
    }
}

#[test]
fn nested_pipe_reference_binds_to_its_own_stage_not_the_outer_one() {
    // In `xs ▷ (◈ ▷ (◈ ✚ ...))`, the innermost `◈` must resolve to the
    // inner pipe's binder, never the outer one.
    let module = lower_ok("z ≔ xs ▷ (◈ ▷ (◈))\n❧");
    match &flat(&module)[0].value {
        Expr::Apply {
            func: outer_lambda, ..
        } => match &**outer_lambda {
            Expr::Lambda {
                param: outer_param,
                body: outer_body,
                ..
            } => match &**outer_body {
                // outer_body is itself the lowering of "◈ ▷ (◈)", i.e.
                // Apply(Lambda(inner_param, Var(inner_param)), Var(outer_param))
                Expr::Apply {
                    func: inner_lambda,
                    arg: outer_ref,
                    ..
                } => {
                    assert!(
                        matches!(**outer_ref, Expr::Var(ref n, _) if n == outer_param),
                        "the pipe subject reference must use the OUTER binder"
                    );
                    match &**inner_lambda {
                        Expr::Lambda {
                            param: inner_param,
                            body: inner_body,
                            ..
                        } => {
                            assert_ne!(
                                inner_param, outer_param,
                                "inner and outer pipe stages must introduce distinct fresh binders"
                            );
                            assert!(
                                matches!(**inner_body, Expr::Var(ref n, _) if n == inner_param),
                                "the innermost '◈' must resolve to the INNER binder, not the outer one"
                            );
                        }
                        other => panic!("expected inner Lambda, got {other:?}"),
                    }
                }
                other => panic!("expected outer body to be an Apply, got {other:?}"),
            },
            other => panic!("expected outer Lambda, got {other:?}"),
        },
        other => panic!("expected outer Apply, got {other:?}"),
    }
}

#[test]
fn pipe_ref_propagates_through_a_lambda_body_inside_a_stage() {
    // "◈" is an ordinary bound variable reference once lowered — nothing
    // shadows it except another pipe's own fresh binder, so it must
    // still resolve correctly even from inside a nested lambda body.
    let module = lower_ok("z ≔ xs ▷ (g(λ(y) → ◈))\n❧");
    match &flat(&module)[0].value {
        Expr::Apply {
            func: outer_lambda, ..
        } => match &**outer_lambda {
            Expr::Lambda { param, body, .. } => {
                // body == Apply(Var(g), Lambda(y, Var(param)))
                match &**body {
                    Expr::Apply {
                        arg: inner_lambda, ..
                    } => match &**inner_lambda {
                        Expr::Lambda {
                            body: innermost, ..
                        } => {
                            assert!(matches!(**innermost, Expr::Var(ref n, _) if n == param));
                        }
                        other => panic!("expected inner Lambda, got {other:?}"),
                    },
                    other => panic!("expected Apply(g, lambda), got {other:?}"),
                }
            }
            other => panic!("expected outer Lambda, got {other:?}"),
        },
        other => panic!("expected outer Apply, got {other:?}"),
    }
}

// ── rejected lowerings ──────────────────────────────────────────────────

#[test]
fn pipe_ref_outside_any_pipeline_is_rejected() {
    let msg = lower_err("x ≔ ◈\n❧");
    assert!(
        msg.contains("outside of a pipeline stage"),
        "message was: {msg}"
    );
}

// Zero-arity rejection now happens at parse time
// (CONCRETE_SYMBOLIC_GRAMMAR.md §7.2/§7.3) — see
// `zero_parameter_lambda_is_rejected` and
// `zero_argument_application_is_rejected` in `tests/parser.rs`.

// ── mutability (SEMANTIC_CORE.md §12/§12.1/§13) ─────────────────────────

#[test]
fn immutable_binding_still_lowers_normally() {
    // Regression: adding mutability support must not disturb this.
    let module = lower_ok("x ≔ 5\n❧");
    assert!(matches!(
        flat(&module)[0].value,
        Expr::Lit(Literal::Int(5), _)
    ));
}

#[test]
fn mutable_binding_lowers_to_mutcell() {
    let module = lower_ok("x ≔˚ 5\n❧");
    match &flat(&module)[0].value {
        Expr::MutCell { initial, .. } => {
            assert!(matches!(**initial, Expr::Lit(Literal::Int(5), _)));
        }
        other => panic!("expected a MutCell, got {other:?}"),
    }
}

#[test]
fn mutable_and_immutable_bindings_are_distinguishable_by_core_shape() {
    // No separate boolean flag on Binding — mutability is entirely
    // represented by whether the value is MutCell-wrapped, matching
    // SEMANTIC_CORE.md's own framing of mutability as a property of the
    // value's shape, not a side tag.
    let module = lower_ok("x ≔ 1\ny ≔˚ 1\n❧");
    assert!(!matches!(flat(&module)[0].value, Expr::MutCell { .. }));
    assert!(matches!(flat(&module)[1].value, Expr::MutCell { .. }));
}

#[test]
fn ordinary_reference_to_a_mutable_binding_lowers_to_mutread() {
    let module = lower_ok("x ≔˚ 5\ny ≔ x\n❧");
    match &flat(&module)[1].value {
        Expr::MutRead { cell, .. } => {
            assert!(matches!(**cell, Expr::Var(ref n, _) if n == "x"));
        }
        other => panic!("expected a MutRead, got {other:?}"),
    }
}

#[test]
fn explicit_reference_form_never_reads() {
    // ˚x always lowers to bare Var(x). Parenthesized: "≔" immediately
    // followed by "˚" is always the Mutability modifier, never a bare
    // ReferenceForm value, so this position needs parens.
    let module = lower_ok("x ≔˚ 5\ny ≔ (˚x)\n❧");
    assert!(matches!(&flat(&module)[1].value, Expr::Var(n, _) if n == "x"));
}

#[test]
fn rebind_lowers_to_mutrebind_against_the_raw_cell() {
    // Plain (non-explicit) spelling: valid here because this rebind is
    // NOT inside any Lambda — no closure boundary is being crossed, so
    // no explicit capture is required (SEMANTIC_CORE.md §13 only
    // constrains references crossing INTO a closure).
    let module = lower_ok("x ≔˚ 5\ny ≔ x ⚙︎ 9\n❧");
    match &flat(&module)[1].value {
        Expr::MutRebind {
            cell, new_value, ..
        } => {
            assert!(matches!(**cell, Expr::Var(ref n, _) if n == "x"));
            assert!(matches!(**new_value, Expr::Lit(Literal::Int(9), _)));
        }
        other => panic!("expected a MutRebind, got {other:?}"),
    }
}

#[test]
fn mutable_capture_follows_frozen_capture_semantics() {
    // SEMANTIC_CORE.md §13: explicit ('˚') capture is what makes
    // mutation-through-closure representable at all. `˚counter ⚙︎ step`
    // is the single-occurrence idiom that both captures and mutates —
    // this must lower cleanly, with no static rejection.
    let module = lower_ok("counter ≔˚ 0\nbump ≔ λ(step) → ˚counter ⚙︎ step\n❧");
    match &flat(&module)[1].value {
        Expr::Lambda { body, .. } => {
            assert!(matches!(**body, Expr::MutRebind { .. }));
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn mutation_through_a_default_captured_reference_is_rejected_statically() {
    // §13: rebinding a mut-bound free variable not explicitly
    // '˚'-captured is a static error — the default-capture reference
    // desugars to a read of the cell's contents, not the cell itself,
    // and `⚙︎` needs the cell.
    let msg = lower_err("counter ≔˚ 0\nbump ≔ λ(step) → counter ⚙︎ step\n❧");
    assert!(msg.contains("desugars to a read"), "message was: {msg}");
}

#[test]
fn rebind_of_a_never_mutable_name_is_rejected() {
    let msg = lower_err("x ≔ 5\ny ≔ λ(z) → ˚x ⚙︎ z\n❧");
    assert!(msg.contains("not a mutable binding"), "message was: {msg}");
}

#[test]
fn nested_lambda_capture_is_not_inherited_from_the_outer_one() {
    // SEMANTIC_CORE.md §13: "nesting does not propagate reference-capture
    // status." The OUTER lambda explicitly captures x (and rebinds it),
    // but the INNER lambda's own plain rebind of the same free variable
    // must still fail — it never captured x itself.
    let msg = lower_err("x ≔˚ 0\nf ≔ λ(a) → ˚x ⚙︎ a ▷ (λ(b) → x ⚙︎ b)\n❧");
    assert!(msg.contains("desugars to a read"), "message was: {msg}");
}

// ── ADTs, Match, and Pattern lowering (SEMANTIC_CORE.md §9/§10) ────────

#[test]
fn nullary_variant_lowers_to_a_bare_constructor_value_no_lambda_wrapper() {
    let module = lower_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    let none_binding = flat(&module)
        .into_iter()
        .find(|b| b.name == "None")
        .unwrap();
    match &none_binding.value {
        Expr::Constructor { tag, args, .. } => {
            assert_eq!(tag, "None");
            assert!(args.is_empty());
        }
        other => panic!("expected a bare Constructor value, got {other:?}"),
    }
}

#[test]
fn unary_variant_lowers_to_a_single_lambda_wrapping_constructor() {
    let module = lower_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    let some_binding = flat(&module)
        .into_iter()
        .find(|b| b.name == "Some")
        .unwrap();
    match &some_binding.value {
        Expr::Lambda { param, body, .. } => match &**body {
            Expr::Constructor { tag, args, .. } => {
                assert_eq!(tag, "Some");
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::Var(n, _) if *n == *param));
            }
            other => panic!("expected Constructor body, got {other:?}"),
        },
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn constructor_reference_is_ordinary_application_after_lowering() {
    // Circle(5) lowers through the SAME Apply path as any other call —
    // reinforcing SEMANTIC_CORE.md §9's "ordinary functions" framing at
    // the Core level too, not just the surface grammar.
    let module = lower_ok("Shape ≔ { Circle(⧆) }\nx ≔ Circle(5)\n❧");
    let x = flat(&module).into_iter().find(|b| b.name == "x").unwrap();
    match &x.value {
        Expr::Apply { func, arg, .. } => {
            assert!(matches!(**func, Expr::Var(ref n, _) if n == "Circle"));
            assert!(matches!(**arg, Expr::Lit(Literal::Int(5), _)));
        }
        other => panic!("expected an Apply, got {other:?}"),
    }
}

#[test]
fn match_and_patterns_lower_structurally() {
    let module = lower_ok(
        r#"
        Option t ≔ { Some(t) ⟢ None }
        y ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        "#,
    );
    let y = flat(&module).into_iter().find(|b| b.name == "y").unwrap();
    match &y.value {
        Expr::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expr::Var(ref n, _) if n == "x"));
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                obfusku_core::ast::Pattern::Constructor { tag, args, .. } => {
                    assert_eq!(tag, "Some");
                    assert!(matches!(&args[0], obfusku_core::ast::Pattern::Var(n, _) if n == "n"));
                }
                other => panic!("expected a Constructor pattern, got {other:?}"),
            }
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn type_decl_is_recorded_on_the_core_module() {
    let module = lower_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    assert_eq!(module.type_decls.len(), 1);
    assert_eq!(module.type_decls[0].tag, "Option");
    assert_eq!(module.type_decls[0].type_params, vec!["t".to_string()]);
    assert_eq!(module.type_decls[0].variants.len(), 2);
}

#[test]
fn result_type_decl_records_two_independent_type_parameters() {
    let module = lower_ok("Result t e ≔ { Ok(t) ⟢ Err(e) }\n❧");
    let td = &module.type_decls[0];
    assert_eq!(td.type_params, vec!["t".to_string(), "e".to_string()]);
    // Ok's field references only "t", Err's only "e" — each variant
    // uses its own subset of the ADT's declared parameters, nothing
    // forces every variant to mention every parameter.
    match &td.variants[0].fields[0] {
        obfusku_core::types::Type::Param(p) => assert_eq!(p, "t"),
        other => panic!("expected Param(\"t\"), got {other:?}"),
    }
    match &td.variants[1].fields[0] {
        obfusku_core::types::Type::Param(p) => assert_eq!(p, "e"),
        other => panic!("expected Param(\"e\"), got {other:?}"),
    }
}

#[test]
fn ok_and_err_lower_to_ordinary_single_lambda_constructor_bindings() {
    // Exactly the same synthesis path already proven for Option's Some
    // — no Result-specific lowering code exists to test separately.
    let module = lower_ok("Result t e ≔ { Ok(t) ⟢ Err(e) }\n❧");
    let ok_binding = flat(&module).into_iter().find(|b| b.name == "Ok").unwrap();
    let err_binding = flat(&module).into_iter().find(|b| b.name == "Err").unwrap();
    for (b, tag) in [(ok_binding, "Ok"), (err_binding, "Err")] {
        match &b.value {
            Expr::Lambda { body, .. } => match &**body {
                Expr::Constructor { tag: t, args, .. } => {
                    assert_eq!(t, tag);
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected Constructor body, got {other:?}"),
            },
            other => panic!("expected a Lambda for {tag}, got {other:?}"),
        }
    }
}

#[test]
fn mutcell_allocation_is_not_a_syntactic_value() {
    // §19.1: generalizable only if the value is a syntactic value.
    // MutCell is Apply-shaped (an allocation), never one.
    let module = lower_ok("x ≔˚ 5\n❧");
    fn is_syntactic_value(e: &Expr) -> bool {
        matches!(e, Expr::Lit(..) | Expr::Var(..) | Expr::Lambda { .. })
    }
    assert!(
        !is_syntactic_value(&flat(&module)[0].value),
        "MutCell must not be classified as a syntactic value"
    );
    assert!(matches!(flat(&module)[0].value, Expr::MutCell { .. }));
}

// ---------------------------------------------------------------------
// FunctionDeclaration adjacency → LetRec grouping (§8.2): group
// membership is a pure function of declaration kinds, never bodies.
// ---------------------------------------------------------------------

#[test]
fn two_adjacent_function_declarations_form_one_letrec_group() {
    let module = lower_ok("λf(x: ⟁): ⟁ → x\nλg(x: ⟁): ⟁ → x\n❧");
    assert_eq!(
        user_groups(&module).len(),
        1,
        "expected exactly one BindingGroup"
    );
    match &user_groups(&module)[0] {
        obfusku_core::ast::BindingGroup::LetRec(members) => {
            assert_eq!(members.len(), 2);
            assert_eq!(members[0].name, "f");
            assert_eq!(members[1].name, "g");
        }
        other => panic!("expected a LetRec group, got {other:?}"),
    }
}

#[test]
fn exported_function_declaration_carries_exported_true_into_its_letrec_binding() {
    let module = lower_ok("λf(x: ⟁): ⟁ →⟳ x\n❧");
    match &user_groups(&module)[0] {
        obfusku_core::ast::BindingGroup::LetRec(members) => {
            assert_eq!(members.len(), 1);
            assert!(members[0].exported);
        }
        other => panic!("expected a LetRec group, got {other:?}"),
    }
}

#[test]
fn non_exported_function_declaration_carries_exported_false_into_its_letrec_binding() {
    let module = lower_ok("λf(x: ⟁): ⟁ → x\n❧");
    match &user_groups(&module)[0] {
        obfusku_core::ast::BindingGroup::LetRec(members) => {
            assert_eq!(members.len(), 1);
            assert!(!members[0].exported);
        }
        other => panic!("expected a LetRec group, got {other:?}"),
    }
}

#[test]
fn value_declaration_annotation_becomes_the_bindings_declared_type() {
    let module = lower_ok("x: \u{27C1} \u{2254} 5\n\u{2767}\n");
    assert_eq!(
        flat(&module)[0].declared_type,
        Some(obfusku_core::types::Type::Int)
    );
}

#[test]
fn value_declaration_without_annotation_has_no_declared_type() {
    let module = lower_ok("x \u{2254} 5\n\u{2767}\n");
    assert_eq!(flat(&module)[0].declared_type, None);
}

#[test]
fn mutable_value_declaration_annotation_names_the_element_type_not_cell_of_it() {
    // The surface annotation is on the element (`x : Int \u{2254}\u{02da} 0`,
    // not `x : Cell<Int> \u{2254}\u{02da} 0`) but the binding's own `value` is
    // already `MutCell`-wrapped, so `declared_type` must be wrapped the
    // same way or it's a guaranteed, spurious mismatch against every
    // correctly-annotated mutable declaration.
    let module = lower_ok("x: \u{27C1} \u{2254}\u{02da} 0\n\u{2767}\n");
    assert_eq!(
        flat(&module)[0].declared_type,
        Some(obfusku_core::types::Type::Cell(Box::new(
            obfusku_core::types::Type::Int
        )))
    );
}

#[test]
fn a_value_declaration_between_two_function_declarations_splits_the_group() {
    let module = lower_ok("λf(x: ⟁): ⟁ → x\nvalue ≔ 1\nλg(x: ⟁): ⟁ → x\n❧");
    assert_eq!(
        user_groups(&module).len(),
        3,
        "expected three separate BindingGroups"
    );
    assert!(matches!(
        &user_groups(&module)[0],
        obfusku_core::ast::BindingGroup::LetRec(members) if members.len() == 1 && members[0].name == "f"
    ));
    assert!(matches!(
        &user_groups(&module)[1],
        obfusku_core::ast::BindingGroup::Let(b) if b.name == "value"
    ));
    assert!(matches!(
        &user_groups(&module)[2],
        obfusku_core::ast::BindingGroup::LetRec(members) if members.len() == 1 && members[0].name == "g"
    ));
}

#[test]
fn a_type_declaration_between_two_function_declarations_also_splits_the_group() {
    let module = lower_ok("λf(x: ⟁): ⟁ → x\nOption t ≔ { Some(t) ⟢ None }\nλg(x: ⟁): ⟁ → x\n❧");
    // type_decls is recorded separately from bindings, but the sum of
    // constructor bindings + f's group + g's group must still show two
    // distinct LetRec groups, not one merged group.
    let letrec_groups: Vec<&Vec<obfusku_core::ast::Binding>> = module
        .bindings
        .iter()
        .filter_map(|g| match g {
            obfusku_core::ast::BindingGroup::LetRec(members) => Some(members),
            _ => None,
        })
        .collect();
    assert_eq!(
        letrec_groups.len(),
        2,
        "expected f and g in separate groups"
    );
    assert_eq!(letrec_groups[0].len(), 1);
    assert_eq!(letrec_groups[0][0].name, "f");
    assert_eq!(letrec_groups[1].len(), 1);
    assert_eq!(letrec_groups[1][0].name, "g");
}

#[test]
fn adjacency_alone_groups_functions_that_never_reference_each_other() {
    // f and g share nothing, yet must land in the same LetRec group —
    // adjacency alone is the signal, never a reference.
    let module = lower_ok("λf(x: ⟁): ⟁ → 1\nλg(x: ⟁): ⟁ → 2\n❧");
    assert_eq!(user_groups(&module).len(), 1);
    assert!(matches!(
        &user_groups(&module)[0],
        obfusku_core::ast::BindingGroup::LetRec(members) if members.len() == 2
    ));
}

#[test]
fn a_reference_between_two_non_adjacent_function_declarations_does_not_merge_their_groups() {
    // f references g, but a ValueDeclaration sits between them, so they
    // land in separate groups despite the reference (grouping only
    // checked here, not that f type-checks).
    let module = lower_ok("λf(x: ⟁): ⟁ → g(x)\nvalue ≔ 1\nλg(x: ⟁): ⟁ → x\n❧");
    assert_eq!(user_groups(&module).len(), 3);
    assert!(matches!(
        &user_groups(&module)[0],
        obfusku_core::ast::BindingGroup::LetRec(members) if members.len() == 1 && members[0].name == "f"
    ));
    assert!(matches!(
        &user_groups(&module)[2],
        obfusku_core::ast::BindingGroup::LetRec(members) if members.len() == 1 && members[0].name == "g"
    ));
}

#[test]
fn a_single_function_declaration_forms_a_one_member_letrec_group_enabling_self_recursion() {
    let module = lower_ok("λf(x: ⟁): ⟁ → f(x)\n❧");
    assert!(matches!(
        &user_groups(&module)[0],
        obfusku_core::ast::BindingGroup::LetRec(members) if members.len() == 1 && members[0].name == "f"
    ));
}

#[test]
fn a_value_declaration_is_never_a_letrec_group_even_when_its_value_is_a_lambda() {
    // The load-bearing case this whole slice exists to enforce: a bare
    // Lambda-valued ValueDeclaration must desugar to an ordinary `Let`,
    // never a `LetRec` — recursion eligibility comes from declaration
    // form, not from the shape of the desugared value.
    let module = lower_ok("f ≔ λ(x) → x\n❧");
    assert!(matches!(
        &user_groups(&module)[0],
        obfusku_core::ast::BindingGroup::Let(b) if b.name == "f"
    ));
}

// ---------------------------------------------------------------------
// Operator lowering — surface `BinaryOp`/`UnaryOp` → Core
// (`SEMANTIC_CORE.md` §20.2). Typing/runtime evaluation are a separate,
// later slice; this only proves source → AST → Core preserves the
// operator and precedence structure the parser already committed to.
// ---------------------------------------------------------------------

use obfusku_core::ast::{BinOp as CoreBinOp, UnOp as CoreUnOp};

#[test]
fn arithmetic_operator_lowers_to_core_binaryop_with_matching_op() {
    let module = lower_ok("x ≔ 1 ✚ 2\n❧");
    match &flat(&module)[0].value {
        Expr::BinaryOp { op, lhs, rhs, .. } => {
            assert_eq!(*op, CoreBinOp::Add);
            assert!(matches!(**lhs, Expr::Lit(Literal::Int(1), _)));
            assert!(matches!(**rhs, Expr::Lit(Literal::Int(2), _)));
        }
        other => panic!("expected a BinaryOp, got {other:?}"),
    }
}

#[test]
fn every_non_short_circuit_binop_lowers_to_the_matching_core_op() {
    let cases: &[(&str, CoreBinOp)] = &[
        ("1 ☠︎ 2", CoreBinOp::Sub),
        ("1 ✱ 2", CoreBinOp::Mul),
        ("1 ÷ 2", CoreBinOp::Div),
        ("1 ⌗ 2", CoreBinOp::Mod),
        ("1 < 2", CoreBinOp::Lt),
        ("1 > 2", CoreBinOp::Gt),
        ("1 <= 2", CoreBinOp::Le),
        ("1 >= 2", CoreBinOp::Ge),
        ("1 == 2", CoreBinOp::Eq),
        ("1 != 2", CoreBinOp::NotEq),
        ("◉ ⊻ ◎", CoreBinOp::Xor),
    ];
    for (src, expected) in cases {
        let module = lower_ok(&format!("x ≔ {src}\n❧"));
        match &flat(&module)[0].value {
            Expr::BinaryOp { op, .. } => assert_eq!(op, expected, "for {src:?}"),
            other => panic!("expected a BinaryOp for {src:?}, got {other:?}"),
        }
    }
}

#[test]
fn unary_operators_lower_to_core_unaryop() {
    let module = lower_ok("x ≔ ¬◉\n❧");
    match &flat(&module)[0].value {
        Expr::UnaryOp { op, operand, .. } => {
            assert_eq!(*op, CoreUnOp::Not);
            assert!(matches!(**operand, Expr::Lit(Literal::Bool(true), _)));
        }
        other => panic!("expected a UnaryOp, got {other:?}"),
    }

    let module2 = lower_ok("x ≔ −5\n❧");
    match &flat(&module2)[0].value {
        Expr::UnaryOp { op, .. } => assert_eq!(*op, CoreUnOp::Neg),
        other => panic!("expected a UnaryOp, got {other:?}"),
    }
}

#[test]
fn precedence_structure_survives_lowering_to_core() {
    // 1 ✚ 2 ✱ 3  must still be Add(1, Mul(2, 3)) once in Core, not
    // reassociated or flattened during lowering.
    let module = lower_ok("x ≔ 1 ✚ 2 ✱ 3\n❧");
    match &flat(&module)[0].value {
        Expr::BinaryOp { op, lhs, rhs, .. } => {
            assert_eq!(*op, CoreBinOp::Add);
            assert!(matches!(**lhs, Expr::Lit(Literal::Int(1), _)));
            match &**rhs {
                Expr::BinaryOp { op: inner_op, .. } => assert_eq!(*inner_op, CoreBinOp::Mul),
                other => panic!("expected inner BinaryOp, got {other:?}"),
            }
        }
        other => panic!("expected a BinaryOp, got {other:?}"),
    }
}

#[test]
fn and_never_reaches_core_as_a_binaryop_it_becomes_match() {
    // `∧` must lower to `Expr::Match`, never `Expr::BinaryOp` (§20.2) —
    // no `BinaryOp` node should appear anywhere in the tree.
    let module = lower_ok("x ≔ ◉ ∧ ◎\n❧");
    let value = &flat(&module)[0].value;
    assert!(
        !contains_binaryop(value),
        "∧ must never lower to Expr::BinaryOp anywhere in the tree, got {value:?}"
    );
    match value {
        Expr::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expr::Lit(Literal::Bool(true), _)));
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                arms[0].pattern,
                obfusku_core::ast::Pattern::Lit(Literal::Bool(true), _)
            ));
            assert!(matches!(
                arms[1].pattern,
                obfusku_core::ast::Pattern::Lit(Literal::Bool(false), _)
            ));
            // Zero(false) arm's own result must be the second operand,
            // not a further-nested BinaryOp.
            assert!(matches!(arms[0].result, Expr::Lit(Literal::Bool(false), _)));
            assert!(matches!(arms[1].result, Expr::Lit(Literal::Bool(false), _)));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn or_never_reaches_core_as_a_binaryop_it_becomes_match() {
    let module = lower_ok("x ≔ ◎ ∨ ◉\n❧");
    let value = &flat(&module)[0].value;
    assert!(
        !contains_binaryop(value),
        "∨ must never lower to Expr::BinaryOp, got {value:?}"
    );
    assert!(matches!(value, Expr::Match { .. }));
}

#[test]
fn and_or_short_circuit_structure_names_the_correct_operand_as_the_conditional_branch() {
    // a ∧ b: True arm evaluates to b (the second operand); False arm is
    // the literal False, never touching b — this is what makes it
    // short-circuit once evaluated, checked here at the Core-shape level
    // (evaluation itself is a later slice).
    let module = lower_ok("x ≔ p ∧ q\n❧");
    match &flat(&module)[0].value {
        Expr::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expr::Var(ref n, _) if n == "p"));
            assert!(matches!(arms[0].result, Expr::Var(ref n, _) if n == "q"));
            assert!(matches!(arms[1].result, Expr::Lit(Literal::Bool(false), _)));
        }
        other => panic!("expected a Match, got {other:?}"),
    }

    let module2 = lower_ok("y ≔ p ∨ q\n❧");
    match &flat(&module2)[0].value {
        Expr::Match { arms, .. } => {
            assert!(matches!(arms[0].result, Expr::Lit(Literal::Bool(true), _)));
            assert!(matches!(arms[1].result, Expr::Var(ref n, _) if n == "q"));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

/// Recursively checks whether any `Expr::BinaryOp` appears anywhere in
/// the tree — used specifically to prove `∧`/`∨` leave no trace of
/// themselves as `BinaryOp` after lowering, not just that the top-level
/// node happens to be `Match`.
fn contains_binaryop(expr: &Expr) -> bool {
    match expr {
        Expr::BinaryOp { .. } => true,
        Expr::UnaryOp { operand, .. } => contains_binaryop(operand),
        Expr::Apply { func, arg, .. } => contains_binaryop(func) || contains_binaryop(arg),
        Expr::Lambda { body, .. } => contains_binaryop(body),
        Expr::MutCell { initial, .. } => contains_binaryop(initial),
        Expr::MutRead { cell, .. } => contains_binaryop(cell),
        Expr::MutRebind {
            cell, new_value, ..
        } => contains_binaryop(cell) || contains_binaryop(new_value),
        Expr::Constructor { args, .. } => args.iter().any(contains_binaryop),
        Expr::Match {
            scrutinee, arms, ..
        } => contains_binaryop(scrutinee) || arms.iter().any(|a| contains_binaryop(&a.result)),
        Expr::Raise { value, .. } => contains_binaryop(value),
        Expr::Catch {
            body, handler_body, ..
        } => contains_binaryop(body) || contains_binaryop(handler_body),
        Expr::Var(..) | Expr::Lit(..) => false,
        Expr::Let { value, body, .. } => contains_binaryop(value) || contains_binaryop(body),
        Expr::LetRec { bindings, body, .. } => {
            bindings.iter().any(|b| contains_binaryop(&b.value)) || contains_binaryop(body)
        }
        Expr::ArrayLiteral { elements, .. } => elements.iter().any(contains_binaryop),
        Expr::Index { array, index, .. } => contains_binaryop(array) || contains_binaryop(index),
    }
}

// ---------------------------------------------------------------------
// Raise / Catch lowering (`SEMANTIC_CORE.md` §15.2's built-in `Exception`
// constructors, synthesized just like a user ADT's).
// ---------------------------------------------------------------------

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
        r#"
        Point ≔ { x: ⧆ ⟢ y: ⧆ }
        p ≔ Point { x: 1.0 ⟢ y: 2.0 }
        q ≔ ⟡ p {
          Point { y: b ⟢ x: a } → a
        }
        ❧
        "#,
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

#[test]
fn array_literal_lowers_to_core_arrayliteral() {
    let module = lower_ok("xs ≔ [1, 2, 3]\n❧");
    let bindings = flat(&module);
    let xs = bindings.iter().find(|b| b.name == "xs").unwrap();
    match &xs.value {
        Expr::ArrayLiteral { elements, .. } => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(elements[0], Expr::Lit(Literal::Int(1), _)));
        }
        other => panic!("expected an ArrayLiteral, got {other:?}"),
    }
}

#[test]
fn index_expr_lowers_to_core_index() {
    let module = lower_ok("xs ≔ [1, 2, 3]\nresult ≔ xs[0]\n❧");
    let bindings = flat(&module);
    let result = bindings.iter().find(|b| b.name == "result").unwrap();
    match &result.value {
        Expr::Index { array, index, .. } => {
            assert!(matches!(**array, Expr::Var(ref n, _) if n == "xs"));
            assert!(matches!(**index, Expr::Lit(Literal::Int(0), _)));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

// ── LocalBinding lowering (`SEMANTIC_CORE.md` §11: `Let`/`LetRec` are
//    genuine Core expression forms with an explicit `body`) ───────────

#[test]
fn local_value_binding_lowers_to_core_let_with_its_continuation_as_body() {
    let module = lower_ok("result ≔ x ≔ 5\n x ✚ 1\n❧");
    let bindings = flat(&module);
    let result = bindings.iter().find(|b| b.name == "result").unwrap();
    match &result.value {
        Expr::Let {
            name, value, body, ..
        } => {
            assert_eq!(name, "x");
            assert!(matches!(**value, Expr::Lit(Literal::Int(5), _)));
            assert!(matches!(**body, Expr::BinaryOp { .. }));
        }
        other => panic!("expected a Let, got {other:?}"),
    }
}

#[test]
fn local_function_declaration_lowers_to_core_letrec_with_a_single_lambda_member() {
    let module = lower_ok("result ≔ λf(n: ⟁): ⟁ → n\n f(5)\n❧");
    let bindings = flat(&module);
    let result = bindings.iter().find(|b| b.name == "result").unwrap();
    match &result.value {
        Expr::LetRec { bindings, body, .. } => {
            assert_eq!(bindings.len(), 1);
            assert_eq!(bindings[0].name, "f");
            assert!(matches!(bindings[0].value, Expr::Lambda { .. }));
            assert!(matches!(**body, Expr::Apply { .. }));
        }
        other => panic!("expected a LetRec, got {other:?}"),
    }
}

#[test]
fn adjacent_local_function_declarations_lower_to_one_letrec_with_both_members() {
    let module = lower_ok(
        "result ≔ λisEven(n: ⟁): ○ → ⟡ n { 0 → ◉ ⟢ m → isOdd(m ☠︎ 1) }\n \
         λisOdd(n: ⟁): ○ → ⟡ n { 0 → ◎ ⟢ m → isEven(m ☠︎ 1) }\n \
         isEven(4)\n❧",
    );
    let bindings = flat(&module);
    let result = bindings.iter().find(|b| b.name == "result").unwrap();
    match &result.value {
        Expr::LetRec { bindings, .. } => {
            assert_eq!(bindings.len(), 2);
            assert_eq!(bindings[0].name, "isEven");
            assert_eq!(bindings[1].name, "isOdd");
        }
        other => panic!("expected a LetRec, got {other:?}"),
    }
}

#[test]
fn local_mutable_binding_lowers_to_mutcell_inside_the_let() {
    let module = lower_ok("result ≔ x ≔˚ 5\n x ⚙︎ 9\n❧");
    let bindings = flat(&module);
    let result = bindings.iter().find(|b| b.name == "result").unwrap();
    match &result.value {
        Expr::Let { value, body, .. } => {
            assert!(matches!(**value, Expr::MutCell { .. }));
            assert!(matches!(**body, Expr::MutRebind { .. }));
        }
        other => panic!("expected a Let, got {other:?}"),
    }
}

#[test]
fn local_mutable_binding_does_not_leak_mutability_past_its_own_body() {
    // Scope correctness: a local `x ≔˚ ...` must only make `x` mut-typed
    // *within its own LocalBinding body* — an unrelated later reference
    // to a same-named outer immutable `x` must not be affected.
    let module = lower_ok(
        "x ≔ 1\n\
         result ≔ (x ≔˚ 5\n x ⚙︎ 9)\n\
         after ≔ x\n❧",
    );
    let bindings = flat(&module);
    let after = bindings.iter().find(|b| b.name == "after").unwrap();
    // Plain `Var`, not `MutRead` — the outer `x` was never mutable, and
    // the inner scope's mutability must not have leaked into it.
    assert!(matches!(after.value, Expr::Var(ref n, _) if n == "x"));
}

#[test]
fn local_immutable_binding_shadowing_an_outer_mutable_one_reads_directly_within_its_scope() {
    // The reverse leak: inside the local scope, a same-named local
    // *immutable* binding must shadow the outer mutable one, so a
    // reference to the name resolves to a plain `Var`, not `MutRead`.
    let module = lower_ok(
        "x ≔˚ 1\n\
         result ≔ x ≔ 2\n x\n❧",
    );
    let bindings = flat(&module);
    let result = bindings.iter().find(|b| b.name == "result").unwrap();
    match &result.value {
        Expr::Let { body, .. } => {
            assert!(matches!(**body, Expr::Var(ref n, _) if n == "x"));
        }
        other => panic!("expected a Let, got {other:?}"),
    }
}

// ── ImportDeclaration lowering: resolves to nothing in Core ────────────

#[test]
fn import_declaration_produces_no_core_binding() {
    let module = lower_ok("\u{27F2}shapes\nx \u{2254} 5\n\u{2767}\n");
    let bindings = flat(&module);
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].name, "x");
}

#[test]
fn import_between_two_function_declarations_splits_the_letrec_group() {
    // Same rule as Type/Value between adjacent FunctionDeclarations
    // (§8.2): group membership is a pure function of declaration kind.
    let module = lower_ok(
        "\u{3bb}f(n: \u{27C1}): \u{27C1} \u{2192} n\n\u{27F2}shapes\n\u{3bb}g(n: \u{27C1}): \u{27C1} \u{2192} n\n\u{2767}\n",
    );
    let groups = user_groups(&module);
    assert_eq!(groups.len(), 2);
    assert!(matches!(
        groups[0],
        obfusku_core::ast::BindingGroup::LetRec(_)
    ));
    assert!(matches!(
        groups[1],
        obfusku_core::ast::BindingGroup::LetRec(_)
    ));
}

// ── P0: anonymous `Lambda`'s optional `Parameter` annotation carries a
//    real `param_ty` into Core, and a name repeated within one surface
//    parameter list shares a renamed type variable while separately
//    written (even nested) `Lambda`s never do (§7.3). ─────────────────

#[test]
fn lambda_parameter_annotation_becomes_param_ty() {
    let module = lower_ok("f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x\n\u{2767}\n");
    match &flat(&module)[0].value {
        Expr::Lambda { param_ty, .. } => {
            assert_eq!(param_ty, &Some(obfusku_core::types::Type::Int));
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn lambda_parameter_without_annotation_has_no_param_ty() {
    let module = lower_ok("f \u{2254} \u{3bb}(x) \u{2192} x\n\u{2767}\n");
    match &flat(&module)[0].value {
        Expr::Lambda { param_ty, .. } => assert_eq!(param_ty, &None),
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn repeated_type_variable_within_one_parameter_list_shares_a_renamed_param() {
    // `λ(x: t, y: t) → ...` — one surface parameter list, so both `t`s
    // must renamed to the *same* module-unique name.
    let module = lower_ok("f \u{2254} \u{3bb}(x: t, y: t) \u{2192} x\n\u{2767}\n");
    match &flat(&module)[0].value {
        Expr::Lambda {
            param_ty: outer_ty,
            body,
            ..
        } => {
            let outer_ty = outer_ty.clone().expect("x should be annotated");
            match &**body {
                Expr::Lambda {
                    param_ty: inner_ty, ..
                } => {
                    let inner_ty = inner_ty.clone().expect("y should be annotated");
                    assert_eq!(
                        outer_ty, inner_ty,
                        "x and y must share the same renamed Param"
                    );
                }
                other => panic!("expected a nested Lambda, got {other:?}"),
            }
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn same_spelled_type_variable_in_two_separately_written_lambdas_does_not_share() {
    // `λ(x: t) → λ(y: t) → ...` — TWO separate surface `Lambda`
    // productions (not one comma-list), so despite reusing the spelling
    // `t`, each must get its OWN independently renamed type variable.
    let module = lower_ok("f \u{2254} \u{3bb}(x: t) \u{2192} \u{3bb}(y: t) \u{2192} x\n\u{2767}\n");
    match &flat(&module)[0].value {
        Expr::Lambda {
            param_ty: outer_ty,
            body,
            ..
        } => {
            let outer_ty = outer_ty.clone().expect("x should be annotated");
            match &**body {
                Expr::Lambda {
                    param_ty: inner_ty, ..
                } => {
                    let inner_ty = inner_ty.clone().expect("y should be annotated");
                    assert_ne!(
                        outer_ty, inner_ty,
                        "x and y come from separate Lambda productions and must not share"
                    );
                }
                other => panic!("expected a nested Lambda, got {other:?}"),
            }
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn function_declaration_parameter_lowering_is_unaffected_by_lambda_param_ty() {
    // FnParameter.ty keeps flowing through `declared_type` only (P0-A) —
    // the synthetic Lambda `lower_function_declaration` builds must never
    // duplicate the annotation onto `param_ty` too.
    let module = lower_ok("\u{3bb}f(x: \u{27C1}): \u{27C1} \u{2192} x\n\u{2767}\n");
    match &user_groups(&module)[0] {
        obfusku_core::ast::BindingGroup::LetRec(members) => match &members[0].value {
            Expr::Lambda { param_ty, .. } => assert_eq!(param_ty, &None),
            other => panic!("expected a Lambda, got {other:?}"),
        },
        other => panic!("expected a LetRec group, got {other:?}"),
    }
}

#[test]
fn two_independent_top_level_lambdas_reusing_a_type_variable_spelling_get_distinct_renames() {
    // Post-fix audit case: the renaming counter is shared across the
    // whole `Lowerer` (one per module) and never reset between
    // top-level declarations, so two SEPARATE top-level bindings' own
    // `Lambda`s — not nested within each other at all — must still get
    // independently renamed `t`s, exactly like two separately-written
    // nested `Lambda`s already do.
    let module = lower_ok(
        "f \u{2254} \u{3bb}(x: t) \u{2192} x\ng \u{2254} \u{3bb}(y: t) \u{2192} y\n\u{2767}\n",
    );
    let bindings = flat(&module);
    let f_ty = match &bindings.iter().find(|b| b.name == "f").unwrap().value {
        Expr::Lambda { param_ty, .. } => param_ty.clone().expect("f's x should be annotated"),
        other => panic!("expected a Lambda, got {other:?}"),
    };
    let g_ty = match &bindings.iter().find(|b| b.name == "g").unwrap().value {
        Expr::Lambda { param_ty, .. } => param_ty.clone().expect("g's y should be annotated"),
        other => panic!("expected a Lambda, got {other:?}"),
    };
    assert_ne!(
        f_ty, g_ty,
        "f and g share nothing but a spelling and must not share a rename"
    );
}
