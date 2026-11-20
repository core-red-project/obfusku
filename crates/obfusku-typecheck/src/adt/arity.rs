//! Checking written types against what they name.

use super::registry::AdtInfo;
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashMap;

/// Collects every distinct [`CoreType::Param`] name in `ty`, in order of
/// first occurrence.
///
/// Each one is later given its own fresh unification variable, which is how a
/// declaration such as `λf(x: t): t → …` becomes generic without any
/// dedicated generic-function mechanism.
pub(crate) fn free_params(ty: &CoreType, out: &mut Vec<String>) {
    match ty {
        CoreType::Int | CoreType::Real | CoreType::Str | CoreType::Bool | CoreType::Unit => {}
        CoreType::Param(name) => {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        CoreType::Function(parameter, result) => {
            free_params(parameter, out);
            free_params(result, out);
        }
        CoreType::Cell(inner) => free_params(inner, out),
        CoreType::Adt(_, args) => args.iter().for_each(|a| free_params(a, out)),
    }
}

/// Checks the argument count of every `Adt` node in `ty` against `adts`.
///
/// A bare name never implicitly supplies its enclosing type's own
/// parameters, so `Option` where `Option ▷ t` is meant is a reported error
/// rather than a silent guess. An unknown name is skipped: reporting it
/// belongs to the constructor and match lookups, which know the context.
pub(crate) fn check_type_arity(
    ty: &CoreType,
    adts: &HashMap<String, AdtInfo>,
    span: Span,
    errors: &mut Vec<Diagnostic>,
) {
    match ty {
        CoreType::Int
        | CoreType::Real
        | CoreType::Str
        | CoreType::Bool
        | CoreType::Unit
        | CoreType::Param(_) => {}
        CoreType::Function(parameter, result) => {
            check_type_arity(parameter, adts, span, errors);
            check_type_arity(result, adts, span, errors);
        }
        CoreType::Cell(inner) => check_type_arity(inner, adts, span, errors),
        CoreType::Adt(name, args) => {
            if let Some(info) = adts.get(name)
                && args.len() != info.type_params.len()
            {
                errors.push(Diagnostic::error(
                    format!(
                        "'{name}' expects {} type argument(s), found {} — a bare '{name}' \
                         never implicitly supplies its enclosing type's own parameters; \
                         write the explicit type argument(s) (e.g. '{name} ▷ t')",
                        info.type_params.len(),
                        args.len()
                    ),
                    span,
                ));
            }
            for arg in args {
                check_type_arity(arg, adts, span, errors);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adt::AdtRegistry;
    use obfusku_diagnostics::SourceId;

    fn span() -> Span {
        Span::point(SourceId(0), 0)
    }

    fn param(name: &str) -> CoreType {
        CoreType::Param(name.to_string())
    }

    #[test]
    fn free_params_lists_each_name_once_in_first_seen_order() {
        let ty = CoreType::function(
            param("b"),
            CoreType::Adt("Pair".into(), vec![param("a"), param("b")]),
        );
        let mut found = Vec::new();
        free_params(&ty, &mut found);
        assert_eq!(found, ["b", "a"]);
    }

    #[test]
    fn a_closed_type_has_no_free_params() {
        let mut found = Vec::new();
        free_params(&CoreType::cell(CoreType::Int), &mut found);
        assert!(found.is_empty());
    }

    #[test]
    fn a_container_with_the_wrong_argument_count_is_reported() {
        let registry = AdtRegistry::default().with_builtins();
        let mut errors = Vec::new();
        check_type_arity(
            &CoreType::Adt("Array".into(), vec![]),
            &registry.adts,
            span(),
            &mut errors,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("expects 1 type argument(s), found 0"));
    }

    #[test]
    fn nested_arity_errors_are_each_reported() {
        let registry = AdtRegistry::default().with_builtins();
        let bare = CoreType::Adt("Array".into(), vec![]);
        let ty = CoreType::function(bare.clone(), CoreType::cell(bare));
        let mut errors = Vec::new();
        check_type_arity(&ty, &registry.adts, span(), &mut errors);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn an_unknown_name_is_left_to_the_caller() {
        let registry = AdtRegistry::default().with_builtins();
        let mut errors = Vec::new();
        check_type_arity(
            &CoreType::Adt("Missing".into(), vec![CoreType::Int]),
            &registry.adts,
            span(),
            &mut errors,
        );
        assert!(errors.is_empty());
    }
}
