//! Runtime evaluator tests, built directly against Core AST — the same
//! isolated-crate-testing pattern `obfusku-typecheck`'s tests already
//! use, since `obfusku-runtime` deliberately has no dependency on
//! `obfusku-syntax`'s parser/lowering to build test fixtures with.

use obfusku_core::ast::{Expr, MatchArm, Module, Pattern};
use obfusku_diagnostics::{SourceId, Span};
use obfusku_runtime::{evaluate, evaluate_with_prelude, Value};

fn sp() -> Span {
    Span {
        source: SourceId(0),
        start: 0,
        end: 0,
    }
}

fn var(n: &str) -> Expr {
    Expr::Var(n.to_string(), sp())
}
fn int(v: i64) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Int(v), sp())
}
fn real(v: f64) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Real(v), sp())
}
fn str_(v: &str) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Str(v.to_string()), sp())
}
fn boolean(v: bool) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Bool(v), sp())
}
fn lambda(param: &str, body: Expr) -> Expr {
    Expr::Lambda {
        param: param.to_string(),
        param_ty: None,
        body: Box::new(body),
        span: sp(),
    }
}
fn apply(f: Expr, a: Expr) -> Expr {
    Expr::Apply {
        func: Box::new(f),
        arg: Box::new(a),
        span: sp(),
    }
}
fn apply2(f: Expr, a: Expr, b: Expr) -> Expr {
    apply(apply(f, a), b)
}
fn mutcell(initial: Expr) -> Expr {
    Expr::MutCell {
        initial: Box::new(initial),
        span: sp(),
    }
}
fn mutread(cell: Expr) -> Expr {
    Expr::MutRead {
        cell: Box::new(cell),
        span: sp(),
    }
}
fn mutrebind(cell: Expr, new_value: Expr) -> Expr {
    Expr::MutRebind {
        cell: Box::new(cell),
        new_value: Box::new(new_value),
        span: sp(),
    }
}
fn ctor(tag: &str, args: Vec<Expr>) -> Expr {
    Expr::Constructor {
        tag: tag.to_string(),
        args,
        span: sp(),
    }
}
fn match_(scrutinee: Expr, arms: Vec<(Pattern, Expr)>) -> Expr {
    Expr::Match {
        scrutinee: Box::new(scrutinee),
        arms: arms
            .into_iter()
            .map(|(pattern, result)| MatchArm {
                pattern,
                result,
                span: sp(),
            })
            .collect(),
        span: sp(),
    }
}
fn pvar(n: &str) -> Pattern {
    Pattern::Var(n.to_string(), sp())
}
fn pwild() -> Pattern {
    Pattern::Wildcard(sp())
}
fn pctor(tag: &str, args: Vec<Pattern>) -> Pattern {
    Pattern::Constructor {
        tag: tag.to_string(),
        args,
        span: sp(),
    }
}
fn raise(value: Expr) -> Expr {
    Expr::Raise {
        value: Box::new(value),
        span: sp(),
    }
}
fn catch(body: Expr, handler_param: &str, handler_body: Expr) -> Expr {
    Expr::Catch {
        body: Box::new(body),
        handler_param: handler_param.to_string(),
        handler_body: Box::new(handler_body),
        span: sp(),
    }
}

/// A one-binding module whose value is `expr` — the common case for
/// these tests, where we only care about the last binding's result.
fn mk_binding(name: &str, value: Expr) -> obfusku_core::ast::Binding {
    obfusku_core::ast::Binding {
        name: name.to_string(),
        value,
        exported: false,
        span: sp(),
        declared_type: None,
    }
}

fn run(expr: Expr) -> Value {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result", expr,
        ))],
    };
    evaluate(&module).expect("evaluation should not fail")
}

/// Every entry becomes its own ordinary `Let` slot (sequential, not
/// mutually visible). Tests needing a `LetRec` group build `Module`
/// directly with `BindingGroup::LetRec`.
fn run_module(bindings: Vec<(&str, Expr)>) -> Value {
    let module = Module {
        type_decls: vec![],
        bindings: bindings
            .into_iter()
            .map(|(name, value)| obfusku_core::ast::BindingGroup::Let(mk_binding(name, value)))
            .collect(),
    };
    evaluate(&module).expect("evaluation should not fail")
}

/// `letrec_members` become one `BindingGroup::LetRec`; `then` bindings
/// follow as ordinary sequential `Let`s.
fn run_letrec(letrec_members: Vec<(&str, Expr)>, then: Vec<(&str, Expr)>) -> Value {
    let mut bindings = vec![obfusku_core::ast::BindingGroup::LetRec(
        letrec_members
            .into_iter()
            .map(|(name, value)| mk_binding(name, value))
            .collect(),
    )];
    bindings.extend(
        then.into_iter()
            .map(|(name, value)| obfusku_core::ast::BindingGroup::Let(mk_binding(name, value))),
    );
    let module = Module {
        type_decls: vec![],
        bindings,
    };
    evaluate(&module).expect("evaluation should not fail")
}

