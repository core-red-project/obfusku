//! Schemes: instantiating them at a use, generalizing a binding into one,
//! and the value restriction that decides whether it may (`§19.1`).

use super::{Checker, Obligation, TypeEnv};
use crate::types::{Constraint, Scheme, Type, TypeVarId};
use obfusku_core::ast::Expr;
use obfusku_diagnostics::Span;
use std::collections::{HashMap, HashSet};

impl Checker {
    /// Replaces `scheme`'s quantified variables with fresh ones. Each of its
    /// constraints becomes an obligation on the fresh variable. A
    /// monomorphic scheme comes back unchanged.
    pub(crate) fn instantiate(&mut self, scheme: &Scheme, span: Span) -> Type {
        let mapping: HashMap<TypeVarId, Type> =
            scheme.vars.iter().map(|v| (*v, self.fresh())).collect();
        for (var, constraint) in &scheme.constraints {
            if let Some(ty) = mapping.get(var) {
                self.require(ty.clone(), *constraint, span);
            }
        }
        scheme.ty.replace_vars(&|v| mapping.get(&v).cloned())
    }

    /// Rebinds a scheme minted by a different checker to this checker's own
    /// variables, preserving its quantification.
    ///
    /// Identifiers are checker-local, so foreign ones could collide with live
    /// local variables and make `generalize` mistake one for the other
    /// (`ADR-001`). Every scheme crossing a module boundary (the prelude,
    /// imports, natives) goes through here.
    ///
    /// # Panics
    ///
    /// If `scheme` mentions a variable outside its own `vars`. Such a scheme
    /// was already broken where it was built; letting it through would only
    /// relocate the bug.
    pub(crate) fn normalize_scheme(&mut self, scheme: &Scheme) -> Scheme {
        let renamed: Vec<(TypeVarId, TypeVarId)> = scheme
            .vars
            .iter()
            .map(|old| (*old, self.fresh_id()))
            .collect();
        let mapping: HashMap<TypeVarId, Type> = renamed
            .iter()
            .map(|(old, new)| (*old, Type::Var(*new)))
            .collect();
        let ty = scheme.ty.replace_vars(&|v| mapping.get(&v).cloned());
        let vars: Vec<TypeVarId> = renamed.iter().map(|(_, new)| *new).collect();

        let mut remaining = HashSet::new();
        ty.free_vars(&mut remaining);
        assert!(
            remaining.iter().all(|v| vars.contains(v)),
            "normalize_scheme: scheme was not closed over its own `vars` — {scheme:?} still \
             has free variable(s) outside {vars:?} after normalization"
        );

        let constraints = scheme
            .constraints
            .iter()
            .filter_map(|(old, constraint)| {
                renamed
                    .iter()
                    .find(|(candidate, _)| candidate == old)
                    .map(|(_, new)| (*new, *constraint))
            })
            .collect();
        Scheme::constrained(vars, constraints, ty)
    }

    /// Quantifies over every variable free in `ty` but not in `env`.
    ///
    /// Callers must first check [`is_syntactic_value`]. Outstanding
    /// obligations on the quantified variables become the scheme's
    /// constraints; the rest stay pending for a later binding to settle.
    pub(crate) fn generalize(&mut self, ty: &Type, env: &TypeEnv) -> Scheme {
        let resolved = self.resolve(ty);
        let mut ty_vars = HashSet::new();
        resolved.free_vars(&mut ty_vars);

        let mut env_vars = HashSet::new();
        for scheme in env.values() {
            let mut scheme_vars = HashSet::new();
            self.resolve(&scheme.ty).free_vars(&mut scheme_vars);
            env_vars.extend(scheme_vars.into_iter().filter(|v| !scheme.vars.contains(v)));
        }

        let vars: Vec<TypeVarId> = ty_vars.difference(&env_vars).copied().collect();
        let constraints = self.constraints_on(&vars);
        Scheme::constrained(vars, constraints, resolved)
    }

    /// Splits the outstanding obligations: those reducing to a constraint on
    /// one of `vars` are returned, the rest are put back.
    fn constraints_on(&mut self, vars: &[TypeVarId]) -> Vec<(TypeVarId, Constraint)> {
        let mut constraints: Vec<(TypeVarId, Constraint)> = Vec::new();
        for obligation in std::mem::take(&mut self.obligations) {
            let mut residual = Vec::new();
            if let Err(d) = self.reduce(&obligation, &mut HashSet::new(), &mut residual) {
                self.deferred.push(d);
                continue;
            }
            for (var, constraint) in residual {
                if vars.contains(&var) {
                    if !constraints.contains(&(var, constraint)) {
                        constraints.push((var, constraint));
                    }
                } else {
                    self.obligations.push(Obligation {
                        ty: Type::Var(var),
                        constraint,
                        span: obligation.span,
                    });
                }
            }
        }
        constraints
    }

    /// The scheme `value`'s type generalizes to, or `None` when the value
    /// restriction forbids generalizing it.
    pub(crate) fn generalize_value(
        &mut self,
        value: &Expr,
        ty: &Type,
        env: &TypeEnv,
    ) -> Option<Scheme> {
        is_syntactic_value(value).then(|| self.generalize(ty, env))
    }
}

