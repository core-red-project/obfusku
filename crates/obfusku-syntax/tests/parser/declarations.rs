//! Value, function, type and import declarations, and annotations.

use super::support::*;

#[test]
fn value_declaration_accepts_a_type_annotation_before_bind() {
    let module = parse_ok("x: ⟁ ≔ 5\n❧");
    let Declaration::Value(vd) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(
        &vd.ty,
        Some(obfusku_syntax::ast::TypeRef::Base(
            obfusku_syntax::ast::BaseType::Int,
            _
        ))
    ));
}

#[test]
fn value_declaration_without_annotation_has_no_declared_type() {
    let module = parse_ok("x ≔ 5\n❧");
    let Declaration::Value(vd) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(vd.ty.is_none());
}

#[test]
fn value_declaration_annotation_composes_with_mutability_and_export() {
    // Annotation comes before '≔'; Mutability/Export still follow '≔'
    // in their already-fixed order (§8.1) — the two modifier families
    // don't interact.
    let module = parse_ok("x: \u{27C1} \u{2254}\u{02da}\u{27F3} 5\n\u{2767}\n");
    let Declaration::Value(vd) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(vd.ty.is_some());
    assert!(vd.is_mutable);
    assert!(vd.is_exported);
}

#[test]
fn type_annotation_is_rejected_on_a_local_binding() {
    // §8.1: the annotation is module-boundary-only (§9); locally the
    // type is always inferred.
    let msg = parse_err("result \u{2254}\n y: \u{27C1} \u{2254} 5\n y\n\u{2767}\n");
    assert!(msg.contains("not valid on a local binding"), "{msg}");
}

// ── ADTs, Match, and Pattern (CONCRETE_SYMBOLIC_GRAMMAR.md §8.3/§10/§11) ─

#[test]
fn generic_adt_declaration_with_sum_body() {
    // The user's own suggested first example, using exactly this
    // grammar's SumBody production — no invented syntax.
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.tag, "Option");
    assert_eq!(td.type_params, vec!["t".to_string()]);
    assert_eq!(td.variants.len(), 2);
    assert_eq!(td.variants[0].tag, "Some");
    assert_eq!(td.variants[0].fields.len(), 1);
    assert_eq!(td.variants[1].tag, "None");
    assert!(td.variants[1].fields.is_empty()); // bare nullary variant, no parens
}

#[test]
fn export_mark_on_a_type_declaration_is_rejected() {
    // P1-2c / §8.4's own 1.0 contract: `TypeDeclaration` is a deliberate
    // exception to the export mark, not a third site for it — every
    // type and its variants are exported unconditionally, with no `⟳`
    // ever written or accepted here at all.
    let msg = parse_err("Shape \u{2254}\u{27F3} { Circle(\u{27C1}) }\n\u{2767}\n");
    assert!(msg.contains("Export"), "{msg}");
}

#[test]
fn non_generic_adt_declaration() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §8.3's own canonical example.
    let module = parse_ok("Shape ≔ { Circle(⧆) ⟢ Rectangle(⧆, ⧆) }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert!(td.type_params.is_empty());
    assert_eq!(td.variants[1].fields.len(), 2);
}

