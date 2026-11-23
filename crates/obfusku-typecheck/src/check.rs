//! The entry points: checking a whole module.
//!
//! Each entry adds one capability to the previous, so a caller picks the
//! narrowest that fits:
//!
//! | function | adds |
//! |---|---|
//! | [`check`] | nothing: a self-contained module |
//! | [`check_with_prelude`] | names already in scope, as if imported |
//! | [`check_with_prelude_and_types`] | imported type declarations |
//! | [`check_shadowing`] | ambient names a local binding may replace |
//!
//! The work itself is a fixed sequence: reject duplicate names, build the
//! ADT registry, check each binding group in order, then resolve every
//! binding's type in a second pass.

mod names;
mod session;

use crate::adt::AdtRegistry;
use crate::typed::TypedModule;
use crate::types::Scheme;
use obfusku_core::ast::{Module, TypeDecl};
use obfusku_diagnostics::Diagnostic;
use session::Session;
use std::collections::{HashMap, HashSet};

/// The type scheme of every name available from outside the module (stdlib,
/// natives, imported modules), seeded into the environment before inference.
pub type Prelude = HashMap<String, Scheme>;

/// Type-checks a Core module.
///
/// Duplicate top-level names are a static error, found before inference
/// runs: a flat binding list cannot give a reused name real lexical
/// shadowing.
///
/// # Errors
///
/// Every diagnostic found, when the module is ill-typed.
pub fn check(module: &Module) -> Result<TypedModule, Vec<Diagnostic>> {
    check_with_prelude(module, Prelude::new())
}

/// [`check`], with `prelude`'s names in scope as if imported.
///
/// A local binding reusing a prelude name is the same static error as
/// reusing any top-level name (§20 gives imports no special shadowing rule
/// over locals); [`check_shadowing`] relaxes that for ambient names.
///
/// # Errors
///
/// Every diagnostic found, when the module is ill-typed.
pub fn check_with_prelude(
    module: &Module,
    prelude: Prelude,
) -> Result<TypedModule, Vec<Diagnostic>> {
    check_with_prelude_and_types(module, prelude, Vec::new())
}

/// [`check_with_prelude`], also merging `imported_type_decls` into this
/// module's ADT registry.
///
/// Without this only the value-level prelude crosses a module boundary: an
/// imported constructor could be *called* (an ordinary application of a
/// scheme) but never *matched against*, because patterns and `Constructor`
/// resolve a tag through this checker's own registry, which is built only
/// from declarations it is given.
///
/// Imports are non-transitive: only direct imports contribute types,
/// mirroring the value prelude. Type declarations store named parameters
/// rather than checker-local variable ids, so importing them cannot collide.
///
/// # Errors
///
/// Every diagnostic found, when the module is ill-typed.
// The owned `Vec` is the published signature; callers and tests pass one.
#[allow(clippy::needless_pass_by_value)]
pub fn check_with_prelude_and_types(
    module: &Module,
    prelude: Prelude,
    imported_type_decls: Vec<TypeDecl>,
) -> Result<TypedModule, Vec<Diagnostic>> {
    check_shadowing(module, prelude, &imported_type_decls, &HashSet::new())
}

/// [`check_with_prelude_and_types`], where a local binding named in
/// `shadowable` replaces that prelude name instead of colliding with it.
///
/// The caller passes exactly the ambient value names that no import
/// re-binds and that are not constructors: a constructor's name is tied to
/// its type, so it can never be shadowed (`ADR-028`).
///
/// # Errors
///
/// Every diagnostic found, when the module is ill-typed.
pub fn check_shadowing(
    module: &Module,
    mut prelude: Prelude,
    imported_type_decls: &[TypeDecl],
    shadowable: &HashSet<String>,
) -> Result<TypedModule, Vec<Diagnostic>> {
    for binding in module.all_bindings() {
        if shadowable.contains(binding.name.as_str()) {
            prelude.remove(binding.name.as_str());
        }
    }
    names::reject_binding_clashes(module, &prelude)?;

    let type_decls: Vec<TypeDecl> = module
        .type_decls
        .iter()
        .chain(imported_type_decls)
        .cloned()
        .collect();
    names::reject_type_clashes(&type_decls)?;

    let mut session = Session::new(AdtRegistry::build(&type_decls)?, prelude);
    for group in &module.bindings {
        session.check_group(group);
    }
    session.into_result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed::TypeResult;
    use crate::types::Type;
    use obfusku_core::ast::{Binding, BindingGroup, Expr, Literal};
    use obfusku_diagnostics::{SourceId, Span};

    fn span() -> Span {
        Span::point(SourceId(0), 0)
    }

    fn binding(name: &str, value: Expr) -> BindingGroup {
        BindingGroup::Let(Binding {
            name: name.to_string(),
            value,
            is_exported: true,
            span: span(),
            declared_type: None,
        })
    }

    fn int(value: i64) -> Expr {
        Expr::Lit(Literal::Int(value), span())
    }

    fn module(bindings: Vec<BindingGroup>) -> Module {
        Module {
            bindings,
            ..Module::default()
        }
    }

    #[test]
    fn an_empty_module_checks() {
        let typed = check(&Module::default()).expect("nothing to reject");
        assert!(typed.bindings.is_empty() && typed.warnings.is_empty());
    }

    #[test]
    fn a_binding_keeps_its_name_export_flag_and_type() {
        let typed = check(&module(vec![binding("n", int(1))])).expect("checks");
        let only = &typed.bindings[0];
        assert_eq!(only.name, "n");
        assert!(only.is_exported);
        assert!(matches!(
            &only.ty,
            TypeResult::Monomorphic(obfusku_core::types::Type::Int)
        ));
    }

    #[test]
    fn a_later_binding_sees_an_earlier_one() {
        let uses = Expr::Var("n".into(), span());
        let typed = check(&module(vec![binding("n", int(1)), binding("m", uses)]));
        assert_eq!(typed.expect("checks").bindings.len(), 2);
    }

    #[test]
    fn a_prelude_name_is_in_scope() {
        let mut prelude = Prelude::new();
        prelude.insert("answer".into(), Scheme::monomorphic(Type::Int));
        let uses = Expr::Var("answer".into(), span());
        let typed = check_with_prelude(&module(vec![binding("m", uses)]), prelude);
        assert!(typed.is_ok());
    }

    #[test]
    fn a_local_binding_may_replace_a_shadowable_prelude_name_only() {
        let mut prelude = Prelude::new();
        prelude.insert("print".into(), Scheme::monomorphic(Type::Unit));
        let m = module(vec![binding("print", int(1))]);
        assert!(check_with_prelude(&m, prelude.clone()).is_err());
        let shadowable = HashSet::from(["print".to_string()]);
        assert!(check_shadowing(&m, prelude, &[], &shadowable).is_ok());
    }

    #[test]
    fn a_value_that_stays_ambiguous_is_an_error() {
        let empty = Expr::ArrayLiteral {
            elements: vec![],
            span: span(),
        };
        let errors = check(&module(vec![binding("xs", empty)])).expect_err("ambiguous");
        assert!(
            errors[0]
                .message
                .contains("cannot infer a concrete type for 'xs'")
        );
    }

    #[test]
    fn one_bad_binding_does_not_hide_another() {
        let bad = |name: &str| binding(name, Expr::Var("ghost".into(), span()));
        let errors = check(&module(vec![bad("a"), bad("b")])).expect_err("two errors");
        assert_eq!(errors.len(), 2);
    }
}
