//! Module-level generic functions and parameter types seeded from signatures.

use super::support::*;

#[test]
fn module_level_generic_identity_generalizes_and_is_reusable_at_different_types() {
    // Same `id` as above, applied at Int and then at Str — proof the
    // fix produces a genuinely generalized scheme (fresh instantiation
    // per call site), not merely "no panic, pinned to the first use."
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let typed = check_ok_letrec(
        vec![id],
        vec![
            binding("a", apply(var("id"), int(5))),
            binding("b", apply(var("id"), string("hi"))),
        ],
    );
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn module_level_generic_function_with_two_independent_type_parameters() {
    // λconst(x: t, y: u): t → x — two distinct free type parameters in
    // one signature; `u` never appears in the return type at all, so
    // it must still get its own fresh variable rather than colliding
    // with `t`'s or being left unbound.
    let const_fn = declared_binding(
        "constFn",
        lambda("x", lambda("y", var("x"))),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Function(
                Box::new(CoreType::Param("u".to_string())),
                Box::new(CoreType::Param("t".to_string())),
            )),
        ),
    );
    let typed = check_ok_letrec(
        vec![const_fn],
        vec![binding(
            "result",
            apply(apply(var("constFn"), int(5)), string("ignored")),
        )],
    );
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn module_level_recursive_generic_function_type_checks() {
    // repeatIdentity(n: Nat, x: t): t → self-recursive *and* generic —
    // exactly the shape map/filter/fold need (recursion via LetRec,
    // genericity via a declared free Param).
    let nat_decl = type_decl(
        "Nat",
        vec![],
        vec![
            variant("Zero", vec![]),
            variant("Succ", vec![CoreType::Adt("Nat".into(), vec![])]),
        ],
    );
    let zero = binding(
        "Zero",
        Expr::Constructor {
            tag: "Zero".to_string(),
            args: vec![],
            span: sp(),
        },
    );
    let succ = binding(
        "Succ",
        lambda(
            "n",
            Expr::Constructor {
                tag: "Succ".to_string(),
                args: vec![var("n")],
                span: sp(),
            },
        ),
    );
    let repeat_identity = declared_binding(
        "repeatIdentity",
        lambda(
            "n",
            lambda(
                "x",
                match_expr(
                    var("n"),
                    vec![
                        arm(pctor("Zero", vec![]), var("x")),
                        arm(
                            pctor("Succ", vec![pvar("k")]),
                            apply(apply(var("repeatIdentity"), var("k")), var("x")),
                        ),
                    ],
                ),
            ),
        ),
        CoreType::Function(
            Box::new(CoreType::Adt("Nat".into(), vec![])),
            Box::new(CoreType::Function(
                Box::new(CoreType::Param("t".to_string())),
                Box::new(CoreType::Param("t".to_string())),
            )),
        ),
    );

    let mut bindings = vec![
        obfusku_core::ast::BindingGroup::Let(zero),
        obfusku_core::ast::BindingGroup::Let(succ),
        obfusku_core::ast::BindingGroup::LetRec(vec![repeat_identity]),
    ];
    bindings.push(obfusku_core::ast::BindingGroup::Let(binding(
        "result",
        apply(
            apply(var("repeatIdentity"), apply(var("Succ"), var("Zero"))),
            int(42),
        ),
    )));
    let module = Module {
        type_decls: vec![nat_decl],
        bindings,
    };
    let typed = check(&module)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn module_level_generic_function_misuse_is_a_diagnostic_never_a_panic() {
    // `id`'s own group-internal (not yet generalized) instantiation is
    // pinned to `Int` by `bad`'s first use, then `bad`'s own declared
    // type (`Str`) contradicts that — a genuine type error the fix must
    // still report cleanly, never as a panic, exactly like the
    // pre-existing concrete-type version of this same check
    // (`letrec_declared_type_is_unified_before_the_body_is_inferred`).
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let bad = declared_binding("bad", apply(var("id"), int(5)), CoreType::Str);
    let ds = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check(&Module {
            type_decls: Vec::new(),
            bindings: vec![obfusku_core::ast::BindingGroup::LetRec(vec![id, bad])],
        })
    })) {
        Ok(Err(ds)) => ds,
        Ok(Ok(_)) => panic!("expected a type error, but the module checked successfully"),
        Err(_) => panic!("a genuinely ill-typed generic usage must be a diagnostic, not a panic"),
    };
    assert!(!describe(&ds).is_empty());
}

