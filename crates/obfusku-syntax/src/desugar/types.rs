//! Type annotations: surface `TypeRef` to Core `Type`.

use crate::ast::{BaseType, FunctionDeclaration, TypeDeclaration, TypeRef, VariantDecl};
use obfusku_core::ast::{self as core};
use obfusku_core::types::Type;
use std::collections::HashMap;

/// The Core type an annotation denotes.
pub(super) fn lower_type_ref(ty: &TypeRef) -> Type {
    match ty {
        TypeRef::Base(BaseType::Int, _) => Type::Int,
        TypeRef::Base(BaseType::Real, _) => Type::Real,
        TypeRef::Base(BaseType::Str, _) => Type::Str,
        TypeRef::Base(BaseType::Bool, _) => Type::Bool,
        TypeRef::Base(BaseType::Unit, _) => Type::Unit,
        TypeRef::Var(name, _) => Type::Param(name.clone()),
        TypeRef::Named(name, _) => Type::Adt(name.clone(), Vec::new()),
        TypeRef::Function(lhs, rhs, _) => Type::function(lower_type_ref(lhs), lower_type_ref(rhs)),
        // `Array ▷ Int` folds onto `Adt(name, args)`: each `▷` appends one
        // type argument, so `Result ▷ Int ▷ Str` accumulates two.
        TypeRef::Apply(lhs, rhs, _) => {
            let arg = lower_type_ref(rhs);
            match lower_type_ref(lhs) {
                Type::Adt(name, mut args) => {
                    args.push(arg);
                    Type::Adt(name, args)
                }
                // Not reachable from a valid program: the left side of `▷`
                // must resolve to a tag.
                other => other,
            }
        }
    }
}

/// `λf(x: A, y: B): C → …` declares `A → (B → C)`.
pub(super) fn declared_function_type(function: &FunctionDeclaration) -> Type {
    Type::curried(
        function.params.iter().map(|p| lower_type_ref(&p.ty)),
        lower_type_ref(&function.return_type),
    )
}

/// Every type-variable name in `ty`, repeats included (the caller
/// deduplicates by inserting into a map).
pub(super) fn type_var_names(ty: &TypeRef, out: &mut Vec<String>) {
    match ty {
        TypeRef::Base(..) | TypeRef::Named(..) => {}
        TypeRef::Var(name, _) => out.push(name.clone()),
        TypeRef::Function(lhs, rhs, _) | TypeRef::Apply(lhs, rhs, _) => {
            type_var_names(lhs, out);
            type_var_names(rhs, out);
        }
    }
}

/// `ty` with every parameter named in `rename` replaced.
///
/// Used to give an anonymous lambda's annotations their own module-unique
/// variable names. That has to happen at the surface-to-Core boundary: once
/// a parameter list is curried into a Core lambda chain, `λ(x: t) → λ(y: t) →
/// …` (two separate lambdas, so two separate `t`s) and `λ(x: t, y: t) → …`
/// (one list, one shared `t`) are structurally identical, and the checker
/// cannot tell them apart.
pub(super) fn rename_type_params(ty: Type, rename: &HashMap<String, String>) -> Type {
    match ty {
        Type::Param(name) => match rename.get(&name) {
            Some(renamed) => Type::Param(renamed.clone()),
            None => Type::Param(name),
        },
        Type::Function(a, b) => Type::function(
            rename_type_params(*a, rename),
            rename_type_params(*b, rename),
        ),
        Type::Cell(t) => Type::cell(rename_type_params(*t, rename)),
        Type::Adt(name, args) => Type::Adt(
            name,
            args.into_iter()
                .map(|a| rename_type_params(a, rename))
                .collect(),
        ),
        other @ (Type::Int | Type::Real | Type::Str | Type::Bool | Type::Unit) => other,
    }
}

/// A surface variant as a Core one.
pub(super) fn lower_variant(variant: &VariantDecl) -> core::VariantDecl {
    core::VariantDecl {
        tag: variant.tag.clone(),
        fields: variant.fields.iter().map(lower_type_ref).collect(),
        field_names: variant.field_names.clone(),
        span: variant.span,
    }
}

/// A surface type declaration as a Core one.
pub(super) fn lower_type_decl(decl: &TypeDeclaration) -> core::TypeDecl {
    core::TypeDecl {
        tag: decl.tag.clone(),
        type_params: decl.type_params.clone(),
        variants: decl.variants.iter().map(lower_variant).collect(),
        span: decl.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Declaration;
    use crate::lexer::tokenize;
    use crate::parser::parse;
    use obfusku_diagnostics::SourceId;

    fn annotation(ty: &str) -> TypeRef {
        let source = format!("x: {ty} ≔ 0\n❧");
        let tokens = tokenize(&source, SourceId(0)).unwrap();
        match parse(&tokens, SourceId(0)).unwrap().declarations.remove(0) {
            Declaration::Value(v) => v.ty.unwrap(),
            other => panic!("expected a value, got {other:?}"),
        }
    }

    fn lowered(ty: &str) -> Type {
        lower_type_ref(&annotation(ty))
    }

    #[test]
    fn base_glyphs_map_to_base_types() {
        assert_eq!(lowered("⟁"), Type::Int);
        assert_eq!(lowered("⧆"), Type::Real);
        assert_eq!(lowered("⌘"), Type::Str);
        assert_eq!(lowered("○"), Type::Bool);
        assert_eq!(lowered("∅"), Type::Unit);
    }

    #[test]
    fn names_and_variables() {
        assert_eq!(lowered("Foo"), Type::Adt("Foo".into(), vec![]));
        assert_eq!(lowered("a"), Type::Param("a".into()));
    }

    #[test]
    fn arrows_are_right_nested() {
        assert_eq!(
            lowered("⟁ → ⌘ → ○"),
            Type::function(Type::Int, Type::function(Type::Str, Type::Bool))
        );
    }

    #[test]
    fn each_application_appends_one_type_argument() {
        assert_eq!(
            lowered("Result ▷ ⟁ ▷ ⌘"),
            Type::Adt("Result".into(), vec![Type::Int, Type::Str])
        );
    }

    #[test]
    fn type_var_names_are_collected_with_repeats() {
        let mut out = Vec::new();
        type_var_names(&annotation("a → b ▷ a"), &mut out);
        assert_eq!(out, ["a", "b", "a"]);
    }

    #[test]
    fn renaming_replaces_only_the_named_parameters() {
        let rename = HashMap::from([("a".to_string(), "$tv0".to_string())]);
        let ty = Type::function(Type::Param("a".into()), Type::Param("b".into()));
        assert_eq!(
            rename_type_params(ty, &rename),
            Type::function(Type::Param("$tv0".into()), Type::Param("b".into()))
        );
    }

    #[test]
    fn renaming_reaches_inside_cells_and_applications() {
        let rename = HashMap::from([("a".to_string(), "z".to_string())]);
        let ty = Type::cell(Type::Adt("L".into(), vec![Type::Param("a".into())]));
        assert_eq!(
            rename_type_params(ty, &rename),
            Type::cell(Type::Adt("L".into(), vec![Type::Param("z".into())]))
        );
    }
}
