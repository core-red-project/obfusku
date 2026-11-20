//! Generated-operation obligations (`ADR-025`).
//!
//! `≡`, `<` and `⍕` work on every type built from primitives, ADTs and
//! collections, and on no function type. The operand is frequently still a
//! variable when the operator is typed, so the requirement is recorded and
//! reduced later, once enough is known.

use super::annotation::instantiate_core_type_lenient;
use super::{Checker, Obligation};
use crate::types::{Constraint, Type, TypeVarId};
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

impl Checker {
    /// Records that `ty` must support `constraint`.
    pub(crate) fn require(&mut self, ty: Type, constraint: Constraint, span: Span) {
        self.obligations.push(Obligation {
            ty,
            constraint,
            span,
        });
    }

    /// Reduces an obligation to requirements on bare type variables
    /// (appended to `out`), or to the diagnostic it violates.
    pub(super) fn reduce(
        &self,
        obligation: &Obligation,
        seen: &mut HashSet<String>,
        out: &mut Vec<(TypeVarId, Constraint)>,
    ) -> Result<(), Diagnostic> {
        self.reduce_type(
            &obligation.ty,
            obligation.constraint,
            obligation.span,
            seen,
            out,
        )
    }

    /// A type supports a generated operation when no function type occurs
    /// anywhere in its structure (§18, extended by `ADR-025`). `Cell` has
    /// identity equality but no ordering, and shows as its contents.
    ///
    /// `seen` holds the ADT instances already being walked, so a recursive
    /// type terminates.
    fn reduce_type(
        &self,
        ty: &Type,
        constraint: Constraint,
        span: Span,
        seen: &mut HashSet<String>,
        out: &mut Vec<(TypeVarId, Constraint)>,
    ) -> Result<(), Diagnostic> {
        let ty = self.resolve(ty);
        let fail = |why: &str| {
            Diagnostic::error(
                format!(
                    "'{}' has no {} — {why} (SEMANTIC_CORE.md §18)",
                    ty.describe(),
                    constraint.describe(),
                ),
                span,
            )
        };
        match &ty {
            Type::Int | Type::Real | Type::Str | Type::Bool | Type::Unit => Ok(()),
            Type::Var(v) => {
                if !out.contains(&(*v, constraint)) {
                    out.push((*v, constraint));
                }
                Ok(())
            }
            Type::Function(..) => Err(fail("a function value has no generated operations")),
            Type::Cell(inner) => match constraint {
                Constraint::Eq => Ok(()),
                Constraint::Ord => Err(fail("a mutable cell has no order")),
                Constraint::Show => self.reduce_type(inner, constraint, span, seen, out),
            },
            Type::Adt(name, args) => {
                for arg in args {
                    self.reduce_type(arg, constraint, span, seen, out)?;
                }
                // A container (`Array`, `Map`, `Set`) has no variants: its
                // arguments, handled above, are everything.
                let Some(info) = self.adt_registry.adts.get(name) else {
                    return Ok(());
                };
                if !seen.insert(ty.describe()) {
                    return Ok(());
                }
                let params: HashMap<String, Type> = info
                    .type_params
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                for tag in &info.variants {
                    let Some(variant) = self.adt_registry.variants.get(tag) else {
                        continue;
                    };
                    for field in &variant.field_types {
                        let field_ty = instantiate_core_type_lenient(field, &params);
                        self.reduce_type(&field_ty, constraint, span, seen, out)?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Discharges every outstanding obligation and returns the errors.
    ///
    /// A requirement left on a bare variable is ambiguous but harmless: no
    /// function type can reach it, so it is dropped.
    pub(crate) fn finish(&mut self) -> Vec<Diagnostic> {
        let mut errors = std::mem::take(&mut self.deferred);
        for obligation in std::mem::take(&mut self.obligations) {
            if let Err(d) = self.reduce(&obligation, &mut HashSet::new(), &mut Vec::new()) {
                errors.push(d);
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{checker, span};
    use super::*;
    use crate::adt::AdtRegistry;
    use obfusku_core::ast::{TypeDecl, VariantDecl};
    use obfusku_core::types::Type as CoreType;

    fn with(constraint: Constraint, ty: Type) -> Result<(), Diagnostic> {
        let mut c = checker();
        c.require(ty, constraint, span());
        c.finish().into_iter().next().map_or(Ok(()), Err)
    }

    #[test]
    fn primitives_support_every_generated_operation() {
        for constraint in [Constraint::Eq, Constraint::Ord, Constraint::Show] {
            assert!(with(constraint, Type::Int).is_ok());
        }
    }

    #[test]
    fn a_function_type_supports_none_of_them() {
        let f = Type::function(Type::Int, Type::Int);
        let error = with(Constraint::Eq, f).expect_err("functions have no equality");
        assert!(error.message.contains("has no equality"));
    }

    #[test]
    fn a_function_nested_in_an_array_is_found() {
        let nested = Type::array(Type::function(Type::Int, Type::Int));
        assert!(with(Constraint::Show, nested).is_err());
    }

    #[test]
    fn cells_have_equality_but_no_order() {
        assert!(with(Constraint::Eq, Type::cell(Type::Int)).is_ok());
        assert!(with(Constraint::Ord, Type::cell(Type::Int)).is_err());
    }

    #[test]
    fn showing_a_cell_requires_its_contents_to_show() {
        let cell = Type::cell(Type::function(Type::Int, Type::Int));
        assert!(with(Constraint::Show, cell).is_err());
    }

    #[test]
    fn a_requirement_on_a_variable_is_dropped_when_nothing_pins_it() {
        let mut c = checker();
        let v = c.fresh();
        c.require(v, Constraint::Ord, span());
        assert!(c.finish().is_empty());
    }

    #[test]
    fn a_requirement_resolves_through_the_substitution() {
        let mut c = checker();
        let v = c.fresh();
        c.require(v.clone(), Constraint::Eq, span());
        c.unify(&v, &Type::function(Type::Int, Type::Int), span())
            .expect("binds");
        assert_eq!(c.finish().len(), 1);
    }

    #[test]
    fn a_recursive_type_terminates() {
        let list = TypeDecl {
            tag: "List".into(),
            type_params: vec!["t".into()],
            variants: vec![
                VariantDecl {
                    tag: "Nil".into(),
                    fields: vec![],
                    field_names: None,
                    span: span(),
                },
                VariantDecl {
                    tag: "Cons".into(),
                    fields: vec![
                        CoreType::Param("t".into()),
                        CoreType::Adt("List".into(), vec![CoreType::Param("t".into())]),
                    ],
                    field_names: None,
                    span: span(),
                },
            ],
            span: span(),
        };
        let mut c = super::super::Checker::new(AdtRegistry::build(&[list]).expect("builds"));
        let ty = Type::Adt("List".into(), vec![Type::Int]);
        c.require(ty, Constraint::Eq, span());
        assert!(c.finish().is_empty());
    }

    #[test]
    fn a_field_of_function_type_poisons_its_adt() {
        let holder = TypeDecl {
            tag: "Holder".into(),
            type_params: vec![],
            variants: vec![VariantDecl {
                tag: "Hold".into(),
                fields: vec![CoreType::function(CoreType::Int, CoreType::Int)],
                field_names: None,
                span: span(),
            }],
            span: span(),
        };
        let mut c = super::super::Checker::new(AdtRegistry::build(&[holder]).expect("builds"));
        c.require(Type::Adt("Holder".into(), vec![]), Constraint::Eq, span());
        assert_eq!(c.finish().len(), 1);
    }
}
