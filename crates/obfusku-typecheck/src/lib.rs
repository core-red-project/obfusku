//! Type checking — `spec/SEMANTIC_CORE.md`'s typing rules: literals,
//! variables, `Lambda`, `Apply`, `MutCell`/`MutRead`/`MutRebind`, and the
//! value restriction (§19.1) governing when a binding may generalize.
//!
//! Operates on [`obfusku_core::ast`] types only — no dependency on
//! `obfusku-syntax` or `obfusku-runtime`.

mod adt;
mod infer;
mod types;

use adt::AdtRegistry;
pub use infer::instantiate_core_type;
use infer::{Checker, TypeEnv};
use obfusku_core::ast::{BindingGroup, Module, TypeDecl};
use obfusku_diagnostics::{Diagnostic, Severity, Span};
pub use types::{Scheme, Type};

/// The type scheme of every name available in the ambient environment
/// (from stdlib, natives, or imported modules) — seeded into a fresh
/// `TypeEnv` before inference.
pub type Prelude = std::collections::HashMap<String, Scheme>;

/// A module that has passed type-checking. Every binding's type is
/// either a fully-resolved [`obfusku_core::types::Type`] or a
/// quantified [`Scheme`], never a bare type variable — an unresolved
/// variable is a type error `check` reports, not carried into a
/// "successful" result.
pub struct TypedModule {
    pub bindings: Vec<TypedBinding>,
    /// Non-fatal diagnostics — currently just unreachable `Match` arms.
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug)]
pub struct TypedBinding {
    pub name: String,
    pub ty: TypeResult,
    pub exported: bool,
}

#[derive(Debug)]
pub enum TypeResult {
    Monomorphic(obfusku_core::types::Type),
    Polymorphic(Scheme),
}

struct Pending {
    name: String,
    exported: bool,
    span: Span,
    /// `Some` if generalized (§19.1); `None` for a monomorphic binding.
    generalized: Option<Scheme>,
    /// May still contain free variables — resolved in the second pass.
    raw_ty: Type,
}

/// Type-checks a Core module.
///
/// Duplicate top-level binding names are a static error, checked before
/// inference runs (a flat binding list has no way to give a reused name
/// real lexical shadowing).
///
/// `LetRec` groups infer and unify every member against its own slot
/// first, generalizing only once the whole group has settled — doing it
/// earlier would let `generalize` mistake a sibling's still-unconstrained
/// placeholder for a genuinely free variable.
///
/// Two passes across the module: a monomorphic binding's type can still
/// contain a free variable right after inference (pinned down only once
/// a later binding uses it), so resolving to the final, variable-free
/// `obfusku_core::types::Type` happens only after every binding is seen.
pub fn check(module: &Module) -> Result<TypedModule, Vec<Diagnostic>> {
    check_with_prelude(module, Prelude::new())
}

/// Same as [`check`], but `prelude` names are already in scope before
/// this module's own bindings are inferred — as if imported. A local
/// binding reusing a prelude name is the same static error as reusing
/// an ordinary top-level name (§20 gives imports no special shadowing
/// rule over locals).
pub fn check_with_prelude(
    module: &Module,
    prelude: Prelude,
) -> Result<TypedModule, Vec<Diagnostic>> {
    check_with_prelude_and_types(module, prelude, Vec::new())
}

