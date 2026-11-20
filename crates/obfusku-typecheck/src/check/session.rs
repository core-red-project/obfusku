//! One module's check, binding group by binding group.

use crate::adt::AdtRegistry;
use crate::infer::{Checker, TypeEnv};
use crate::typed::{TypeResult, TypedBinding, TypedModule};
use crate::types::{Scheme, Type};
use obfusku_core::ast::{Binding, BindingGroup};
use obfusku_diagnostics::{Diagnostic, Span};

/// A binding whose type is known but possibly still mentions variables a
/// *later* binding will pin down (`SEMANTIC_CORE.md` §19.1).
struct Pending {
    name: String,
    exported: bool,
    span: Span,
    /// `Some` if the value restriction allowed generalizing.
    generalized: Option<Scheme>,
    /// The type as inferred. Resolved only in the second pass.
    raw_ty: Type,
}

impl Pending {
    /// The final, variable-free form of this binding's type.
    ///
    /// A monomorphic binding that still has a variable never got fully
    /// constrained; that is an ambiguity error, not something to guess past.
    fn resolve(self, checker: &Checker) -> Result<TypedBinding, Diagnostic> {
        let (vars, constraints, ty) = match self.generalized {
            Some(scheme) => (scheme.vars, scheme.constraints, checker.resolve(&scheme.ty)),
            None => (Vec::new(), Vec::new(), checker.resolve(&self.raw_ty)),
        };
        let resolved = if vars.is_empty() {
            let core = ty.to_core().ok_or_else(|| {
                Diagnostic::error(
                    format!(
                        "cannot infer a concrete type for '{}': its type is still ambiguous \
                         after checking (not eligible for generalization, but never fully \
                         constrained)",
                        self.name
                    ),
                    self.span,
                )
            })?;
            TypeResult::Monomorphic(core)
        } else {
            TypeResult::Polymorphic(Scheme::constrained(vars, constraints, ty))
        };
        Ok(TypedBinding {
            name: self.name,
            ty: resolved,
            exported: self.exported,
        })
    }
}

/// The state of checking one module: the checker, the growing environment,
/// the bindings waiting for the second pass, and every error so far.
///
/// Errors accumulate rather than abort, so one bad binding does not hide the
/// problems in the others.
pub(super) struct Session {
    checker: Checker,
    env: TypeEnv,
    pending: Vec<Pending>,
    errors: Vec<Diagnostic>,
}

impl Session {
    /// Starts a session with `prelude` already in scope.
    ///
    /// Each prelude scheme was minted by some other checker, so it is rebound
    /// to this checker's variables first (`ADR-001`).
    pub(super) fn new(registry: AdtRegistry, prelude: super::Prelude) -> Self {
        let mut checker = Checker::new(registry);
        let env = prelude
            .into_iter()
            .map(|(name, scheme)| (name, checker.normalize_scheme(&scheme)))
            .collect();
        Session {
            checker,
            env,
            pending: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Checks one group and adds its bindings to the environment.
    pub(super) fn check_group(&mut self, group: &BindingGroup) {
        match group {
            BindingGroup::Let(binding) => self.check_let(binding),
            BindingGroup::LetRec(members) => self.check_letrec(members),
        }
    }

    /// §8.1: a `ValueDeclaration`'s annotation is unified with the inferred
    /// type *after* inference. It is a real constraint, not a stored label.
    fn check_let(&mut self, binding: &Binding) {
        let inferred = match self.checker.infer(&binding.value, &self.env) {
            Ok(ty) => ty,
            Err(mut diagnostics) => {
                self.errors.append(&mut diagnostics);
                return;
            }
        };
        if let Some(written) = &binding.declared_type {
            let declared =
                self.checker
                    .instantiate_declared(written, binding.span, &mut self.errors);
            if let Err(d) = self.checker.unify(&inferred, &declared, binding.span) {
                self.errors.push(d);
                return;
            }
        }
        self.commit(binding, inferred);
    }

    /// Every member is necessarily a `Lambda` (desugaring guarantees it), so
    /// it is not re-checked here.
    fn check_letrec(&mut self, members: &[Binding]) {
        let Some(types) = self
            .checker
            .infer_group(members, &self.env, &mut self.errors)
        else {
            return;
        };
        for (member, ty) in members.iter().zip(types) {
            self.commit(member, ty);
        }
    }

    /// Generalizes if the value restriction allows, makes the binding
    /// visible to later ones, and queues it for the second pass.
    fn commit(&mut self, binding: &Binding, inferred: Type) {
        let generalized = self
            .checker
            .generalize_value(&binding.value, &inferred, &self.env);
        let scheme = generalized
            .clone()
            .unwrap_or_else(|| Scheme::monomorphic(inferred.clone()));
        self.env.insert(binding.name.clone(), scheme);
        self.pending.push(Pending {
            name: binding.name.clone(),
            exported: binding.exported,
            span: binding.span,
            generalized,
            raw_ty: inferred,
        });
    }

    /// Discharges the remaining obligations, then resolves every binding.
    ///
    /// Two passes because a monomorphic binding's type can still hold a
    /// variable right after inference, pinned down only once a later binding
    /// uses it; the variable-free core type exists only after all are seen.
    pub(super) fn into_result(self) -> Result<TypedModule, Vec<Diagnostic>> {
        let Session {
            mut checker,
            pending,
            mut errors,
            ..
        } = self;
        errors.extend(checker.finish());
        if !errors.is_empty() {
            return Err(errors);
        }
        let mut bindings = Vec::with_capacity(pending.len());
        for binding in pending {
            match binding.resolve(&checker) {
                Ok(typed) => bindings.push(typed),
                Err(d) => errors.push(d),
            }
        }
        if errors.is_empty() {
            Ok(TypedModule {
                bindings,
                warnings: checker.warnings,
            })
        } else {
            Err(errors)
        }
    }
}
