//! `LetRec` groups, declared types, annotated lambdas and type arity.

use super::support::*;

#[test]
fn self_referential_type_equation_in_a_letrec_group_is_rejected_by_the_occurs_check() {
    // f = λx → λy → f(x) forces R = (type of y) -> R — an infinite
    // type. The LetRec slot's presence must not bypass the occurs check.
    let f_body = lambda("y", apply(var("f"), var("x")));
    let f = binding("f", lambda("x", f_body));
    let ds = check_err_letrec(vec![f]);
    assert!(describe(&ds).contains("infinite type"), "{}", describe(&ds));
}

#[test]
fn letrec_group_members_are_mutually_visible_regardless_of_order() {
    // g = λx → x ; f = λx → g(x) — f references g, declared *after* it
    // within the same group. No error expected.
    let g = binding("g", lambda("x", var("x")));
    let f = binding("f", lambda("x", apply(var("g"), var("x"))));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::LetRec(vec![f, g])],
    };
    assert!(check(&module).is_ok());
}

#[test]
fn declared_value_type_rejects_an_incompatible_value() {
    let x = declared_binding("x", string("hi"), CoreType::Int);
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(x)],
    };
    match check(&module) {
        Err(errs) => assert!(!errs.is_empty()),
        Ok(_) => panic!("declared Int, actual Str must be rejected"),
    }
}

#[test]
fn declared_value_type_accepts_a_compatible_value() {
    let x = declared_binding("x", int(5), CoreType::Int);
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(x)],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

#[test]
fn undeclared_value_type_is_unaffected_and_still_infers_normally() {
    // No `declared_type` at all (the pre-P1-3a default) must behave
    // exactly as before — the new unify-against-declared step is only
    // reached when `declared_type` is `Some`.
    let x = binding("x", int(5));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(x)],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

#[test]
fn annotated_lambda_parameter_rejects_an_incompatible_body() {
    // λ(x: Int) → x ✚ "x" — Int is fine for x, but the body's own use is
    // what should fail, proving the annotation is a real constraint that
    // participates in the body's own inference, not just checked after
    // the fact and discarded.
    let bad_body = Expr::BinaryOp {
        op: obfusku_core::ast::BinOp::Add,
        lhs: Box::new(var("x")),
        rhs: Box::new(string("x")),
        span: sp(),
    };
    let f = binding("f", annotated_lambda("x", CoreType::Int, bad_body));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(f)],
    };
    match check(&module) {
        Err(errs) => assert!(!errs.is_empty()),
        Ok(_) => panic!("Int + Str inside the body must be rejected"),
    }
}