/// Same as [`check_with_prelude`], but also merges `imported_type_decls`
/// into this module's own `AdtRegistry` — every `TypeDeclaration` an
/// imported module (or the ambient stdlib) declares, so that a pattern
/// written in *this* module against an imported ADT's constructors can
/// actually resolve them. Without this, only the ordinary value-level
/// `Prelude` (constructor *functions*, as plain `Scheme`s) crosses a
/// module boundary — a real, previously-undiscovered gap: an imported
/// constructor could always be *called* (`Circle(2.0)`, an ordinary
/// `Apply` against an already-resolved `Scheme`), but never *matched
/// against* (`⟡ s { Circle(r) → ... }`), since `Pattern::Constructor`
/// and `Expr::Constructor` both resolve a tag through this `Checker`'s
/// own `AdtRegistry`, built only from `module.type_decls` — the
/// *current* file's own declarations, never an imported one's.
///
/// Non-transitive: only direct imports contribute their types, mirroring
/// the value prelude mechanism. Because `AdtInfo` and `VariantInfo` store
/// named type parameters rather than checker-local `TypeVarId`s, importing
/// type declarations carries no variable collision risk.
pub fn check_with_prelude_and_types(
    module: &Module,
    prelude: Prelude,
    imported_type_decls: Vec<TypeDecl>,
) -> Result<TypedModule, Vec<Diagnostic>> {
    let mut seen: std::collections::HashMap<&str, Span> = std::collections::HashMap::new();
    let mut dup_errors = Vec::new();
    for group in &module.bindings {
        for binding in group.bindings() {
            if prelude.contains_key(binding.name.as_str()) {
                // `prelude` is one flat merge of whole-module imports,
                // the ambient stdlib prelude, and ambient natives (see
                // `obfusku-cli::lib::run_file`/`check_file`) — this
                // crate has no way to tell which of those a given name
                // came from, so the message must stay accurate for all
                // three rather than naming only one of them.
                dup_errors.push(Diagnostic {
                    severity: obfusku_diagnostics::Severity::Error,
                    message: format!(
                        "'{}' is already bound by the ambient prelude (an import, or a stdlib/native name)",
                        binding.name
                    ),
                    primary: binding.span,
                });
            } else if let Some(first_span) = seen.get(binding.name.as_str()) {
                // This crate has no `SourceMap` (operates on Core AST
                // only, per its own header) and so can't render
                // `first_span` as a `line:col` itself — a raw byte
                // offset baked into the message text would be the only
                // alternative, and confusing next to every other
                // diagnostic's real location. A separate `Note`
                // diagnostic at that span lets the renderer (which does
                // have the `SourceMap`) resolve it the same way as the
                // primary error.
                dup_errors.push(Diagnostic {
                    severity: obfusku_diagnostics::Severity::Error,
                    message: format!("'{}' is already bound in this module", binding.name),
                    primary: binding.span,
                });
                dup_errors.push(Diagnostic {
                    severity: obfusku_diagnostics::Severity::Note,
                    message: format!("'{}' first bound here", binding.name),
                    primary: *first_span,
                });
            } else {
                seen.insert(&binding.name, binding.span);
            }
        }
    }
    if !dup_errors.is_empty() {
        return Err(dup_errors);
    }

    // Collision check: duplicate ADT or constructor tags across local
    // and imported declarations are static errors. ADT and constructor
    // tags are checked in separate namespaces to allow record/tuple
    // single-variant self-tagging.
    let all_type_decls: Vec<&TypeDecl> = module
        .type_decls
        .iter()
        .chain(imported_type_decls.iter())
        .collect();
    let mut type_dup_errors = Vec::new();
    fn note_duplicate(name: &str, span: Span, first_span: Span, errors: &mut Vec<Diagnostic>) {
        errors.push(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "'{name}' is declared by more than one type (locally or via an import)"
            ),
            primary: span,
        });
        errors.push(Diagnostic {
            severity: Severity::Note,
            message: format!("'{name}' first declared here"),
            primary: first_span,
        });
    }
    let mut seen_adt_names: std::collections::HashMap<&str, Span> =
        std::collections::HashMap::new();
    let mut seen_variant_tags: std::collections::HashMap<&str, Span> =
        std::collections::HashMap::new();
    for td in &all_type_decls {
        if let Some(&first_span) = seen_adt_names.get(td.tag.as_str()) {
            note_duplicate(&td.tag, td.span, first_span, &mut type_dup_errors);
        } else {
            seen_adt_names.insert(&td.tag, td.span);
        }
        for v in &td.variants {
            if let Some(&first_span) = seen_variant_tags.get(v.tag.as_str()) {
                note_duplicate(&v.tag, td.span, first_span, &mut type_dup_errors);
            } else {
                seen_variant_tags.insert(&v.tag, td.span);
            }
        }
    }
    if !type_dup_errors.is_empty() {
        return Err(type_dup_errors);
    }
    let owned_type_decls: Vec<TypeDecl> = all_type_decls.into_iter().cloned().collect();
    let registry = AdtRegistry::build(&owned_type_decls)?;
    let mut checker = Checker::new(registry);
    // Rebind foreign schemes to this checker's fresh variables (ADR-001).
    let mut env: TypeEnv = prelude
        .into_iter()
        .map(|(name, scheme)| (name, checker.normalize_scheme(&scheme)))
        .collect();
    let mut pending = Vec::new();
    let mut errors = Vec::new();

    for group in &module.bindings {
        match group {
            BindingGroup::Let(binding) => {
                match checker.infer(&binding.value, &env) {
                    Ok(inferred) => {
                        // §8.1: A `ValueDeclaration` annotation is unified
                        // against the inferred type after inference.
                        if let Some(declared) = &binding.declared_type {
                            adt::check_type_arity(
                                declared,
                                &checker.adt_registry().adts,
                                binding.span,
                                &mut errors,
                            );
                            let mut free = Vec::new();
                            adt::free_params(declared, &mut free);
                            let fresh_map: std::collections::HashMap<String, Type> =
                                free.into_iter().map(|p| (p, checker.fresh())).collect();
                            let declared_internal =
                                infer::instantiate_core_type(declared, &fresh_map);
                            if let Err(d) =
                                checker.unify(&inferred, &declared_internal, binding.span)
                            {
                                errors.push(d);
                                continue;
                            }
                        }
                        commit_binding(&mut checker, &mut env, &mut pending, binding, inferred);
                    }
                    Err(mut ds) => errors.append(&mut ds),
                }
            }
            BindingGroup::LetRec(members) => {
                // Every member is necessarily a Lambda (a structural
                // guarantee from desugaring), not re-checked here.
                let mut slots: TypeEnv = TypeEnv::new();
                // A declared annotation's own instantiation (fresh
                // per-member, per §-below), kept past this loop so the
                // second pass can push it *into* the Lambda chain's own
                // parameters rather than only checking it at the end —
                // see `Checker::infer_lambda_chain_against`'s doc
                // comment for why the ordering matters.
                let mut declared_internals: std::collections::HashMap<&str, Type> =
                    std::collections::HashMap::new();
                for m in members {
                    let v = checker.fresh();
                    // Declared annotation unified into the slot before
                    // the body is inferred, so it's a real constraint.
                    if let Some(declared) = &m.declared_type {
                        adt::check_type_arity(
                            declared,
                            &checker.adt_registry().adts,
                            m.span,
                            &mut errors,
                        );
                        // Fresh unification variables for unbound type parameters.
                        let mut free = Vec::new();
                        adt::free_params(declared, &mut free);
                        let fresh_map: std::collections::HashMap<String, Type> =
                            free.into_iter().map(|p| (p, checker.fresh())).collect();
                        let declared_internal = infer::instantiate_core_type(declared, &fresh_map);
                        if let Err(d) = checker.unify(&v, &declared_internal, m.span) {
                            errors.push(d);
                        }
                        declared_internals.insert(m.name.as_str(), declared_internal);
                    }
                    slots.insert(m.name.clone(), Scheme::monomorphic(v));
                }
                let mut group_env = env.clone();
                group_env.extend(slots.iter().map(|(k, v)| (k.clone(), v.clone())));

                let mut inferred_members = Vec::with_capacity(members.len());
                let mut group_failed = false;
                for m in members {
                    let result = match declared_internals.get(m.name.as_str()) {
                        Some(declared_internal) => checker.infer_lambda_chain_against(
                            &m.value,
                            declared_internal,
                            &group_env,
                        ),
                        None => checker.infer(&m.value, &group_env),
                    };
                    match result {
                        Ok(inferred) => {
                            if let Some(slot) = slots.get(&m.name) {
                                if let Type::Var(id) = slot.ty {
                                    if let Err(d) = checker.unify(&Type::Var(id), &inferred, m.span)
                                    {
                                        errors.push(d);
                                        group_failed = true;
                                        continue;
                                    }
                                }
                            }
                            inferred_members.push((m, inferred));
                        }
                        Err(mut ds) => {
                            errors.append(&mut ds);
                            group_failed = true;
                        }
                    }
                }
                if group_failed {
                    continue;
                }
                for (m, inferred) in inferred_members {
                    commit_binding(&mut checker, &mut env, &mut pending, m, inferred);
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut typed_bindings = Vec::new();
    for p in pending {
        let (vars, resolved_ty) = match &p.generalized {
            Some(scheme) => (scheme.vars.clone(), checker.resolve(&scheme.ty)),
            None => (Vec::new(), checker.resolve(&p.raw_ty)),
        };
        if vars.is_empty() {
            match resolved_ty.to_core() {
                Some(core_ty) => typed_bindings.push(TypedBinding {
                    name: p.name,
                    ty: TypeResult::Monomorphic(core_ty),
                    exported: p.exported,
                }),
                None => errors.push(diagnostic_ambiguous(&p.name, p.span)),
            }
        } else {
            typed_bindings.push(TypedBinding {
                name: p.name,
                ty: TypeResult::Polymorphic(Scheme {
                    vars,
                    ty: resolved_ty,
                }),
                exported: p.exported,
            });
        }
    }

    if errors.is_empty() {
        Ok(TypedModule {
            bindings: typed_bindings,
            warnings: checker.warnings,
        })
    } else {
        Err(errors)
    }
}

/// Generalizes (§19.1) if eligible, commits into `env`, and queues for
/// the second pass.
fn commit_binding(
    checker: &mut Checker,
    env: &mut TypeEnv,
    pending: &mut Vec<Pending>,
    binding: &obfusku_core::ast::Binding,
    inferred: Type,
) {
    if infer::is_syntactic_value(&binding.value) {
        let scheme = checker.generalize(&inferred, env);
        env.insert(binding.name.clone(), scheme.clone());
        pending.push(Pending {
            name: binding.name.clone(),
            exported: binding.exported,
            span: binding.span,
            generalized: Some(scheme),
            raw_ty: inferred,
        });
    } else {
        env.insert(binding.name.clone(), Scheme::monomorphic(inferred.clone()));
        pending.push(Pending {
            name: binding.name.clone(),
            exported: binding.exported,
            span: binding.span,
            generalized: None,
            raw_ty: inferred,
        });
    }
}

fn diagnostic_ambiguous(name: &str, span: obfusku_diagnostics::Span) -> Diagnostic {
    Diagnostic {
        severity: obfusku_diagnostics::Severity::Error,
        message: format!(
            "cannot infer a concrete type for '{name}': its type is still ambiguous \
             after checking (not eligible for generalization, but never fully constrained)"
        ),
        primary: span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_core::ast::Module;

    #[test]
    fn check_accepts_an_empty_module() {
        let module = Module::default();
        assert!(check(&module).is_ok());
    }
}