fn as_int(v: Value) -> i64 {
    match v {
        Value::Int(i) => i,
        other => panic!("expected Int, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Literals and basic evaluation
// ---------------------------------------------------------------------

#[test]
fn literals_evaluate_to_themselves() {
    assert!(matches!(run(int(42)), Value::Int(42)));
    assert!(matches!(run(real(1.5)), Value::Real(v) if v == 1.5));
    assert!(matches!(run(str_("hi")), Value::Str(s) if &*s == "hi"));
    assert!(matches!(run(boolean(true)), Value::Bool(true)));
}

// ---------------------------------------------------------------------
// Functions, application, currying, partial application
// ---------------------------------------------------------------------

#[test]
fn identity_function_returns_its_argument() {
    let id = lambda("x", var("x"));
    assert_eq!(as_int(run(apply(id, int(7)))), 7);
}

#[test]
fn curried_two_argument_function_via_nested_lambdas() {
    // const_ = λx → λy → x
    let const_ = lambda("x", lambda("y", var("x")));
    assert_eq!(as_int(run(apply2(const_, int(1), int(2)))), 1);
}

#[test]
fn partial_application_produces_a_reusable_closure() {
    let add_via_const_pattern = lambda("x", lambda("y", var("x")));
    let module = run_module(vec![
        ("give1", apply(add_via_const_pattern, int(1))),
        ("a", apply(var("give1"), int(100))),
        ("b", apply(var("give1"), int(200))),
    ]);
    // last binding ("b") should still be 1, proving the same partially
    // applied closure works across multiple calls without state bleed.
    assert_eq!(as_int(module), 1);
}

#[test]
fn calling_a_non_function_value_is_a_runtime_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: apply(int(5), int(1)),
                exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

// ---------------------------------------------------------------------
// Lexical closures: capture, shadowing, isolation, nesting
// ---------------------------------------------------------------------

#[test]
fn closure_captures_free_variable_from_definition_site_not_call_site() {
    // make_adder = λx → λy → x   (ignores y, returns the captured x)
    let make = lambda("x", lambda("y", var("x")));
    let module = run_module(vec![
        ("addFive", apply(make.clone(), int(5))),
        // calling addFive with a *different* value bound to `x` in this
        // scope must not leak in — closures are lexical, not dynamic.
        ("x", int(999)),
        ("result", apply(var("addFive"), int(1))),
    ]);
    assert_eq!(as_int(module), 5);
}

#[test]
fn nested_closures_each_capture_independently() {
    // outer = λx → λy → λz → x   — three levels deep, innermost still
    // sees the outermost binding.
    let inner = lambda("z", var("x"));
    let mid = lambda("y", inner);
    let outer = lambda("x", mid);
    let module = run_module(vec![(
        "result",
        apply(apply(apply(outer, int(1)), int(2)), int(3)),
    )]);
    assert_eq!(as_int(module), 1);
}

#[test]
fn two_calls_to_the_same_closure_are_isolated() {
    // λx → x, called twice with different arguments must not interfere.
    let id = lambda("x", var("x"));
    let module = run_module(vec![
        ("id", id),
        ("a", apply(var("id"), int(1))),
        ("b", apply(var("id"), int(2))),
    ]);
    assert_eq!(as_int(module), 2);
    // and re-confirm the first call's result independently:
    let module2 = run_module(vec![
        ("id2", lambda("x", var("x"))),
        ("a", apply(var("id2"), int(11))),
    ]);
    assert_eq!(as_int(module2), 11);
}

// ---------------------------------------------------------------------
// Mutation: cell identity, default (live) vs explicit capture
// ---------------------------------------------------------------------

#[test]
fn mutcell_read_and_rebind_round_trip() {
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("_bump", mutrebind(var("counter"), int(42))),
        ("result", mutread(var("counter"))),
    ]);
    assert_eq!(as_int(module), 42);
}

#[test]
fn default_capture_of_a_mutcell_sees_rebinds_made_after_closure_creation() {
    // §13: default capture shares the live cell and re-reads it at call
    // time, so `peek` must observe a rebind made after its creation.
    let peek = lambda("_", mutread(var("counter")));
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("peek", peek),
        ("_rebind", mutrebind(var("counter"), int(99))),
        ("result", apply(var("peek"), boolean(true))),
    ]);
    assert_eq!(as_int(module), 99);
}

