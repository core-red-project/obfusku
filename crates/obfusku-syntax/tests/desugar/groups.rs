//! `Let` versus `LetRec` grouping and declared types.

use super::support::*;

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
            assert!(members[0].is_exported);
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
            assert!(!members[0].is_exported);
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
