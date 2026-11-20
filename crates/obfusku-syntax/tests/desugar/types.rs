//! Constructors, matches, type declarations and type-variable renaming.

use super::support::*;

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
        r"
        Option t ≔ { Some(t) ⟢ None }
        y ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        ",
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