/// Whether `expr` may generalize (§19.1).
///
/// Only forms that cannot allocate a cell or run effects qualify, because a
/// polymorphic mutable cell would let `Cell<t>` be written at one type and
/// read at another. A `Constructor` qualifies only when all its arguments do.
pub(crate) fn is_syntactic_value(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(..) | Expr::Var(..) | Expr::Lambda { .. } => true,
        Expr::Constructor { args, .. } => args.iter().all(is_syntactic_value),
        Expr::Apply { .. }
        | Expr::MutCell { .. }
        | Expr::MutRead { .. }
        | Expr::MutRebind { .. }
        | Expr::Match { .. }
        | Expr::Raise { .. }
        | Expr::Catch { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::Let { .. }
        | Expr::LetRec { .. }
        | Expr::ArrayLiteral { .. }
        | Expr::Index { .. }
        | Expr::Field { .. }
        | Expr::Update { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{apply, checker, int, lambda, span, var};
    use super::*;
    use obfusku_core::ast::Expr;

    #[test]
    fn instantiating_gives_each_use_its_own_variables() {
        let mut c = checker();
        let a = c.fresh_id();
        let identity = Scheme::poly(vec![a], Type::function(Type::Var(a), Type::Var(a)));
        let first = c.instantiate(&identity, span());
        let second = c.instantiate(&identity, span());
        assert_ne!(first, second);
    }

    #[test]
    fn instantiating_a_monomorphic_scheme_changes_nothing() {
        let mut c = checker();
        let scheme = Scheme::monomorphic(Type::Int);
        assert_eq!(c.instantiate(&scheme, span()), Type::Int);
    }

    #[test]
    fn instantiating_a_constrained_scheme_queues_an_obligation() {
        let mut c = checker();
        let a = c.fresh_id();
        let scheme = Scheme::constrained(
            vec![a],
            vec![(a, Constraint::Ord)],
            Type::function(Type::Var(a), Type::Bool),
        );
        c.instantiate(&scheme, span());
        assert_eq!(c.obligations.len(), 1);
    }

    #[test]
    fn generalizing_quantifies_the_variables_the_environment_lacks() {
        let mut c = checker();
        let v = c.fresh();
        let scheme = c.generalize(&Type::function(v.clone(), v), &TypeEnv::new());
        assert_eq!(scheme.vars.len(), 1);
    }

    #[test]
    fn a_variable_free_in_the_environment_stays_monomorphic() {
        let mut c = checker();
        let v = c.fresh();
        let mut env = TypeEnv::new();
        env.insert("x".into(), Scheme::monomorphic(v.clone()));
        let scheme = c.generalize(&Type::function(v.clone(), v), &env);
        assert!(scheme.vars.is_empty());
    }

    #[test]
    fn an_obligation_on_a_quantified_variable_becomes_a_constraint() {
        let mut c = checker();
        let v = c.fresh();
        c.require(v.clone(), Constraint::Eq, span());
        let scheme = c.generalize(&Type::function(v.clone(), v), &TypeEnv::new());
        assert_eq!(scheme.constraints.len(), 1);
        assert_eq!(scheme.constraints[0].1, Constraint::Eq);
        assert!(c.obligations.is_empty());
    }

    #[test]
    fn an_obligation_on_an_outer_variable_stays_pending() {
        let mut c = checker();
        let outer = c.fresh();
        let inner = c.fresh();
        c.require(outer.clone(), Constraint::Show, span());
        let mut env = TypeEnv::new();
        env.insert("x".into(), Scheme::monomorphic(outer));
        let scheme = c.generalize(&inner, &env);
        assert!(scheme.constraints.is_empty());
        assert_eq!(c.obligations.len(), 1);
    }

    #[test]
    fn normalizing_renames_variables_and_keeps_constraints() {
        let mut c = checker();
        let foreign = TypeVarId(900);
        let scheme = Scheme::constrained(
            vec![foreign],
            vec![(foreign, Constraint::Eq)],
            Type::Var(foreign),
        );
        let local = c.normalize_scheme(&scheme);
        assert_ne!(local.vars, scheme.vars);
        assert_eq!(local.ty, Type::Var(local.vars[0]));
        assert_eq!(local.constraints, vec![(local.vars[0], Constraint::Eq)]);
    }

    #[test]
    #[should_panic(expected = "not closed over its own `vars`")]
    fn normalizing_an_open_scheme_is_a_bug_not_a_silent_pass() {
        let mut c = checker();
        c.normalize_scheme(&Scheme::monomorphic(Type::Var(TypeVarId(5))));
    }

    #[test]
    fn only_forms_without_effects_are_syntactic_values() {
        assert!(is_syntactic_value(&int(1)));
        assert!(is_syntactic_value(&var("x")));
        assert!(is_syntactic_value(&lambda("x", var("x"))));
        assert!(!is_syntactic_value(&apply(var("f"), int(1))));
        let cell = Expr::MutCell {
            initial: Box::new(int(0)),
            span: span(),
        };
        assert!(!is_syntactic_value(&cell));
    }

    #[test]
    fn a_constructor_is_a_value_only_if_its_arguments_are() {
        let constructor = |args| Expr::Constructor {
            tag: "Some".into(),
            args,
            span: span(),
        };
        assert!(is_syntactic_value(&constructor(vec![int(1)])));
        assert!(!is_syntactic_value(&constructor(vec![apply(
            var("f"),
            int(1)
        )])));
    }

    #[test]
    fn generalize_value_refuses_an_effectful_expression() {
        let mut c = checker();
        let v = c.fresh();
        let call = apply(var("f"), int(1));
        assert!(c.generalize_value(&call, &v, &TypeEnv::new()).is_none());
        assert!(c.generalize_value(&int(1), &v, &TypeEnv::new()).is_some());
    }
}