#[test]
fn local_letrec_generic_declared_type_no_longer_panics() {
    let typed = check_ok(vec![binding(
        "result",
        letrec_expr(
            vec![declared_binding(
                "id",
                lambda("x", var("x")),
                CoreType::Function(
                    Box::new(CoreType::Param("t".to_string())),
                    Box::new(CoreType::Param("t".to_string())),
                ),
            )],
            apply(var("id"), int(7)),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn local_letrec_two_arg_function_x_plus_y_type_checks() {
    // Same P0-A shape as the module-level test below, through the
    // local (expression-level) `Expr::LetRec` path instead.
    let typed = check_ok(vec![binding(
        "result",
        letrec_expr(
            vec![declared_binding(
                "add",
                lambda("x", lambda("y", binop(BinOp::Add, var("x"), var("y")))),
                CoreType::Function(
                    Box::new(CoreType::Int),
                    Box::new(CoreType::Function(
                        Box::new(CoreType::Int),
                        Box::new(CoreType::Int),
                    )),
                ),
            )],
            apply(apply(var("add"), int(2)), int(3)),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn two_arg_function_x_plus_y_type_checks() {
    // The exact shape that used to fail: `λadd(x: ⟁, y: ⟁): ⟁ → x ✚ y`.
    let add = declared_binding(
        "add",
        lambda("x", lambda("y", binop(BinOp::Add, var("x"), var("y")))),
        CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Function(
                Box::new(CoreType::Int),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let typed = check_ok_letrec(
        vec![add],
        vec![binding("result", apply(apply(var("add"), int(2)), int(3)))],
    );
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn two_arg_function_y_plus_x_type_checks_identically() {
    // Operand order must not matter — this used to fail exactly the
    // same way `x ✚ y` did, proving the old bug was never really about
    // "which side," only "neither side is ever resolved yet."
    let add = declared_binding(
        "add",
        lambda("x", lambda("y", binop(BinOp::Add, var("y"), var("x")))),
        CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Function(
                Box::new(CoreType::Int),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let typed = check_ok_letrec(
        vec![add],
        vec![binding("result", apply(apply(var("add"), int(2)), int(3)))],
    );
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn recursive_two_arg_function_uses_operator_on_its_own_accumulator_param() {
    // A genuinely recursive two-argument function whose body applies an
    // operator directly to its own (non-literal) accumulator parameter
    // — the shape every `0 ✚ acc ✚ n` workaround in this session's
    // stdlib/native tests existed only to route around.
    let nat_decl = type_decl(
        "Nat",
        vec![],
        vec![
            variant("Zero", vec![]),
            variant("Succ", vec![CoreType::Adt("Nat".into(), vec![])]),
        ],
    );
    let zero = binding(
        "Zero",
        Expr::Constructor {
            tag: "Zero".to_string(),
            args: vec![],
            span: sp(),
        },
    );
    let succ = binding(
        "Succ",
        lambda(
            "n",
            Expr::Constructor {
                tag: "Succ".to_string(),
                args: vec![var("n")],
                span: sp(),
            },
        ),
    );
    let sum_to = declared_binding(
        "sumTo",
        lambda(
            "n",
            lambda(
                "acc",
                match_expr(
                    var("n"),
                    vec![
                        arm(pctor("Zero", vec![]), var("acc")),
                        arm(
                            pctor("Succ", vec![pvar("k")]),
                            apply(
                                apply(var("sumTo"), var("k")),
                                binop(BinOp::Add, var("acc"), int(1)),
                            ),
                        ),
                    ],
                ),
            ),
        ),
        CoreType::Function(
            Box::new(CoreType::Adt("Nat".into(), vec![])),
            Box::new(CoreType::Function(
                Box::new(CoreType::Int),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let mut bindings = vec![
        obfusku_core::ast::BindingGroup::Let(zero),
        obfusku_core::ast::BindingGroup::Let(succ),
        obfusku_core::ast::BindingGroup::LetRec(vec![sum_to]),
    ];
    bindings.push(obfusku_core::ast::BindingGroup::Let(binding(
        "result",
        apply(
            apply(
                var("sumTo"),
                apply(var("Succ"), apply(var("Succ"), var("Zero"))),
            ),
            int(0),
        ),
    )));
    let module = Module {
        type_decls: vec![nat_decl],
        bindings,
    };
    let typed = check(&module)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn two_arg_function_with_mismatched_operand_types_is_still_a_real_diagnostic() {
    // The fix must not weaken error detection: `x: ⟁, y: ⌘` used with
    // `x ✚ y` is genuinely ill-typed (§20.2 has no Int+Str case) and
    // must still be reported, never silently accepted or panicked on.
    let add = declared_binding(
        "add",
        lambda("x", lambda("y", binop(BinOp::Add, var("x"), var("y")))),
        CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let ds = check_err_letrec(vec![add]);
    assert!(!describe(&ds).is_empty());
}
