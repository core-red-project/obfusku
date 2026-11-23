//! Literals, references, lambdas, application, holes and pipes.

use super::support::*;

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
    assert!(flat(&module)[0].is_exported);
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
