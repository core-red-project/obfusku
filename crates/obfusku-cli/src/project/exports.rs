//! What a checked and evaluated module offers to the code that imports it.
//!
//! The stdlib, imported modules and the REPL all turn a module's typed
//! bindings and evaluated values into prelude entries. Doing it here, once,
//! keeps the three in agreement about which names are withheld
//! ([`BUILTIN_EXPORTS`]) and how a binding's type becomes a [`Scheme`].

use obfusku_runtime::Value;
use obfusku_typecheck::{Prelude, Scheme, TypeResult, TypedModule};
use std::collections::HashMap;

/// Built-in exception constructors.
///
/// Desugaring synthesizes them into *every* module (`SEMANTIC_CORE.md` §15), so
/// re-exporting them would make each importer collide with its own copy.
pub(crate) const BUILTIN_EXPORTS: [&str; 5] = [
    "DivisionByZero",
    "NonExhaustiveMatch",
    "InvalidOperation",
    "Failure",
    "IntegerOverflow",
];

fn is_builtin(name: &str) -> bool {
    BUILTIN_EXPORTS.contains(&name)
}

/// The type scheme a binding is published under: a monomorphic result is
/// lifted into the checker's own type representation with no variables.
pub(crate) fn scheme_of(ty: &TypeResult) -> Scheme {
    match ty {
        TypeResult::Monomorphic(core) => Scheme::monomorphic(
            obfusku_typecheck::instantiate_core_type(core, &HashMap::new()),
        ),
        TypeResult::Polymorphic(scheme) => scheme.clone(),
    }
}

/// Type schemes of `typed`'s bindings. With `only_exported`, a binding must
/// carry the `⟳` mark (`ADR-005`); the stdlib and the REPL expose every
/// binding because there is nothing else to keep private from.
pub(crate) fn export_types(typed: &TypedModule, only_exported: bool) -> Prelude {
    typed
        .bindings
        .iter()
        .filter(|b| (b.exported || !only_exported) && !is_builtin(&b.name))
        .map(|b| (b.name.clone(), scheme_of(&b.ty)))
        .collect()
}

/// Values of an evaluated module, selected like [`export_types`].
pub(crate) fn export_values(
    evaluated: Vec<(String, bool, Value)>,
    only_exported: bool,
) -> HashMap<String, Value> {
    evaluated
        .into_iter()
        .filter(|(name, exported, _)| (*exported || !only_exported) && !is_builtin(name))
        .map(|(name, _, value)| (name, value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(source: &str) -> TypedModule {
        let mut map = obfusku_diagnostics::SourceMap::new();
        let id = map.add_file(source);
        let core = obfusku_syntax::parse_and_desugar(source, id).expect("parses");
        obfusku_typecheck::check(&core).expect("type-checks")
    }

    #[test]
    fn only_exported_bindings_are_offered_to_importers() {
        let typed = checked("a ≔⟳ 1\nb ≔ 2\n❧\n");
        let types = export_types(&typed, true);
        assert!(types.contains_key("a") && !types.contains_key("b"));
    }

    #[test]
    fn the_stdlib_and_repl_see_every_binding() {
        let typed = checked("a ≔⟳ 1\nb ≔ 2\n❧\n");
        let types = export_types(&typed, false);
        assert!(types.contains_key("a") && types.contains_key("b"));
    }

    #[test]
    fn builtin_exception_constructors_are_never_exported() {
        let typed = checked("a ≔⟳ 1\n❧\n");
        let types = export_types(&typed, false);
        assert!(BUILTIN_EXPORTS.iter().all(|n| !types.contains_key(*n)));
    }

    #[test]
    fn values_follow_the_same_selection() {
        let evaluated = vec![
            ("a".to_string(), true, Value::Int(1)),
            ("b".to_string(), false, Value::Int(2)),
            ("Failure".to_string(), true, Value::Unit),
        ];
        let only = export_values(evaluated.clone(), true);
        assert_eq!(only.keys().collect::<Vec<_>>(), ["a"]);
        let all = export_values(evaluated, false);
        assert!(all.contains_key("b") && !all.contains_key("Failure"));
    }

    #[test]
    fn a_monomorphic_binding_becomes_a_variable_free_scheme() {
        let typed = checked("a ≔⟳ 1\n❧\n");
        let scheme = scheme_of(&typed.bindings.iter().find(|b| b.name == "a").unwrap().ty);
        assert!(scheme.vars.is_empty());
    }
}
