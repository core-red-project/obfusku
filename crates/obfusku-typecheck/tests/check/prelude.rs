//! Preludes, foreign schemes and generated-operation constraints.

use super::support::*;

#[test]
fn prelude_names_are_in_scope_before_the_module_is_checked() {
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(binding(
            "result",
            apply(var("imported"), int(5)),
        ))],
    };
    let mut prelude = obfusku_typecheck::Prelude::new();
    let fn_ty = CoreType::Function(Box::new(CoreType::Int), Box::new(CoreType::Int));
    prelude.insert(
        "imported".to_string(),
        obfusku_typecheck::Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &fn_ty,
            &std::collections::HashMap::new(),
        )),
    );
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn a_local_binding_reusing_a_prelude_name_is_a_static_error() {
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(binding(
            "imported",
            int(1),
        ))],
    };
    let mut prelude = obfusku_typecheck::Prelude::new();
    prelude.insert(
        "imported".to_string(),
        obfusku_typecheck::Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &CoreType::Int,
            &std::collections::HashMap::new(),
        )),
    );
    let ds = match obfusku_typecheck::check_with_prelude(&module, prelude) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a duplicate-name error"),
    };
    assert!(
        describe(&ds).contains("already bound by the ambient prelude"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn check_without_a_prelude_behaves_exactly_like_check() {
    let typed = check_ok(vec![binding("x", int(5))]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

#[test]
fn a_local_generic_function_still_generalizes_with_a_foreign_generic_scheme_in_the_prelude() {
    let prelude = foreign_generic_identity_prelude();
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::LetRec(vec![id]),
            obfusku_core::ast::BindingGroup::Let(binding("a", apply(var("id"), int(5)))),
            obfusku_core::ast::BindingGroup::Let(binding("b", apply(var("id"), string("hi")))),
        ],
    };
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn an_unused_local_generic_function_type_checks_cleanly_with_a_foreign_generic_scheme_present() {
    // The "declared but never used" symptom found during investigation:
    // an otherwise-valid, unused generic function must not become a
    // spurious "ambiguous type" error just because an unrelated foreign
    // generic scheme sits in the same prelude.
    let prelude = foreign_generic_identity_prelude();
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::LetRec(vec![id]),
            obfusku_core::ast::BindingGroup::Let(binding("result", int(5))),
        ],
    };
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn foreign_prelude_scheme_itself_is_usable_and_still_polymorphic_after_normalization() {
    // Normalization must preserve quantification, not just avoid
    // collisions — the foreign scheme itself must still work as a
    // genuinely polymorphic value once it's in scope.
    let prelude = foreign_generic_identity_prelude();
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(binding("a", apply(var("foreignId"), int(5)))),
            obfusku_core::ast::BindingGroup::Let(binding(
                "b",
                apply(var("foreignId"), string("hi")),
            )),
        ],
    };
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn an_ordering_use_on_a_generalized_parameter_becomes_a_scheme_constraint() {
    use obfusku_core::ast::BinOp;
    // λa. λb. a < b   — generic in a, needs `Ord`.
    let typed = check_ok(vec![binding(
        "lt",
        lambda("a", lambda("b", binop_expr(BinOp::Lt, var("a"), var("b")))),
    )]);
    let b = typed.bindings.iter().find(|b| b.name == "lt").unwrap();
    match &b.ty {
        TypeResult::Polymorphic(scheme) => {
            assert!(
                scheme
                    .constraints
                    .iter()
                    .any(|(_, c)| *c == obfusku_typecheck::Constraint::Ord),
                "{scheme:?}"
            );
        }
        other => panic!("expected a polymorphic binding, got {other:?}"),
    }
}

#[test]
fn a_constrained_generic_is_checked_at_each_call_site() {
    use obfusku_core::ast::BinOp;
    let lt = lambda("a", lambda("b", binop_expr(BinOp::Lt, var("a"), var("b"))));
    // Fine at Int and at Str.
    check_ok(vec![
        binding("lt", lt.clone()),
        binding("i", apply(apply(var("lt"), int(1)), int(2))),
        binding("s", apply(apply(var("lt"), string("a")), string("b"))),
    ]);
    // A function has no ordering.
    let ds = check_err(vec![
        binding("lt", lt),
        binding(
            "bad",
            apply(
                apply(var("lt"), lambda("x", var("x"))),
                lambda("y", var("y")),
            ),
        ),
    ]);
    assert!(describe(&ds).contains("ordering"), "{}", describe(&ds));
}

#[test]
fn a_let_bound_constrained_generic_works_at_two_types() {
    use obfusku_core::ast::BinOp;
    let body = let_expr(
        "lt",
        lambda("a", lambda("b", binop_expr(BinOp::Lt, var("a"), var("b")))),
        binop_expr(
            BinOp::Eq,
            apply(apply(var("lt"), int(1)), int(2)),
            apply(apply(var("lt"), string("a")), string("b")),
        ),
    );
    let typed = check_ok(vec![binding("r", body)]);
    assert_eq!(monomorphic_type(&typed, "r"), CoreType::Bool);
}

#[test]
fn record_field_read_and_multi_field_update_are_typed() {
    let m = module_with(
        vec![point_decl()],
        vec![
            binding("p", point(1, 2)),
            binding("q", update(var("p"), vec![("x", int(10)), ("y", int(20))])),
            binding("r", field(var("q"), "y")),
        ],
    );
    let typed = check(&m).unwrap_or_else(|ds| panic!("{}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "r"), CoreType::Int);
    assert_eq!(
        monomorphic_type(&typed, "q"),
        CoreType::Adt("P".to_string(), vec![])
    );
}
