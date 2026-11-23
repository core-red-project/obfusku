//! Match exhaustiveness (`SEMANTIC_CORE.md` §17).
//!
//! A match is exhaustive when some arm is irrefutable, or when the arms
//! cover every inhabitant of a closed scrutinee type. The check is
//! structural: it recurses into constructor fields, so a hole in a nested
//! position (`Some(Err(_))` missing) is found, not just one at the top.

use super::Checker;
use super::annotation::instantiate_core_type;
use crate::types::Type;
use obfusku_core::ast::{Literal, Pattern};
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

impl Checker {
    /// Closed types (an ADT, `Bool`) are exhaustive when every variant or
    /// value is covered; open ones (`Int`, `Real`, `Str`, an extensible
    /// `Exception`) always need an irrefutable arm.
    ///
    /// A function, cell, unit or still-unresolved scrutinee is accepted:
    /// there is nothing the arms could meaningfully enumerate.
    pub(super) fn check_exhaustive(
        &self,
        scrutinee_ty: &Type,
        patterns: &[&Pattern],
        span: Span,
    ) -> Result<(), Diagnostic> {
        if patterns.iter().any(|p| p.is_irrefutable()) {
            return Ok(());
        }
        match self.resolve(scrutinee_ty) {
            Type::Adt(name, args) => self.check_exhaustive_adt(&name, &args, patterns, span),
            Type::Int | Type::Real | Type::Str => Err(Diagnostic::error(
                "non-exhaustive match: this type has infinitely many values; a wildcard or \
                 variable arm is required",
                span,
            )),
            Type::Bool => {
                let covered: HashSet<bool> = patterns
                    .iter()
                    .filter_map(|p| match p {
                        Pattern::Lit(Literal::Bool(b), _) => Some(*b),
                        _ => None,
                    })
                    .collect();
                if covered.len() == 2 {
                    Ok(())
                } else {
                    Err(Diagnostic::error(
                        "non-exhaustive match: both 'true' and 'false' must be covered, or a \
                         wildcard/variable arm added",
                        span,
                    ))
                }
            }
            Type::Function(..) | Type::Cell(_) | Type::Unit | Type::Var(_) => Ok(()),
        }
    }

    /// `type_args` are the scrutinee's actual type arguments at this
    /// position (`[Int]` for `Option<Int>`), needed to instantiate each
    /// field's declared `Param` before recursing into it.
    fn check_exhaustive_adt(
        &self,
        adt_name: &str,
        type_args: &[Type],
        patterns: &[&Pattern],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(adt_info) = self.adt_registry.adts.get(adt_name) else {
            return Ok(());
        };
        if adt_info.open {
            return Err(Diagnostic::error(
                format!(
                    "non-exhaustive match: '{adt_name}' is an open type (modules can add \
                     variants), so a wildcard or variable arm is required"
                ),
                span,
            ));
        }
        let covered: HashSet<&str> = patterns
            .iter()
            .filter_map(|p| match p {
                Pattern::Constructor { tag, .. } => Some(tag.as_str()),
                _ => None,
            })
            .collect();
        let missing: Vec<&str> = adt_info
            .variants
            .iter()
            .map(String::as_str)
            .filter(|tag| !covered.contains(tag))
            .collect();
        if !missing.is_empty() {
            return Err(Diagnostic::error(
                format!(
                    "non-exhaustive match: missing case(s) for {}",
                    missing.join(", ")
                ),
                span,
            ));
        }
        for tag in &adt_info.variants {
            self.check_fields_exhaustive(tag, type_args, patterns, span)?;
        }
        Ok(())
    }