#[test]
fn explicit_capture_shares_the_same_cell_as_the_original_binding() {
    // bump = λstep → MutRebind(Var(counter), step)  (explicit-capture
    // rebind — the bare `Var(counter)` here is exactly what the surface
    // `˚counter ⚙︎ step` form desugars to).
    let bump = lambda("step", mutrebind(var("counter"), var("step")));
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("bump", bump),
        ("_call", apply(var("bump"), int(10))),
        ("result", mutread(var("counter"))),
    ]);
    assert_eq!(as_int(module), 10);
}

#[test]
fn multiple_closures_capturing_the_same_cell_observe_each_others_writes() {
    let counter_maker = mutcell(int(0));
    let module = run_module(vec![
        ("counter", counter_maker),
        (
            "bump",
            lambda("step", mutrebind(var("counter"), var("step"))),
        ),
        ("peek", lambda("_", mutread(var("counter")))),
        ("_c1", apply(var("bump"), int(1))),
        ("_c2", apply(var("bump"), int(2))),
        ("result", apply(var("peek"), boolean(true))),
    ]);
    assert_eq!(as_int(module), 2);
}

#[test]
fn reading_a_non_cell_value_is_a_runtime_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: mutread(int(5)),
                exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

// ---------------------------------------------------------------------
// ADTs / constructors / pattern matching
// ---------------------------------------------------------------------

#[test]
fn nullary_constructor_matches_by_tag() {
    let expr = match_(
        ctor("None", vec![]),
        vec![(pctor("None", vec![]), int(0)), (pwild(), int(1))],
    );
    assert_eq!(as_int(run(expr)), 0);
}

#[test]
fn payload_bearing_constructor_binds_its_field() {
    let expr = match_(
        ctor("Some", vec![int(7)]),
        vec![
            (pctor("Some", vec![pvar("x")]), var("x")),
            (pwild(), int(-1)),
        ],
    );
    assert_eq!(as_int(run(expr)), 7);
}

#[test]
fn nested_constructors_destructure_recursively() {
    // Some(Some(9)) matched against Some(Some(x))
    let expr = match_(
        ctor("Some", vec![ctor("Some", vec![int(9)])]),
        vec![
            (
                pctor("Some", vec![pctor("Some", vec![pvar("x")])]),
                var("x"),
            ),
            (pwild(), int(-1)),
        ],
    );
    assert_eq!(as_int(run(expr)), 9);
}

#[test]
fn exhaustive_match_over_two_variants_picks_the_matching_arm() {
    let ok_case = match_(
        ctor("Ok", vec![int(1)]),
        vec![
            (pctor("Ok", vec![pvar("v")]), var("v")),
            (pctor("Err", vec![pvar("_e")]), int(-1)),
        ],
    );
    assert_eq!(as_int(run(ok_case)), 1);

    let err_case = match_(
        ctor("Err", vec![int(1)]),
        vec![
            (pctor("Ok", vec![pvar("v")]), var("v")),
            (pctor("Err", vec![pvar("_e")]), int(-1)),
        ],
    );
    assert_eq!(as_int(run(err_case)), -1);
}

#[test]
fn constructor_equality_is_structural_not_identity() {
    // Two independently-built Some(5) values compare equal via a match
    // against a literal-shaped arm — proven indirectly since Value has
    // no public equality operator surfaced to Core yet; instead prove it
    // via structural_eq directly.
    use obfusku_runtime::Value as V;
    let a = V::Adt {
        tag: std::rc::Rc::from("Some"),
        args: std::rc::Rc::from(vec![V::Int(5)]),
    };
    let b = V::Adt {
        tag: std::rc::Rc::from("Some"),
        args: std::rc::Rc::from(vec![V::Int(5)]),
    };
    assert!(a.structural_eq(&b));
    let c = V::Adt {
        tag: std::rc::Rc::from("Some"),
        args: std::rc::Rc::from(vec![V::Int(6)]),
    };
    assert!(!a.structural_eq(&c));
}

#[test]
fn no_matching_arm_at_runtime_is_reported_as_an_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: match_(ctor("Foo", vec![]), vec![(pctor("Bar", vec![]), int(1))]),
                exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

// ---------------------------------------------------------------------
// Recursion / tail-call optimization
// ---------------------------------------------------------------------

#[test]
fn module_level_recursive_function_computes_correctly() {
    // countdown = λn → match n with 0 → 0 | _ → countdown(n - 1)... but
    // there's no subtraction primitive in Core yet, so instead prove
    // recursion terminates via constructor-shaped decrement: represent
    // n as a chain of `Succ`/`Zero` and recurse structurally.
    let succ = |n: Expr| ctor("Succ", vec![n]);
    let zero = ctor("Zero", vec![]);
    let three = succ(succ(succ(zero)));

    // depth = λn → match n with Zero → 0 | Succ(k) → depth(k)  -- always
    // returns 0, but only by actually recursing down the chain.
    let depth_body = match_(
        var("n"),
        vec![
            (pctor("Zero", vec![]), int(0)),
            (
                pctor("Succ", vec![pvar("k")]),
                apply(var("depth"), var("k")),
            ),
        ],
    );
    let module = run_letrec(
        vec![("depth", lambda("n", depth_body))],
        vec![("result", apply(var("depth"), three))],
    );
    assert_eq!(as_int(module), 0);
}

