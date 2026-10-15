//! ADT registry — built once per module from `obfusku_core::ast::Module`'s
//! `type_decls`, consulted by `Constructor` typing and by pattern
//! checking/exhaustiveness (`SEMANTIC_CORE.md` §9, §17).

use obfusku_core::ast::TypeDecl;
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::collections::HashMap;

/// Variant tags in declaration order (exhaustiveness reports missing
/// cases in this order). `type_params` is the ADT's declared arity.
#[derive(Debug, Clone)]
pub struct AdtInfo {
    pub variants: Vec<String>,
    pub type_params: Vec<String>,
    /// An open type can gain variants in modules this one never sees, so
    /// a match over it is exhaustive only with a wildcard or variable
    /// arm (`Exception`, `ADR-026`).
    pub open: bool,
}

/// Which ADT a constructor belongs to, its type parameters, and its
/// declared field types — which may reference `CoreType::Param` for one
/// of `type_params`, substituted at each use site.
#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub adt_name: String,
    pub type_params: Vec<String>,
    pub field_types: Vec<CoreType>,
    /// Declared field names, for a record's single variant.
    pub field_names: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct AdtRegistry {
    pub adts: HashMap<String, AdtInfo>,
    pub variants: HashMap<String, VariantInfo>,
    /// Record field name → the constructor tags that declare it.
    pub field_owners: HashMap<String, Vec<String>>,
}

impl AdtRegistry {
    pub fn build(type_decls: &[TypeDecl]) -> Result<Self, Vec<Diagnostic>> {
        let mut adts = HashMap::new();
        let mut variants = HashMap::new();

        // `Exception` (§15.2): built-in, never user-declared.
        // `Failure`'s payload is a concrete `Str`, not generic — a
        // deliberate simplification, not a gap.
        adts.insert(
            "Exception".to_string(),
            AdtInfo {
                variants: vec![
                    "DivisionByZero".to_string(),
                    "NonExhaustiveMatch".to_string(),
                    "InvalidOperation".to_string(),
                    "Failure".to_string(),
                    "IntegerOverflow".to_string(),
                ],
                type_params: vec![],
                open: true,
            },
        );
        // `Array<T>` (§9.2), `Map<K, V>` and `Set<K>` (`ADR-025`):
        // registered with no variants (never built by
        // `Expr::Constructor`) so `check_type_arity` validates their
        // annotations too.
        // `⌭` (bytes, `ADR-027`) is the one non-generic container.
        for (name, params) in [
            ("Array", vec!["t"]),
            ("Map", vec!["k", "v"]),
            ("Set", vec!["k"]),
            ("⌭", vec![]),
        ] {
            adts.insert(
                name.to_string(),
                AdtInfo {
                    variants: vec![],
                    type_params: params.into_iter().map(String::from).collect(),
                    open: false,
                },
            );
        }
        variants.insert(
            "DivisionByZero".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![],
                field_names: None,
            },
        );
        // §20.2: any `Int` arithmetic result outside `i64`'s
        // representable range — deterministic and catchable, never a
        // build-profile-dependent panic/silent wrap. Not division by
        // zero (`DivisionByZero` stays separate; `MIN / -1`/`MIN % -1`
        // are the one overflow case those two operators can hit).
        variants.insert(
            "IntegerOverflow".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![],
                field_names: None,
            },
        );
        variants.insert(
            "NonExhaustiveMatch".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![],
                field_names: None,
            },
        );
        variants.insert(
            "InvalidOperation".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![CoreType::Str],
                field_names: None,
            },
        );
        variants.insert(
            "Failure".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![CoreType::Str, CoreType::Str],
                field_names: None,
            },
        );

        let mut field_owners: HashMap<String, Vec<String>> = HashMap::new();
        for td in type_decls {
            // A declaration of `Exception` extends the built-in type
            // (`ADR-026`); any other name introduces a new one.
            let info = adts.entry(td.tag.clone()).or_insert_with(|| AdtInfo {
                variants: Vec::new(),
                type_params: td.type_params.clone(),
                open: false,
            });
            for v in &td.variants {
                if !info.variants.contains(&v.tag) {
                    info.variants.push(v.tag.clone());
                }
                variants.insert(
                    v.tag.clone(),
                    VariantInfo {
                        adt_name: td.tag.clone(),
                        type_params: td.type_params.clone(),
                        field_types: v.fields.clone(),
                        field_names: v.field_names.clone(),
                    },
                );
                for name in v.field_names.iter().flatten() {
                    field_owners
                        .entry(name.clone())
                        .or_default()
                        .push(v.tag.clone());
                }
            }
        }

        // Validate that all field types match declared ADT arities.
        let mut errors = Vec::new();
        for td in type_decls {
            for v in &td.variants {
                for field_ty in &v.fields {
                    check_type_arity(field_ty, &adts, v.span, &mut errors);
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(AdtRegistry {
            adts,
            variants,
            field_owners,
        })
    }
}

/// Collects every distinct `CoreType::Param` name reachable in `ty`, in
/// first-occurrence order, for fresh variable instantiation.
pub(crate) fn free_params(ty: &CoreType, out: &mut Vec<String>) {
    match ty {
        CoreType::Int | CoreType::Real | CoreType::Str | CoreType::Bool | CoreType::Unit => {}
        CoreType::Param(name) => {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        CoreType::Function(a, b) => {
            free_params(a, out);
            free_params(b, out);
        }
        CoreType::Cell(t) => free_params(t, out),
        CoreType::Adt(_, args) => {
            for a in args {
                free_params(a, out);
            }
        }
    }
}

/// Checks every `CoreType::Adt(name, args)` node's arity against `adts`.
/// An unknown `name` is left alone — unresolved-name handling belongs
/// to the caller (e.g. constructor/exhaustiveness lookup).
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
        CoreType::Function(a, b) => {
            check_type_arity(a, adts, span, errors);
            check_type_arity(b, adts, span, errors);
        }
        CoreType::Cell(t) => check_type_arity(t, adts, span, errors),
        CoreType::Adt(name, args) => {
            if let Some(info) = adts.get(name)
                && args.len() != info.type_params.len()
            {
                errors.push(Diagnostic {
                    severity: Severity::Error,
                    message: format!(
                        "'{name}' expects {} type argument(s), found {} — a bare '{name}' \
                             never implicitly supplies its enclosing type's own parameters; \
                             write the explicit type argument(s) (e.g. '{name} ▷ t')",
                        info.type_params.len(),
                        args.len()
                    ),
                    primary: span,
                });
            }
            for a in args {
                check_type_arity(a, adts, span, errors);
            }
        }
    }
}
