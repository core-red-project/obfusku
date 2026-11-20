//! Local bindings, imports, arrays and indexing.

use super::support::*;

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