#[test]
fn deep_tail_recursion_does_not_grow_the_rust_stack() {
    // loop_ = λn → match n with Zero → 0 | Succ(k) → loop_(k)
    //
    // Evaluating the depth-N `Succ`-chain argument costs one native
    // stack frame per layer (non-tail), fully unwinding before `loop_`
    // starts running. Depth is picked large enough that the tail-call
    // phase alone, without trampolining, would need more stack than any
    // ordinary process has — proving flat stack usage really comes from
    // TCO, not a small N happening to fit.
    let depth = 100_000usize;
    let mut n_expr = ctor("Zero", vec![]);
    for _ in 0..depth {
        n_expr = ctor("Succ", vec![n_expr]);
    }

    let loop_body = match_(
        var("n"),
        vec![
            (pctor("Zero", vec![]), int(0)),
            (
                pctor("Succ", vec![pvar("k")]),
                apply(var("loop_"), var("k")),
            ),
        ],
    );

    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024 * 1024)
        .spawn(move || {
            as_int(run_letrec(
                vec![("loop_", lambda("n", loop_body))],
                vec![("result", apply(var("loop_"), n_expr))],
            ))
        })
        .expect("spawn test thread");
    let result = handle.join().expect("test thread panicked");
    assert_eq!(result, 0);
}

// ---------------------------------------------------------------------
// Raise / Catch
// ---------------------------------------------------------------------

#[test]
fn raise_without_catch_is_an_uncaught_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: raise(str_("boom")),
                exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

#[test]
fn catch_intercepts_a_raise_and_runs_the_handler() {
    let expr = catch(raise(int(42)), "e", var("e"));
    assert_eq!(as_int(run(expr)), 42);
}

#[test]
fn catch_with_no_raise_evaluates_to_the_body() {
    let expr = catch(int(1), "e", int(999));
    assert_eq!(as_int(run(expr)), 1);
}

#[test]
fn nested_catch_only_the_innermost_active_handler_intercepts() {
    // catch(catch(raise(1), "e", raise(var e)), "e2", var e2)
    // inner handler re-raises what it caught; outer handler catches that.
    let inner = catch(raise(int(1)), "e", raise(var("e")));
    let outer = catch(inner, "e2", var("e2"));
    assert_eq!(as_int(run(outer)), 1);
}

#[test]
fn raise_inside_a_handler_is_not_caught_by_the_same_catch() {
    // The handler re-raises; there is no *outer* catch, so this must be
    // an uncaught error, proving the frame deactivated before the
    // handler ran (per §15.1).
    let expr = catch(raise(int(1)), "e", raise(int(2)));
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: expr,
                exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

#[test]
fn catch_around_a_function_call_that_raises_deep_inside() {
    // thrower = λ_ → raise(5)
    // catch(thrower(unit), "e", e)
    let thrower = lambda("_", raise(int(5)));
    let module = run_module(vec![
        ("thrower", thrower),
        (
            "result",
            catch(apply(var("thrower"), boolean(true)), "e", var("e")),
        ),
    ]);
    assert_eq!(as_int(module), 5);
}

// ---------------------------------------------------------------------
// Cross-feature composition
// ---------------------------------------------------------------------

#[test]
fn closure_over_a_mutable_cell_combined_with_match() {
    // toggle = λ_ → match MutRead(flag) with
    //            True → MutRebind(flag, False)
    //            False → MutRebind(flag, True)
    // then read it back.
    let toggle_body = match_(
        mutread(var("flag")),
        vec![
            (
                Pattern::Lit(obfusku_core::ast::Literal::Bool(true), sp()),
                mutrebind(var("flag"), boolean(false)),
            ),
            (pwild(), mutrebind(var("flag"), boolean(true))),
        ],
    );
    let module = run_module(vec![
        ("flag", mutcell(boolean(true))),
        ("toggle", lambda("_", toggle_body)),
        ("_t1", apply(var("toggle"), boolean(true))),
        ("_t2", apply(var("toggle"), boolean(true))),
        ("result", mutread(var("flag"))),
    ]);
    // toggled twice from true: true -> false -> true
    assert!(matches!(module, Value::Bool(true)));
}

#[test]
fn mutation_inside_a_catch_handler_is_observed_after_catch_returns() {
    let expr = catch(
        raise(boolean(true)),
        "_e",
        mutrebind(var("counter"), int(1)),
    );
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("_run", expr),
        ("result", mutread(var("counter"))),
    ]);
    assert_eq!(as_int(module), 1);
}