#[test]
fn function_declaration_parses_with_mandatory_annotations() {
    let module = parse_ok("λf(x: ⟁): ⟁ → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert_eq!(fd.name, "f");
    assert_eq!(fd.params.len(), 1);
    assert_eq!(fd.params[0].name, "x");
    assert!(matches!(
        &fd.params[0].ty,
        obfusku_syntax::ast::TypeRef::Base(obfusku_syntax::ast::BaseType::Int, _)
    ));
    assert!(matches!(
        &fd.return_type,
        obfusku_syntax::ast::TypeRef::Base(obfusku_syntax::ast::BaseType::Int, _)
    ));
    assert!(matches!(&fd.body, Expression::Reference(n, _) if n == "x"));
}

#[test]
fn function_declaration_with_multiple_parameters() {
    let module = parse_ok("λf(x: ⟁, y: ⟁): ⟁ → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert_eq!(fd.params.len(), 2);
    assert_eq!(fd.params[1].name, "y");
}

#[test]
fn anonymous_lambda_at_declaration_position_is_still_an_error_without_a_binder() {
    // 'λ(' at declaration position isn't a FunctionDeclaration (no name)
    // and isn't a legal bare-expression declaration either.
    let msg = parse_err("λ(x) → x\n❧");
    assert!(msg.contains("function name"), "{msg}");
}

#[test]
fn zero_parameter_function_declaration_is_rejected() {
    let msg = parse_err("λf(): ⟁ → 1\n❧");
    assert!(msg.contains("at least one parameter"), "{msg}");
}

#[test]
fn function_declaration_missing_parameter_type_annotation_is_rejected() {
    let msg = parse_err("λf(x): ⟁ → x\n❧");
    assert!(msg.contains("':'"), "{msg}");
}

#[test]
fn function_declaration_missing_return_type_annotation_is_rejected() {
    let msg = parse_err("λf(x: ⟁) → x\n❧");
    assert!(msg.contains("':'"), "{msg}");
}

#[test]
fn function_declaration_accepts_export_after_return_arrow() {
    // §8.4 (post-amendment): export mark follows the declaration's own
    // '≔' or '→'; FunctionDeclaration has no '≔', so it's '→⟳'.
    let module = parse_ok("λf(x: ⟁): ⟁ →⟳ x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(fd.is_exported);
}

#[test]
fn function_declaration_without_export_defaults_to_not_exported() {
    let module = parse_ok("λf(x: ⟁): ⟁ → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(!fd.is_exported);
}

#[test]
fn export_mark_immediately_after_lambda_is_rejected() {
    // The superseded `λ⟳f(...)` candidate stays invalid — §8.4 fixed the
    // position to '→⟳', not 'λ⟳'.
    let msg = parse_err("λ⟳f(x: ⟁): ⟁ → x\n❧");
    assert!(msg.contains("function name"), "{msg}");
}

#[test]
fn export_mark_is_rejected_on_a_local_function_declaration() {
    // §8.4: export is restricted to top-level Module declarations by a
    // static check, not a parse-time restriction, since the grammar
    // shape is identical between top-level and local FunctionDeclaration.
    let msg = parse_err("result ≔ λf(x: ⟁): ⟁ →⟳ x\n f(1)\n❧");
    assert!(
        msg.contains("not valid on a local function declaration"),
        "{msg}"
    );
}

#[test]
fn anonymous_lambda_in_expression_position_is_unaffected_by_function_declaration() {
    // Anonymous Lambda (no annotations, no name) must still work exactly
    // as before at expression position — FunctionDeclaration only claims
    // declaration position.
    let module = parse_ok("f ≔ λ(x) → x\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Lambda { .. }));
}

// ── P0: anonymous `Lambda`'s optional `Parameter` annotation (§7.3) ────

#[test]
fn lambda_parameter_accepts_an_optional_type_annotation() {
    let module = parse_ok("f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x\n\u{2767}\n");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::Lambda { params, .. } = &decl.value else {
        panic!("expected a Lambda")
    };
    assert!(matches!(
        &params[0].ty,
        Some(obfusku_syntax::ast::TypeRef::Base(
            obfusku_syntax::ast::BaseType::Int,
            _
        ))
    ));
}

#[test]
fn lambda_parameter_without_annotation_has_no_declared_type() {
    let module = parse_ok("f \u{2254} \u{3bb}(x) \u{2192} x\n\u{2767}\n");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::Lambda { params, .. } = &decl.value else {
        panic!("expected a Lambda")
    };
    assert!(params[0].ty.is_none());
}

#[test]
fn lambda_multiple_parameters_mix_annotated_and_unannotated() {
    let module = parse_ok("f \u{2254} \u{3bb}(x: \u{27C1}, y) \u{2192} x\n\u{2767}\n");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::Lambda { params, .. } = &decl.value else {
        panic!("expected a Lambda")
    };
    assert!(params[0].ty.is_some());
    assert!(params[1].ty.is_none());
}

// ---------------------------------------------------------------------
// Arithmetic / comparison / equality / logical operators (§7.1a/§13,
// §20.2).
// ---------------------------------------------------------------------

#[test]
fn bare_word_int_parses_as_an_ordinary_tag_never_as_the_base_type() {
    // "Int" as a bare word is never an alias for `⟁` — it parses as
    // `TypeRef::Named` (an ordinary Tag reference), never `TypeRef::Base`.
    let module = parse_ok("λf(x: Int): Int → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(matches!(&fd.params[0].ty, TypeRef::Named(n, _) if n == "Int"));
    assert!(!matches!(&fd.params[0].ty, TypeRef::Base(..)));
}

#[test]
fn a_user_declared_tag_still_works_as_an_annotation() {
    let module = parse_ok("Nat ≔ { Zero ⟢ Succ(Nat) }\nλf(x: Nat): Nat → x\n❧");
    let Declaration::Function(fd) = &module.declarations[1] else {
        panic!("expected a Function declaration")
    };
    assert!(matches!(&fd.params[0].ty, TypeRef::Named(n, _) if n == "Nat"));
}

#[test]
fn function_typed_return_needs_explicit_parens_not_ambiguous_with_the_declarations_own_arrow() {
    let module = parse_ok("λcompose(f: (⟁ → ⟁)): (⟁ → ⟁) → f\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(matches!(&fd.return_type, TypeRef::Function(_, _, _)));
}

#[test]
fn type_application_parses_and_binds_tighter_than_function_type() {
    // g: Array ▷ Int → Int == (Array ▷ Int) → Int, per §9's own example.
    let module = parse_ok("Array t ≔ { Empty }\nλg(x: Array ▷ ⟁): ⟁ → 1\n❧");
    let Declaration::Function(fd) = &module.declarations[1] else {
        panic!("expected a Function declaration")
    };
    match &fd.params[0].ty {
        TypeRef::Apply(lhs, rhs, _) => {
            assert!(matches!(**lhs, TypeRef::Named(ref n, _) if n == "Array"));
            assert!(matches!(**rhs, TypeRef::Base(BaseType::Int, _)));
        }
        other => panic!("expected a TypeApplication, got {other:?}"),
    }
}

// ── RecordBody / TupleBody (ABSTRACT_GRAMMAR.md §2.3/§4,
//    CONCRETE_SYMBOLIC_GRAMMAR.md §8.3/§9) ────────────────────────────

#[test]
fn record_body_declaration_parses_one_self_tagged_variant_with_field_names() {
    let module = parse_ok("Point ≔ { x: ⧆ ⟢ y: ⧆ }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.tag, "Point");
    assert_eq!(td.variants.len(), 1);
    assert_eq!(td.variants[0].tag, "Point");
    assert_eq!(td.variants[0].fields.len(), 2);
    assert_eq!(
        td.variants[0].field_names,
        Some(vec!["x".to_string(), "y".to_string()])
    );
}

#[test]
fn tuple_body_declaration_parses_one_self_tagged_variant_with_no_field_names() {
    let module = parse_ok("Pair ≔ (⟁, ⌘)\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.tag, "Pair");
    assert_eq!(td.variants.len(), 1);
    assert_eq!(td.variants[0].tag, "Pair");
    assert_eq!(td.variants[0].fields.len(), 2);
    assert_eq!(td.variants[0].field_names, None);
}

#[test]
fn sum_body_variant_still_has_no_field_names() {
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.variants[0].field_names, None);
}

#[test]
fn named_construction_parses_tag_and_fields() {
    let module = parse_ok("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1.0 ⟢ y: 2.0 }\n❧");
    let Declaration::Value(vd) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    let Expression::NamedConstruction { tag, fields, .. } = &vd.value else {
        panic!("expected a NamedConstruction, got {:?}", vd.value)
    };
    assert_eq!(tag, "Point");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "x");
    assert_eq!(fields[1].0, "y");
}

#[test]
fn named_constructor_pattern_parses_tag_and_fields() {
    let module = parse_ok(
        r"
        Point ≔ { x: ⧆ ⟢ y: ⧆ }
        p ≔ Point { x: 1.0 ⟢ y: 2.0 }
        q ≔ ⟡ p {
          Point { x: a ⟢ y: b } → a
        }
        ❧
        ",
    );
    let Declaration::Value(decl) = &module.declarations[2] else {
        panic!("expected a Value declaration")
    };
    let Expression::Match { arms, .. } = &decl.value else {
        panic!("expected a Match, got {:?}", decl.value)
    };
    match &arms[0].pattern {
        obfusku_syntax::ast::Pattern::NamedConstructor { tag, fields, .. } => {
            assert_eq!(tag, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "x");
            assert_eq!(fields[1].0, "y");
        }
        other => panic!("expected a NamedConstructor pattern, got {other:?}"),
    }
}

// ── LocalBinding (ABSTRACT_GRAMMAR.md §3.6, CONCRETE_SYMBOLIC_GRAMMAR.md
//    §7.4: `ValueDeclaration Expression` | `FunctionDeclaration+
//    Expression`) ────────────────────────────────────────────────────

#[test]
fn import_declaration_parses_the_module_name() {
    let module = parse_ok("\u{27F2}shapes\n\u{2767}\n");
    let Declaration::Import(id) = &module.declarations[0] else {
        panic!("expected an Import declaration")
    };
    assert_eq!(id.module_name, "shapes");
}

#[test]
fn import_declaration_missing_a_module_name_is_rejected() {
    let msg = parse_err("\u{27F2}\n\u{2767}\n");
    assert!(msg.contains("expected a module name"), "{msg}");
}

#[test]
fn multiple_import_declarations_parse_in_order() {
    let module = parse_ok("\u{27F2}shapes\n\u{27F2}colors\nx \u{2254} 5\n\u{2767}\n");
    let Declaration::Import(a) = &module.declarations[0] else {
        panic!("expected an Import declaration")
    };
    let Declaration::Import(b) = &module.declarations[1] else {
        panic!("expected an Import declaration")
    };
    assert_eq!(a.module_name, "shapes");
    assert_eq!(b.module_name, "colors");
    assert!(matches!(module.declarations[2], Declaration::Value(_)));
}
