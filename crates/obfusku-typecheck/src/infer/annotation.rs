//! Turning a written type into one the checker can unify.
//!
//! A declared type uses [`CoreType::Param`] for a type variable the author
//! wrote (`t` in `λf(x: t): t → …`). Each such name becomes a fresh
//! unification variable, which is the whole mechanism behind generic
//! functions: there is no separate notion of a generic declaration.

use super::{Checker, Infer};
use crate::adt::{check_type_arity, free_params};
use crate::types::Type;
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashMap;

/// Converts `core_ty` to the checker's vocabulary, replacing each `Param`
/// according to `fresh_map`.
///
/// # Panics
///
/// If a `Param` has no entry in `fresh_map`. Callers build the map from the
/// type's own free parameters, so a miss is an internal error.
#[must_use]
pub fn instantiate_core_type(core_ty: &CoreType, fresh_map: &HashMap<String, Type>) -> Type {
    convert(core_ty, &|name| {
        fresh_map
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("unbound type parameter '{name}' in a declared field type"))
    })
}

/// [`instantiate_core_type`], but an unmapped `Param` becomes `Unit`.
///
/// For walking declared field types where an arity mismatch is reported
/// elsewhere and this walk must not fail on it.
pub(super) fn instantiate_core_type_lenient(
    core_ty: &CoreType,
    params: &HashMap<String, Type>,
) -> Type {
    convert(core_ty, &|name| {
        params.get(name).cloned().unwrap_or(Type::Unit)
    })
}

/// The one structural conversion; `param` decides what a `Param` becomes.
fn convert(core_ty: &CoreType, param: &impl Fn(&str) -> Type) -> Type {
    match core_ty {
        CoreType::Int => Type::Int,
        CoreType::Real => Type::Real,
        CoreType::Str => Type::Str,
        CoreType::Bool => Type::Bool,
        CoreType::Unit => Type::Unit,
        CoreType::Function(parameter, result) => {
            Type::function(convert(parameter, param), convert(result, param))
        }
        CoreType::Cell(inner) => Type::cell(convert(inner, param)),
        CoreType::Adt(name, args) => Type::Adt(
            name.clone(),
            args.iter().map(|a| convert(a, param)).collect(),
        ),
        CoreType::Param(name) => param(name),
    }
}

impl Checker {
    /// A declared type with a fresh variable per free parameter.
    ///
    /// Arity errors are appended to `errors` and conversion still goes on, so
    /// one bad annotation does not hide the other problems in its group.
    pub(crate) fn instantiate_declared(
        &mut self,
        declared: &CoreType,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) -> Type {
        check_type_arity(declared, &self.adt_registry.adts, span, errors);
        let mut free = Vec::new();
        free_params(declared, &mut free);
        let fresh_map: HashMap<String, Type> =
            free.into_iter().map(|p| (p, self.fresh())).collect();
        instantiate_core_type(declared, &fresh_map)
    }

    /// An annotation on an anonymous `Lambda`'s parameter (§7.3).
    ///
    /// Equal variable names share one variable for the whole module:
    /// desugaring renames them to be module-unique, so two parameters
    /// sharing a name are forced equal and independently renamed ones are
    /// not.
    pub(crate) fn instantiate_lambda_annotation(
        &mut self,
        declared: &CoreType,
        span: Span,
    ) -> Infer<Type> {
        let mut errors = Vec::new();
        check_type_arity(declared, &self.adt_registry.adts, span, &mut errors);
        if !errors.is_empty() {
            return Err(errors);
        }
        let mut free = Vec::new();
        free_params(declared, &mut free);
        let mut fresh_map = HashMap::new();
        for name in free {
            let var = match self.lambda_param_vars.get(&name) {
                Some(existing) => existing.clone(),
                None => {
                    let var = self.fresh();
                    self.lambda_param_vars.insert(name.clone(), var.clone());
                    var
                }
            };
            fresh_map.insert(name, var);
        }
        Ok(instantiate_core_type(declared, &fresh_map))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{checker, span};
    use super::*;

    fn param(name: &str) -> CoreType {
        CoreType::Param(name.to_string())
    }

    #[test]
    fn a_closed_type_converts_structurally() {
        let core = CoreType::function(CoreType::Int, CoreType::cell(CoreType::Str));
        let converted = instantiate_core_type(&core, &HashMap::new());
        assert_eq!(converted, Type::function(Type::Int, Type::cell(Type::Str)));
    }

    #[test]
    fn a_param_becomes_what_the_map_says() {
        let map = HashMap::from([("t".to_string(), Type::Bool)]);
        assert_eq!(instantiate_core_type(&param("t"), &map), Type::Bool);
    }

    #[test]
    #[should_panic(expected = "unbound type parameter 't'")]
    fn an_unmapped_param_is_an_internal_error() {
        instantiate_core_type(&param("t"), &HashMap::new());
    }

    #[test]
    fn the_lenient_conversion_fills_a_gap_with_unit() {
        let converted = instantiate_core_type_lenient(&param("t"), &HashMap::new());
        assert_eq!(converted, Type::Unit);
    }

    #[test]
    fn declared_params_each_get_one_fresh_variable() {
        let mut c = checker();
        let declared = CoreType::function(param("t"), param("t"));
        let ty = c.instantiate_declared(&declared, span(), &mut Vec::new());
        let Type::Function(parameter, result) = ty else {
            panic!("a function type stays a function type");
        };
        assert_eq!(parameter, result);
        assert!(matches!(*parameter, Type::Var(_)));
    }

    #[test]
    fn two_separate_declarations_do_not_share_variables() {
        let mut c = checker();
        let declared = param("t");
        let first = c.instantiate_declared(&declared, span(), &mut Vec::new());
        let second = c.instantiate_declared(&declared, span(), &mut Vec::new());
        assert_ne!(first, second);
    }

    #[test]
    fn an_arity_error_is_collected_and_conversion_continues() {
        let mut c = checker();
        let mut errors = Vec::new();
        let bare_array = CoreType::Adt("Array".into(), vec![]);
        let ty = c.instantiate_declared(&bare_array, span(), &mut errors);
        assert_eq!(errors.len(), 1);
        assert_eq!(ty, Type::Adt("Array".into(), vec![]));
    }

    #[test]
    fn lambda_annotations_share_a_variable_by_name() {
        let mut c = checker();
        let a = c.instantiate_lambda_annotation(&param("t"), span()).expect("ok");
        let b = c.instantiate_lambda_annotation(&param("t"), span()).expect("ok");
        let other = c.instantiate_lambda_annotation(&param("u"), span()).expect("ok");
        assert_eq!(a, b);
        assert_ne!(a, other);
    }

    #[test]
    fn a_lambda_annotation_with_bad_arity_fails() {
        let mut c = checker();
        let bad = CoreType::Adt("Map".into(), vec![CoreType::Int]);
        assert!(c.instantiate_lambda_annotation(&bad, span()).is_err());
    }
}