    /// Every field of `tag` must itself be matched exhaustively by the
    /// sub-patterns the arms for `tag` put in that position.
    fn check_fields_exhaustive(
        &self,
        tag: &str,
        type_args: &[Type],
        patterns: &[&Pattern],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let variant = &self.adt_registry.variants[tag];
        let params: HashMap<String, Type> = variant
            .type_params
            .iter()
            .cloned()
            .zip(type_args.iter().cloned())
            .collect();
        for (index, field_ty) in variant.field_types.iter().enumerate() {
            let subpatterns: Vec<&Pattern> = patterns
                .iter()
                .filter_map(|p| match p {
                    Pattern::Constructor { tag: t, args, .. } if t == tag => Some(&args[index]),
                    _ => None,
                })
                .collect();
            if subpatterns.is_empty() || subpatterns.iter().any(|p| p.is_irrefutable()) {
                continue;
            }
            match self.resolve(&instantiate_core_type(field_ty, &params)) {
                Type::Adt(name, args) => {
                    self.check_exhaustive_adt(&name, &args, &subpatterns, span)?;
                }
                _ => {
                    return Err(Diagnostic::error(
                        format!(
                            "non-exhaustive match: field {index} of '{tag}' requires a \
                             wildcard or variable pattern"
                        ),
                        span,
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{checker, span};
    use super::*;
    use crate::adt::AdtRegistry;
    use obfusku_core::ast::{TypeDecl, VariantDecl};
    use obfusku_core::types::Type as CoreType;

    fn ctor(tag: &str, args: Vec<Pattern>) -> Pattern {
        Pattern::Constructor {
            tag: tag.to_string(),
            args,
            span: span(),
        }
    }

    fn wildcard() -> Pattern {
        Pattern::Wildcard(span())
    }

    fn boolean(value: bool) -> Pattern {
        Pattern::Lit(Literal::Bool(value), span())
    }

    fn variant(tag: &str, fields: Vec<CoreType>) -> VariantDecl {
        VariantDecl {
            tag: tag.to_string(),
            fields,
            field_names: None,
            span: span(),
        }
    }

    /// `Result<t, e>` plus `Option<t>`: enough to nest one inside the other.
    fn nested_checker() -> Checker {
        let param = |n: &str| CoreType::Param(n.to_string());
        let result = TypeDecl {
            tag: "Result".into(),
            type_params: vec!["t".into(), "e".into()],
            variants: vec![
                variant("Ok", vec![param("t")]),
                variant("Err", vec![param("e")]),
            ],
            span: span(),
        };
        let option = TypeDecl {
            tag: "Option".into(),
            type_params: vec!["t".into()],
            variants: vec![variant("Some", vec![param("t")]), variant("None", vec![])],
            span: span(),
        };
        Checker::new(AdtRegistry::build(&[result, option]).expect("valid"))
    }

    fn check(c: &Checker, scrutinee: &Type, patterns: &[Pattern]) -> Result<(), Diagnostic> {
        let refs: Vec<&Pattern> = patterns.iter().collect();
        c.check_exhaustive(scrutinee, &refs, span())
    }

    fn option_of(inner: Type) -> Type {
        Type::Adt("Option".into(), vec![inner])
    }

    #[test]
    fn a_wildcard_arm_makes_any_match_exhaustive() {
        let c = checker();
        assert!(check(&c, &Type::Int, &[wildcard()]).is_ok());
    }

    #[test]
    fn an_open_primitive_type_needs_a_catch_all() {
        let c = checker();
        let error = check(&c, &Type::Int, &[]).expect_err("infinite type");
        assert!(error.message.contains("infinitely many values"));
    }

    #[test]
    fn bool_needs_both_literals() {
        let c = checker();
        assert!(check(&c, &Type::Bool, &[boolean(true)]).is_err());
        assert!(check(&c, &Type::Bool, &[boolean(true), boolean(false)]).is_ok());
    }

    #[test]
    fn missing_variants_are_named_in_declaration_order() {
        let c = nested_checker();
        let ty = Type::Adt("Result".into(), vec![Type::Int, Type::Str]);
        let error = check(&c, &ty, &[ctor("Ok", vec![wildcard()])]).expect_err("missing Err");
        assert_eq!(
            error.message,
            "non-exhaustive match: missing case(s) for Err"
        );
    }

    #[test]
    fn covering_every_variant_is_exhaustive() {
        let c = nested_checker();
        let ty = option_of(Type::Int);
        let arms = [ctor("Some", vec![wildcard()]), ctor("None", vec![])];
        assert!(check(&c, &ty, &arms).is_ok());
    }

    #[test]
    fn a_hole_in_a_nested_position_is_found() {
        let c = nested_checker();
        let ty = option_of(Type::Adt("Result".into(), vec![Type::Int, Type::Int]));
        let arms = [
            ctor("Some", vec![ctor("Ok", vec![wildcard()])]),
            ctor("None", vec![]),
        ];
        let error = check(&c, &ty, &arms).expect_err("Some(Err(_)) is missing");
        assert!(error.message.contains("missing case(s) for Err"));
    }

    #[test]
    fn a_fully_covered_nested_match_is_exhaustive() {
        let c = nested_checker();
        let ty = option_of(Type::Adt("Result".into(), vec![Type::Int, Type::Int]));
        let arms = [
            ctor("Some", vec![ctor("Ok", vec![wildcard()])]),
            ctor("Some", vec![ctor("Err", vec![wildcard()])]),
            ctor("None", vec![]),
        ];
        assert!(check(&c, &ty, &arms).is_ok());
    }

    #[test]
    fn a_literal_in_a_field_of_an_open_type_demands_a_catch_all_there() {
        let c = nested_checker();
        let ty = option_of(Type::Int);
        let lit = Pattern::Lit(Literal::Int(1), span());
        let arms = [ctor("Some", vec![lit]), ctor("None", vec![])];
        let error = check(&c, &ty, &arms).expect_err("field 0 is Int");
        assert!(error.message.contains("field 0 of 'Some'"));
    }

    #[test]
    fn exception_is_open_so_every_variant_still_needs_a_catch_all() {
        let c = checker();
        let arms = [ctor("DivisionByZero", vec![])];
        let error = check(&c, &Type::exception(), &arms).expect_err("open type");
        assert!(error.message.contains("is an open type"));
    }

    #[test]
    fn types_with_nothing_to_enumerate_are_accepted() {
        let c = checker();
        assert!(check(&c, &Type::Unit, &[]).is_ok());
        assert!(check(&c, &Type::function(Type::Int, Type::Int), &[]).is_ok());
    }
}
