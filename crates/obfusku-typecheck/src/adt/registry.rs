//! The registry of declared types and their constructors.

use super::arity::check_type_arity;
use obfusku_core::ast::TypeDecl;
use obfusku_core::exception::BUILTIN_EXCEPTIONS;
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::Diagnostic;
use std::collections::HashMap;

/// The built-in open type every program shares (§15.2).
pub(crate) const EXCEPTION: &str = "Exception";
/// The built-in array type (§9.2).
pub(crate) const ARRAY: &str = "Array";
/// Built-in containers, which have type parameters but no constructors
/// (`ADR-025`); `⌭` (bytes, `ADR-027`) is the one non-generic container.
const CONTAINERS: [(&str, &[&str]); 4] = [
    (ARRAY, &["t"]),
    ("Map", &["k", "v"]),
    ("Set", &["k"]),
    ("⌭", &[]),
];

/// One declared type.
#[derive(Debug, Clone)]
pub(crate) struct AdtInfo {
    /// Variant tags in declaration order; exhaustiveness reports missing
    /// cases in this order.
    pub(crate) variants: Vec<String>,
    /// The declared type parameters; their count is the type's arity.
    pub(crate) type_params: Vec<String>,
    /// An open type can gain variants in modules this one never sees, so a
    /// match over it is exhaustive only with a wildcard or variable arm
    /// (`Exception`, `ADR-026`).
    pub(crate) is_open: bool,
}

/// One constructor: the type it builds and the shape of its fields.
#[derive(Debug, Clone)]
pub(crate) struct VariantInfo {
    /// The type this constructor belongs to.
    pub(crate) adt_name: String,
    /// That type's parameters, substituted afresh at each use.
    pub(crate) type_params: Vec<String>,
    /// Declared field types, which may mention `CoreType::Param` for one of
    /// `type_params`.
    pub(crate) field_types: Vec<CoreType>,
    /// Declared field names, for a record's single variant.
    pub(crate) field_names: Option<Vec<String>>,
}

/// Every type and constructor visible to one module.
#[derive(Debug, Default)]
pub(crate) struct AdtRegistry {
    /// Types by name.
    pub(crate) adts: HashMap<String, AdtInfo>,
    /// Constructors by tag.
    pub(crate) variants: HashMap<String, VariantInfo>,
    /// Record field name → the constructor tags that declare it, so an
    /// unannotated `x.field` can find its record.
    pub(crate) field_owners: HashMap<String, Vec<String>>,
}

impl AdtRegistry {
    /// Builds the registry from the module's own and imported declarations.
    ///
    /// Fails with every arity error found in a declared field type.
    pub(crate) fn build(type_decls: &[TypeDecl]) -> Result<Self, Vec<Diagnostic>> {
        let mut registry = AdtRegistry::default().with_builtins();
        registry.declare(type_decls);
        registry.validate_field_arities(type_decls)?;
        Ok(registry)
    }

    /// Registers `Exception` and the containers.
    ///
    /// The exception variants come from the core's shared table, so the
    /// checker, the desugarer and the runtime cannot disagree about them.
    /// Every payload is `Str`: `Failure`'s two fields are both text, a
    /// deliberate simplification rather than a gap.
    pub(crate) fn with_builtins(mut self) -> Self {
        let tags: Vec<String> = BUILTIN_EXCEPTIONS
            .iter()
            .map(|(tag, _)| (*tag).to_string())
            .collect();
        self.adts.insert(
            EXCEPTION.to_string(),
            AdtInfo {
                variants: tags,
                type_params: Vec::new(),
                is_open: true,
            },
        );
        for (tag, arity) in BUILTIN_EXCEPTIONS {
            self.variants.insert(
                tag.to_string(),
                VariantInfo {
                    adt_name: EXCEPTION.to_string(),
                    type_params: Vec::new(),
                    field_types: vec![CoreType::Str; arity],
                    field_names: None,
                },
            );
        }
        // Registered without variants, so `Expr::Constructor` can never
        // build one, but annotations naming them are still arity-checked.
        for (name, params) in CONTAINERS {
            self.adts.insert(
                name.to_string(),
                AdtInfo {
                    variants: Vec::new(),
                    type_params: params.iter().map(|p| (*p).to_string()).collect(),
                    is_open: false,
                },
            );
        }
        self
    }

    /// Adds user declarations. A declaration of `Exception` extends the
    /// built-in type (`ADR-026`); any other name introduces a new one.
    fn declare(&mut self, type_decls: &[TypeDecl]) {
        for decl in type_decls {
            let adt_meta = self
                .adts
                .entry(decl.tag.clone())
                .or_insert_with(|| AdtInfo {
                    variants: Vec::new(),
                    type_params: decl.type_params.clone(),
                    is_open: false,
                });
            for variant in &decl.variants {
                if !adt_meta.variants.contains(&variant.tag) {
                    adt_meta.variants.push(variant.tag.clone());
                }
                self.variants.insert(
                    variant.tag.clone(),
                    VariantInfo {
                        adt_name: decl.tag.clone(),
                        type_params: decl.type_params.clone(),
                        field_types: variant.fields.clone(),
                        field_names: variant.field_names.clone(),
                    },
                );
                for name in variant.field_names.iter().flatten() {
                    self.field_owners
                        .entry(name.clone())
                        .or_default()
                        .push(variant.tag.clone());
                }
            }
        }
    }