#[test]
fn recursive_function_over_an_adt_combined_with_tail_position_match() {
    // sum-depth style: same shape as the recursion test above but proves
    // ADT + closure + recursion compose without special-casing.
    let succ = |n: Expr| ctor("Succ", vec![n]);
    let n = succ(succ(ctor("Zero", vec![])));
    let is_zero_body = match_(
        var("n"),
        vec![
            (pctor("Zero", vec![]), boolean(true)),
            (
                pctor("Succ", vec![pvar("k")]),
                apply(var("is_zero"), var("k")),
            ),
        ],
    );
    let module = run_letrec(
        vec![("is_zero", lambda("n", is_zero_body))],
        vec![("result", apply(var("is_zero"), n))],
    );
    assert!(matches!(module, Value::Bool(true)));
}

#[test]
fn duplicate_top_level_binding_names_are_a_caller_error_here_not_a_runtime_check() {
    // Two module bindings sharing a name don't get real lexical
    // shadowing at this layer — the runtime stores every binding in one
    // shared, mutable frame, so a reused name silently overwrites the
    // earlier one. `obfusku-typecheck::check` rejects duplicate
    // top-level names outright, so only hand-built Core (as here, an
    // explicit caller error per `evaluate`'s contract) reaches this.
    let capture_x = lambda("ignored", var("x"));
    let module = run_module(vec![
        ("x", int(1)),
        ("capture_x", capture_x),
        ("x", int(2)),
        ("result", apply(var("capture_x"), boolean(true))),
    ]);
    assert_eq!(
        as_int(module),
        2,
        "documents the overwrite, not a guarantee"
    );
}

// ---------------------------------------------------------------------
// Operators (`SEMANTIC_CORE.md` §20.2) — runtime evaluation. Typing is
// obfusku-typecheck's job (already tested there); these tests build
// well-typed Core directly and check actual evaluated results/errors.
// ---------------------------------------------------------------------

use obfusku_core::ast::{BinOp, UnOp};

fn binop(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::BinaryOp {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span: sp(),
    }
}
fn unop(op: UnOp, operand: Expr) -> Expr {
    Expr::UnaryOp {
        op,
        operand: Box::new(operand),
        span: sp(),
    }
}

#[test]
fn arithmetic_int() {
    assert_eq!(as_int(run(binop(BinOp::Add, int(3), int(4)))), 7);
    assert_eq!(as_int(run(binop(BinOp::Sub, int(3), int(4)))), -1);
    assert_eq!(as_int(run(binop(BinOp::Mul, int(3), int(4)))), 12);
    assert_eq!(as_int(run(binop(BinOp::Div, int(12), int(4)))), 3);
}

#[test]
fn arithmetic_real() {
    let as_real = |v: Value| match v {
        Value::Real(r) => r,
        other => panic!("expected Real, got {other:?}"),
    };
    assert_eq!(as_real(run(binop(BinOp::Add, real(1.5), real(2.5)))), 4.0);
    assert_eq!(as_real(run(binop(BinOp::Sub, real(1.5), real(2.5)))), -1.0);
    assert_eq!(as_real(run(binop(BinOp::Mul, real(2.0), real(3.0)))), 6.0);
    assert_eq!(as_real(run(binop(BinOp::Div, real(6.0), real(2.0)))), 3.0);
}

#[test]
fn string_concatenation() {
    let v = run(binop(BinOp::Add, str_("foo"), str_("bar")));
    assert!(matches!(v, Value::Str(s) if &*s == "foobar"));
}

#[test]
fn int_division_by_zero_raises_a_distinctly_tagged_division_by_zero_exception() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Div, int(1), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
}

#[test]
fn int_modulo_by_zero_also_raises() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Mod, int(7), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
}

// ---------------------------------------------------------------------
// P0-D: `Int` is fixed-width; an unrepresentable arithmetic result
// raises `IntegerOverflow`, deterministically, in every build profile —
// never a debug-only panic, never a release-only silent wrap (the
// previous, accidental behavior of bare Rust operators on `i64`).
// `DivisionByZero` stays a separate condition, confirmed unchanged
// above and again below for `MIN_INT`-specific cases.
// ---------------------------------------------------------------------

fn assert_raises_integer_overflow(expr: Expr) {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad", expr,
        ))],
    };
    let err = evaluate(&module).expect_err("expected an IntegerOverflow raise, not success");
    assert!(err.message.contains("IntegerOverflow"), "{}", err.message);
}

#[test]
fn int_max_plus_one_raises_integer_overflow_not_a_panic() {
    assert_raises_integer_overflow(binop(BinOp::Add, int(i64::MAX), int(1)));
}

