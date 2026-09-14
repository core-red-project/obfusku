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
}

/// Which ADT a constructor belongs to, its type parameters, and its
/// declared field types — which may reference `CoreType::Param` for one
/// of `type_params`, substituted at each use site.
#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub adt_name: String,
    pub type_params: Vec<String>,
    pub field_types: Vec<CoreType>,
}

#[derive(Debug, Default)]
pub struct AdtRegistry {
    pub adts: HashMap<String, AdtInfo>,
    pub variants: HashMap<String, VariantInfo>,
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
            },
        );
        // `Array<T>` (§9.2): registered with no variants (constructed
        // via `Expr::ArrayLiteral`, never `Expr::Constructor`) so
        // `check_type_arity` can validate its annotations too.
        adts.insert(
            "Array".to_string(),
            AdtInfo {
                variants: vec![],
                type_params: vec!["t".to_string()],
            },
        );
        variants.insert(
            "DivisionByZero".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![],
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
            },
        );
        variants.insert(
            "NonExhaustiveMatch".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![],
            },
        );
        variants.insert(
            "InvalidOperation".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![CoreType::Str],
            },
        );
        variants.insert(
            "Failure".to_string(),
            VariantInfo {
                adt_name: "Exception".to_string(),
                type_params: vec![],
                field_types: vec![CoreType::Str, CoreType::Str],
            },
        );

        for td in type_decls {
            adts.insert(
                td.tag.clone(),
                AdtInfo {
                    variants: td.variants.iter().map(|v| v.tag.clone()).collect(),
                    type_params: td.type_params.clone(),
                },
            );
            for v in &td.variants {
                variants.insert(
                    v.tag.clone(),
                    VariantInfo {
                        adt_name: td.tag.clone(),
                        type_params: td.type_params.clone(),
                        field_types: v.fields.clone(),
                    },
                );
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

        Ok(AdtRegistry { adts, variants })
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
            if let Some(info) = adts.get(name) {
                if args.len() != info.type_params.len() {
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
            }
            for a in args {
                check_type_arity(a, adts, span, errors);
            }
        }
    }
}