#[test]
fn annotated_lambda_parameter_accepts_a_compatible_body() {
    let f = binding("id", annotated_lambda("x", CoreType::Int, var("x")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(f)],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(
        monomorphic_type(&typed, "id"),
        CoreType::Function(Box::new(CoreType::Int), Box::new(CoreType::Int))
    );
}

#[test]
fn unannotated_lambda_parameter_still_generalizes_as_before() {
    // λ(x) → x, applied at two different types via the ordinary
    // generalization path — proves the new annotated arm didn't disturb
    // the pre-existing unannotated one.
    let id = binding("id", lambda("x", var("x")));
    let a = binding("a", apply(var("id"), int(5)));
    let b = binding("b", apply(var("id"), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(id),
            obfusku_core::ast::BindingGroup::Let(a),
            obfusku_core::ast::BindingGroup::Let(b),
        ],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn two_lambda_parameters_sharing_a_renamed_type_variable_are_forced_equal() {
    // Simulates what `obfusku-syntax::desugar`'s renaming produces for
    // `λ(x: t, y: t) → x` — both parameters annotated with the identical
    // `Type::Param` name, so applying them at incompatible types must
    // fail even though each parameter's own annotation, read in
    // isolation, permits any type.
    let shared = CoreType::Param("$tv0".to_string());
    let inner = annotated_lambda("y", shared.clone(), var("x"));
    let outer = annotated_lambda("x", shared, inner);
    let f = binding("f", outer);
    let apply_both = binding("result", apply(apply(var("f"), int(1)), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(f),
            obfusku_core::ast::BindingGroup::Let(apply_both),
        ],
    };
    match check(&module) {
        Err(errs) => assert!(!errs.is_empty()),
        Ok(_) => panic!("x: Int and y: Str must be rejected — both share the same declared 't'"),
    }
}

#[test]
fn two_lambda_parameters_with_independently_renamed_type_variables_do_not_share() {
    // The contrasting case: two DIFFERENT renamed names (what desugar
    // produces for two separately-written nested Lambdas that happen to
    // reuse the same surface spelling) — applying them at different
    // types must succeed.
    let inner = annotated_lambda("y", CoreType::Param("$tv1".to_string()), var("x"));
    let outer = annotated_lambda("x", CoreType::Param("$tv0".to_string()), inner);
    let f = binding("f", outer);
    let apply_both = binding("result", apply(apply(var("f"), int(1)), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(f),
            obfusku_core::ast::BindingGroup::Let(apply_both),
        ],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn checker_lambda_param_vars_cache_keeps_two_independently_renamed_names_apart() {
    // Post-fix audit case, at the `Checker` level directly: `f` and `g`
    // are two SEPARATE top-level `Let` bindings (siblings, not nested),
    // each carrying its own `param_ty` — desugar would give these
    // genuinely distinct renamed names (`$tv0`/`$tv1`), never the same
    // one, precisely because they come from two unrelated surface
    // parameter lists. Simulated directly here to prove
    // `lambda_param_vars` (the checker-lifetime cache) treats distinct
    // keys as fully independent: applying `f` at Int and `g` at Str in
    // the same `Checker` must both succeed.
    let f = binding(
        "f",
        annotated_lambda("x", CoreType::Param("$tv0".to_string()), var("x")),
    );
    let g = binding(
        "g",
        annotated_lambda("y", CoreType::Param("$tv1".to_string()), var("y")),
    );
    let a = binding("a", apply(var("f"), int(5)));
    let result = binding("result", apply(var("g"), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(f),
            obfusku_core::ast::BindingGroup::Let(g),
            obfusku_core::ast::BindingGroup::Let(a),
            obfusku_core::ast::BindingGroup::Let(result),
        ],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Str);
}

#[test]
fn module_level_generic_identity_no_longer_panics_and_infers_correctly() {
    // λid(x: t): t → x — used to panic with "unbound type parameter
    // 't' in a declared field type" before the fix.
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let typed = check_ok_letrec(vec![id], vec![binding("result", apply(var("id"), int(5)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn self_referential_generic_variant_field_needs_explicit_type_application() {
    // `List t = { Nil | Cons(t, List<t>) }`, correctly using
    // `CoreType::Adt("List", [Param("t")])` for the recursive field
    // (the `List ▷ t` surface form) — must build and type-check cleanly.
    let list_decl = type_decl(
        "List",
        vec!["t"],
        vec![
            variant("Nil", vec![]),
            variant(
                "Cons",
                vec![
                    CoreType::Param("t".to_string()),
                    CoreType::Adt("List".to_string(), vec![CoreType::Param("t".to_string())]),
                ],
            ),
        ],
    );
    let nil = binding(
        "Nil",
        Expr::Constructor {
            tag: "Nil".to_string(),
            args: vec![],
            span: sp(),
        },
    );
    let cons = binding(
        "Cons",
        lambda(
            "h",
            lambda(
                "t",
                Expr::Constructor {
                    tag: "Cons".to_string(),
                    args: vec![var("h"), var("t")],
                    span: sp(),
                },
            ),
        ),
    );
    let xs = binding("xs", apply(apply(var("Cons"), int(1)), var("Nil")));
    let _typed = check_ok_adt(vec![list_decl], vec![nil, cons, xs]);
}

#[test]
fn bare_self_reference_with_wrong_arity_is_a_real_type_error() {
    // Same shape, but the recursive field is `List` (bare, zero
    // arguments) where `List` is declared with one type parameter —
    // must be rejected at registry-build time, not silently accepted as
    // "the zero-argument `List`."
    let list_decl = type_decl(
        "List",
        vec!["t"],
        vec![
            variant("Nil", vec![]),
            variant(
                "Cons",
                vec![
                    CoreType::Param("t".to_string()),
                    CoreType::Adt("List".to_string(), vec![]),
                ],
            ),
        ],
    );
    let ds = check_err_adt(vec![list_decl], vec![]);
    assert!(
        describe(&ds).contains("expects 1 type argument"),
        "{}",
        describe(&ds)
    );
}