    /// Checks field types last, so a field may name a type declared later
    /// in the same module.
    fn validate_field_arities(&self, type_decls: &[TypeDecl]) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        for decl in type_decls {
            for variant in &decl.variants {
                for field in &variant.fields {
                    check_type_arity(field, &self.adts, variant.span, &mut errors);
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_core::ast::VariantDecl;
    use obfusku_diagnostics::{SourceId, Span};

    fn span() -> Span {
        Span::point(SourceId(0), 0)
    }

    fn variant(tag: &str, fields: Vec<CoreType>) -> VariantDecl {
        VariantDecl {
            tag: tag.to_string(),
            fields,
            field_names: None,
            span: span(),
        }
    }

    fn decl(tag: &str, params: &[&str], variants: Vec<VariantDecl>) -> TypeDecl {
        TypeDecl {
            tag: tag.to_string(),
            type_params: params.iter().map(|p| (*p).to_string()).collect(),
            variants,
            span: span(),
        }
    }

    #[test]
    fn every_builtin_exception_is_registered_with_text_payloads() {
        let registry = AdtRegistry::build(&[]).expect("no declarations to reject");
        for (tag, arity) in BUILTIN_EXCEPTIONS {
            let variant_meta = &registry.variants[tag];
            assert_eq!(variant_meta.adt_name, EXCEPTION);
            assert_eq!(variant_meta.field_types, vec![CoreType::Str; arity]);
        }
    }

    #[test]
    fn exception_is_open_and_lists_its_variants_in_table_order() {
        let registry = AdtRegistry::build(&[]).expect("builds");
        let exception = &registry.adts[EXCEPTION];
        assert!(exception.is_open);
        let tags: Vec<&str> = BUILTIN_EXCEPTIONS.iter().map(|(t, _)| *t).collect();
        assert_eq!(exception.variants, tags);
    }

    #[test]
    fn containers_have_arity_but_no_constructors() {
        let registry = AdtRegistry::build(&[]).expect("builds");
        assert_eq!(registry.adts["Map"].type_params, ["k", "v"]);
        assert!(registry.adts["⌭"].type_params.is_empty());
        assert!(registry.adts[ARRAY].variants.is_empty());
    }

    #[test]
    fn a_declaration_registers_its_type_and_constructors() {
        let option = decl(
            "Option",
            &["t"],
            vec![
                variant("Some", vec![CoreType::Param("t".into())]),
                variant("None", vec![]),
            ],
        );
        let registry = AdtRegistry::build(&[option]).expect("builds");
        assert_eq!(registry.adts["Option"].variants, ["Some", "None"]);
        assert_eq!(registry.variants["Some"].adt_name, "Option");
        assert_eq!(registry.variants["Some"].type_params, ["t"]);
    }

    #[test]
    fn declaring_exception_extends_the_builtin_type() {
        let extra = decl("Exception", &[], vec![variant("Timeout", vec![])]);
        let registry = AdtRegistry::build(&[extra]).expect("builds");
        let exception = &registry.adts[EXCEPTION];
        assert!(exception.is_open);
        assert_eq!(exception.variants.len(), BUILTIN_EXCEPTIONS.len() + 1);
        assert_eq!(
            exception.variants.last().map(String::as_str),
            Some("Timeout")
        );
    }

    #[test]
    fn record_fields_remember_which_constructor_owns_them() {
        let mut point = variant("Point", vec![CoreType::Int, CoreType::Int]);
        point.field_names = Some(vec!["x".into(), "y".into()]);
        let registry = AdtRegistry::build(&[decl("Point", &[], vec![point])]).expect("builds");
        assert_eq!(registry.field_owners["x"], ["Point"]);
        assert_eq!(registry.field_owners["y"], ["Point"]);
    }

    #[test]
    fn a_field_naming_a_type_declared_later_is_accepted() {
        let first = decl(
            "Wrapper",
            &[],
            vec![variant("Wrap", vec![CoreType::Adt("Inner".into(), vec![])])],
        );
        let second = decl("Inner", &[], vec![variant("Leaf", vec![])]);
        assert!(AdtRegistry::build(&[first, second]).is_ok());
    }

    #[test]
    fn a_field_with_the_wrong_type_argument_count_is_rejected() {
        let bad = decl(
            "Holder",
            &[],
            vec![variant("Hold", vec![CoreType::Adt("Array".into(), vec![])])],
        );
        let errors = AdtRegistry::build(&[bad]).expect_err("arity mismatch");
        assert_eq!(errors.len(), 1);
    }
}