#[test]
fn int_min_minus_one_raises_integer_overflow() {
    assert_raises_integer_overflow(binop(BinOp::Sub, int(i64::MIN), int(1)));
}

#[test]
fn int_max_times_two_raises_integer_overflow() {
    assert_raises_integer_overflow(binop(BinOp::Mul, int(i64::MAX), int(2)));
}

#[test]
fn int_min_divided_by_negative_one_raises_integer_overflow_not_division_by_zero() {
    assert_raises_integer_overflow(binop(BinOp::Div, int(i64::MIN), int(-1)));
}

#[test]
fn int_min_modulo_negative_one_raises_integer_overflow() {
    assert_raises_integer_overflow(binop(BinOp::Mod, int(i64::MIN), int(-1)));
}

#[test]
fn negating_int_min_raises_integer_overflow() {
    assert_raises_integer_overflow(unop(UnOp::Neg, int(i64::MIN)));
}

#[test]
fn division_by_zero_is_still_division_by_zero_not_integer_overflow() {
    // Regression: confirms the two conditions were never merged —
    // `IntegerOverflow` only covers `MIN_INT / -1`, not the zero case.
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Div, int(1), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
    assert!(!err.message.contains("IntegerOverflow"), "{}", err.message);
}

#[test]
fn modulo_by_zero_is_still_division_by_zero_not_integer_overflow() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "bad",
            binop(BinOp::Mod, int(1), int(0)),
        ))],
    };
    let err = evaluate(&module).expect_err("expected a division-by-zero raise");
    assert!(err.message.contains("DivisionByZero"), "{}", err.message);
    assert!(!err.message.contains("IntegerOverflow"), "{}", err.message);
}

#[test]
fn ordinary_in_range_arithmetic_is_completely_unaffected() {
    assert_eq!(
        as_int(run(binop(BinOp::Add, int(i64::MAX - 1), int(1)))),
        i64::MAX
    );
    assert_eq!(
        as_int(run(binop(BinOp::Sub, int(i64::MIN + 1), int(1)))),
        i64::MIN
    );
    assert_eq!(as_int(run(unop(UnOp::Neg, int(i64::MAX)))), -i64::MAX);
}

#[test]
fn real_division_by_zero_follows_ieee_754_never_raises() {
    let pos_inf = run(binop(BinOp::Div, real(1.0), real(0.0)));
    match pos_inf {
        Value::Real(r) => assert!(r.is_infinite() && r > 0.0),
        other => panic!("expected Real(Infinity), got {other:?}"),
    }
    let nan = run(binop(BinOp::Div, real(0.0), real(0.0)));
    match nan {
        Value::Real(r) => assert!(r.is_nan()),
        other => panic!("expected Real(NaN), got {other:?}"),
    }
}

#[test]
fn modulo_is_truncating_sign_of_dividend_matching_the_frozen_worked_examples() {
    // Exactly SEMANTIC_CORE.md §20.2's four frozen examples.
    assert_eq!(as_int(run(binop(BinOp::Mod, int(7), int(3)))), 1);
    assert_eq!(as_int(run(binop(BinOp::Mod, int(-7), int(3)))), -1);
    assert_eq!(as_int(run(binop(BinOp::Mod, int(7), int(-3)))), 1);
    assert_eq!(as_int(run(binop(BinOp::Mod, int(-7), int(-3)))), -1);
}

#[test]
fn comparison_operators() {
    assert!(matches!(
        run(binop(BinOp::Lt, int(1), int(2))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Gt, int(1), int(2))),
        Value::Bool(false)
    ));
    assert!(matches!(
        run(binop(BinOp::Le, int(2), int(2))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Ge, int(1), int(2))),
        Value::Bool(false)
    ));
}

#[test]
fn real_ordering_with_nan_is_always_false_ieee_754_not_total() {
    // §18: ordering and equality are allowed to disagree about NaN.
    let nan = real(f64::NAN);
    assert!(matches!(
        run(binop(BinOp::Lt, nan.clone(), real(1.0))),
        Value::Bool(false)
    ));
    assert!(matches!(
        run(binop(BinOp::Gt, nan, real(1.0))),
        Value::Bool(false)
    ));
}

#[test]
fn equality_is_total_nan_equals_nan_unlike_ordering() {
    let v = run(binop(BinOp::Eq, real(f64::NAN), real(f64::NAN)));
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn equality_and_inequality_on_ordinary_values() {
    assert!(matches!(
        run(binop(BinOp::Eq, int(1), int(1))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::NotEq, int(1), int(2))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Eq, str_("a"), str_("a"))),
        Value::Bool(true)
    ));
}

