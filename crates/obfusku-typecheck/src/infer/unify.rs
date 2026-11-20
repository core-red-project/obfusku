//! The substitution and unification.

use super::{Checker, Infer, reject};
use crate::types::{Type, TypeVarId};
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashSet;

impl Checker {
    /// Applies everything learned so far to `ty`, chasing chains of
    /// variables to their end.
    pub(crate) fn resolve(&self, ty: &Type) -> Type {
        ty.replace_vars(&|v| self.subst.get(&v).map(|bound| self.resolve(bound)))
    }

    /// Whether binding `v` to `ty` would make `v` contain itself.
    fn occurs(&self, v: TypeVarId, ty: &Type) -> bool {
        let mut vars = HashSet::new();
        self.resolve(ty).free_vars(&mut vars);
        vars.contains(&v)
    }

    /// Makes `a` and `b` the same type by extending the substitution, or
    /// says why they cannot be.
    pub(crate) fn unify(&mut self, a: &Type, b: &Type, span: Span) -> Result<(), Diagnostic> {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Type::Var(v1), Type::Var(v2)) if v1 == v2 => Ok(()),
            (Type::Var(v), t) | (t, Type::Var(v)) => {
                if self.occurs(*v, t) {
                    Err(Diagnostic::error(
                        format!("infinite type: {} occurs within itself", t.describe()),
                        span,
                    ))
                } else {
                    self.subst.insert(*v, t.clone());
                    Ok(())
                }
            }
            (Type::Int, Type::Int)
            | (Type::Real, Type::Real)
            | (Type::Str, Type::Str)
            | (Type::Bool, Type::Bool)
            | (Type::Unit, Type::Unit) => Ok(()),
            (Type::Function(a1, r1), Type::Function(a2, r2)) => {
                self.unify(a1, a2, span)?;
                self.unify(r1, r2, span)
            }
            (Type::Cell(t1), Type::Cell(t2)) => self.unify(t1, t2, span),
            (Type::Adt(n1, args1), Type::Adt(n2, args2))
                if n1 == n2 && args1.len() == args2.len() =>
            {
                args1
                    .iter()
                    .zip(args2)
                    .try_for_each(|(x, y)| self.unify(x, y, span))
            }
            _ => Err(Diagnostic::error(
                format!("expected {}, found {}", a.describe(), b.describe()),
                span,
            )),
        }
    }

    /// [`Checker::unify`], for callers whose own result is a list of
    /// diagnostics.
    pub(crate) fn expect_same(&mut self, a: &Type, b: &Type, span: Span) -> Infer<()> {
        self.unify(a, b, span).map_err(|d| vec![d])
    }

    /// Unifies, replacing the generic mismatch message with one that names
    /// the construct (`message` runs *after* the failed attempt, so it prints
    /// the types as resolved then).
    pub(crate) fn expect_same_or(
        &mut self,
        a: &Type,
        b: &Type,
        span: Span,
        message: impl FnOnce(&Checker) -> String,
    ) -> Infer<()> {
        match self.unify(a, b, span) {
            Ok(()) => Ok(()),
            Err(_) => reject(span, message(self)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{checker, span};
    use super::*;

    fn unify(c: &mut Checker, a: &Type, b: &Type) -> Result<(), Diagnostic> {
        c.unify(a, b, span())
    }

    #[test]
    fn equal_primitives_unify() {
        let mut c = checker();
        assert!(unify(&mut c, &Type::Int, &Type::Int).is_ok());
    }

    #[test]
    fn different_primitives_report_both_sides() {
        let mut c = checker();
        let error = unify(&mut c, &Type::Int, &Type::Str).expect_err("mismatch");
        assert_eq!(error.message, "expected Int, found String");
    }

    #[test]
    fn a_variable_takes_the_type_it_meets() {
        let mut c = checker();
        let v = c.fresh();
        unify(&mut c, &v, &Type::Bool).expect("binds");
        assert_eq!(c.resolve(&v), Type::Bool);
    }

    #[test]
    fn chains_of_variables_resolve_to_the_end() {
        let mut c = checker();
        let (a, b) = (c.fresh(), c.fresh());
        unify(&mut c, &a, &b).expect("links");
        unify(&mut c, &b, &Type::Real).expect("binds");
        assert_eq!(c.resolve(&a), Type::Real);
    }

    #[test]
    fn a_variable_cannot_contain_itself() {
        let mut c = checker();
        let v = c.fresh();
        let error = unify(&mut c, &v, &Type::cell(v.clone())).expect_err("occurs check");
        assert!(error.message.starts_with("infinite type"));
    }

    #[test]
    fn functions_unify_parameter_and_result() {
        let mut c = checker();
        let v = c.fresh();
        let f = Type::function(v.clone(), Type::Int);
        unify(&mut c, &f, &Type::function(Type::Str, Type::Int)).expect("unifies");
        assert_eq!(c.resolve(&v), Type::Str);
    }

    #[test]
    fn adts_with_different_names_or_arities_do_not_unify() {
        let mut c = checker();
        let a = Type::Adt("A".into(), vec![]);
        let b = Type::Adt("B".into(), vec![]);
        assert!(unify(&mut c, &a, &b).is_err());
        let wide = Type::Adt("A".into(), vec![Type::Int]);
        assert!(unify(&mut c, &a, &wide).is_err());
    }

    #[test]
    fn cells_are_invariant_in_their_element() {
        let mut c = checker();
        let error = unify(&mut c, &Type::cell(Type::Int), &Type::cell(Type::Str));
        assert!(error.is_err());
    }

    #[test]
    fn a_custom_message_sees_the_types_as_resolved_at_failure() {
        let mut c = checker();
        let v = c.fresh();
        unify(&mut c, &v, &Type::Int).expect("binds");
        let errors = c
            .expect_same_or(&v, &Type::Str, span(), |c| format!("had {}", c.show(&v)))
            .expect_err("mismatch");
        assert_eq!(errors[0].message, "had Int");
    }
}