#[test]
fn equality_on_adt_values_is_structural() {
    let a = ctor("Some", vec![int(5)]);
    let b = ctor("Some", vec![int(5)]);
    assert!(matches!(run(binop(BinOp::Eq, a, b)), Value::Bool(true)));
    let c = ctor("Some", vec![int(6)]);
    let d = ctor("Some", vec![int(5)]);
    assert!(matches!(run(binop(BinOp::Eq, c, d)), Value::Bool(false)));
}

#[test]
fn xor_is_eager_and_bool_only() {
    assert!(matches!(
        run(binop(BinOp::Xor, boolean(true), boolean(false))),
        Value::Bool(true)
    ));
    assert!(matches!(
        run(binop(BinOp::Xor, boolean(true), boolean(true))),
        Value::Bool(false)
    ));
}

#[test]
fn unary_not_and_negation() {
    assert!(matches!(
        run(unop(UnOp::Not, boolean(true))),
        Value::Bool(false)
    ));
    assert_eq!(as_int(run(unop(UnOp::Neg, int(5)))), -5);
    let v = run(unop(UnOp::Neg, real(2.5)));
    assert!(matches!(v, Value::Real(r) if r == -2.5));
}

#[test]
fn both_operands_are_always_evaluated_strictly_left_to_right() {
    // No accidental short-circuit in BinaryOp evaluation: both sides of
    // ⊻ must be evaluated even though it has no such shortcut anyway.
    let expr = binop(
        BinOp::Xor,
        boolean(true),
        apply(lambda("x", var("x")), boolean(false)),
    );
    assert!(matches!(run(expr), Value::Bool(true)));
}

// ── LocalBinding: `Expr::Let`/`Expr::LetRec` (`SEMANTIC_CORE.md` §11) ──

fn let_(name: &str, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        name: name.to_string(),
        value: Box::new(value),
        body: Box::new(body),
        span: sp(),
    }
}

fn letrec_(members: Vec<(&str, Expr)>, body: Expr) -> Expr {
    Expr::LetRec {
        bindings: members.into_iter().map(|(n, v)| mk_binding(n, v)).collect(),
        body: Box::new(body),
        span: sp(),
    }
}

#[test]
fn let_evaluates_value_then_binds_it_for_the_body() {
    let expr = let_("x", int(5), binop(BinOp::Add, var("x"), int(1)));
    assert_eq!(as_int(run(expr)), 6);
}

#[test]
fn nested_let_shadowing_evaluates_the_innermost_binding() {
    let expr = let_("x", int(1), let_("x", int(2), var("x")));
    assert_eq!(as_int(run(expr)), 2);
}

#[test]
fn let_mutable_binding_supports_mutation_scoped_to_its_body() {
    let expr = let_(
        "x",
        mutcell(int(5)),
        apply(lambda("_", mutread(var("x"))), mutrebind(var("x"), int(9))),
    );
    assert_eq!(as_int(run(expr)), 9);
}

#[test]
fn letrec_supports_self_recursion_via_tail_calls_at_real_depth() {
    // The reason `LetRec` (not `Let`) exists at all: `countdown`'s own
    // name must be visible inside its own body. Run deep enough (20,000)
    // that this would stack-overflow if `body`'s tail continuation
    // (§14.1) weren't actually looping in the trampoline.
    let countdown = lambda(
        "n",
        match_(
            var("n"),
            vec![
                (
                    Pattern::Lit(obfusku_core::ast::Literal::Int(0), sp()),
                    int(0),
                ),
                (
                    pwild(),
                    apply(var("countdown"), binop(BinOp::Sub, var("n"), int(1))),
                ),
            ],
        ),
    );
    let expr = letrec_(
        vec![("countdown", countdown)],
        apply(var("countdown"), int(20_000)),
    );
    assert_eq!(as_int(run(expr)), 0);
}

#[test]
fn letrec_members_are_mutually_visible_and_can_call_each_other() {
    let is_even = lambda(
        "n",
        match_(
            var("n"),
            vec![
                (
                    Pattern::Lit(obfusku_core::ast::Literal::Int(0), sp()),
                    boolean(true),
                ),
                (
                    pwild(),
                    apply(var("isOdd"), binop(BinOp::Sub, var("n"), int(1))),
                ),
            ],
        ),
    );
    let is_odd = lambda(
        "n",
        match_(
            var("n"),
            vec![
                (
                    Pattern::Lit(obfusku_core::ast::Literal::Int(0), sp()),
                    boolean(false),
                ),
                (
                    pwild(),
                    apply(var("isEven"), binop(BinOp::Sub, var("n"), int(1))),
                ),
            ],
        ),
    );
    let expr = letrec_(
        vec![("isEven", is_even), ("isOdd", is_odd)],
        apply(var("isEven"), int(10)),
    );
    assert!(matches!(run(expr), Value::Bool(true)));
}

#[test]
fn letrec_group_does_not_leak_its_members_outside_the_local_body() {
    // `countdown` is only visible within the `LetRec`'s own `body` — a
    // later, unrelated reference must be an unbound-variable failure,
    // proving the group's frame is a genuinely nested child scope.
    let module = Module {
        type_decls: vec![],
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(mk_binding(
                "first",
                letrec_(
                    vec![("countdown", lambda("n", var("n")))],
                    apply(var("countdown"), int(1)),
                ),
            )),
            obfusku_core::ast::BindingGroup::Let(mk_binding("second", var("countdown"))),
        ],
    };
    assert!(evaluate(&module).is_err());
}

// ── ArrayLiteral / Index (`SEMANTIC_CORE.md` §9.2) ─────────────────────

fn array_lit(elements: Vec<Expr>) -> Expr {
    Expr::ArrayLiteral {
        elements,
        span: sp(),
    }
}
fn index(array: Expr, idx: Expr) -> Expr {
    Expr::Index {
        array: Box::new(array),
        index: Box::new(idx),
        span: sp(),
    }
}

#[test]
fn array_literal_evaluates_its_elements_and_produces_an_array_value() {
    let v = run(array_lit(vec![int(1), int(2), int(3)]));
    match v {
        Value::Array(elems) => {
            assert_eq!(elems.len(), 3);
            assert!(matches!(elems[0], Value::Int(1)));
            assert!(matches!(elems[2], Value::Int(3)));
        }
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
fn empty_array_literal_evaluates_to_an_empty_array() {
    let v = run(array_lit(vec![]));
    match v {
        Value::Array(elems) => assert!(elems.is_empty()),
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
fn indexing_reads_the_element_at_position() {
    let v = run(index(array_lit(vec![int(10), int(20), int(30)]), int(1)));
    assert_eq!(as_int(v), 20);
}

#[test]
fn out_of_bounds_index_raises_invalidoperation() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result",
            index(array_lit(vec![int(1), int(2)]), int(5)),
        ))],
    };
    match evaluate(&module) {
        Err(d) => assert!(d.message.contains("uncaught exception")),
        Ok(v) => panic!("expected an error, got {v:?}"),
    }
}

#[test]
fn negative_index_is_also_out_of_bounds() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result",
            index(array_lit(vec![int(1)]), int(-1)),
        ))],
    };
    assert!(evaluate(&module).is_err());
}

#[test]
fn array_equality_is_structural_elementwise() {
    let a = array_lit(vec![int(1), int(2)]);
    let b = array_lit(vec![int(1), int(2)]);
    assert!(matches!(run(binop(BinOp::Eq, a, b)), Value::Bool(true)));
}

#[test]
fn array_inequality_by_length_or_element() {
    let a = array_lit(vec![int(1), int(2)]);
    let b = array_lit(vec![int(1), int(3)]);
    assert!(matches!(run(binop(BinOp::NotEq, a, b)), Value::Bool(true)));
}

#[test]
fn out_of_bounds_index_is_an_ordinary_catchable_exception() {
    let v = run(catch(index(array_lit(vec![int(1)]), int(9)), "e", int(0)));
    assert_eq!(as_int(v), 0);
}

// ── `evaluate_with_prelude` (imports) ───────────────────────────────

#[test]
fn prelude_bindings_are_visible_to_the_module() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result",
            apply(var("imported"), int(5)),
        ))],
    };
    let bindings = evaluate_with_prelude(
        &module,
        vec![(
            "imported".to_string(),
            Value::Closure(std::rc::Rc::new(obfusku_runtime::value::ClosureData {
                param: "x".to_string(),
                body: std::rc::Rc::new(binop(BinOp::Add, var("x"), int(1))),
                env: obfusku_runtime::env::Env::root(),
            })),
        )],
    )
    .expect("evaluation should not fail");
    let (_, _, result_value) = bindings
        .into_iter()
        .find(|(n, _, _)| n == "result")
        .unwrap();
    assert_eq!(as_int(result_value), 6);
}

#[test]
fn evaluate_with_prelude_returns_every_top_level_binding_with_its_export_flag() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(obfusku_core::ast::Binding {
                name: "priv".to_string(),
                value: int(1),
                exported: false,
                span: sp(),
                declared_type: None,
            }),
            obfusku_core::ast::BindingGroup::Let(obfusku_core::ast::Binding {
                name: "pub_".to_string(),
                value: int(2),
                exported: true,
                span: sp(),
                declared_type: None,
            }),
        ],
    };
    let bindings =
        evaluate_with_prelude(&module, std::iter::empty()).expect("evaluation should not fail");
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].0, "priv");
    assert!(!bindings[0].1);
    assert_eq!(bindings[1].0, "pub_");
    assert!(bindings[1].1);
}
