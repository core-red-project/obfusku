//! Typing of `Match` and of the patterns in its arms (`SEMANTIC_CORE.md`
//! §10, §17).

use super::{Checker, Infer, TypeEnv, reject};
use crate::types::{Scheme, Type};
use obfusku_core::ast::{Expr, MatchArm, Pattern};
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashSet;

/// What the arms seen so far already cover, to flag an arm that can never
/// run. Unreachable arms are a warning, not an error: the program is still
/// well typed, just carrying dead code.
#[derive(Default)]
struct Coverage<'a> {
    /// Constructors fully matched by an earlier arm.
    covered: HashSet<&'a str>,
    /// A wildcard or variable arm was seen: every later arm is dead.
    catch_all: bool,
}

impl<'a> Coverage<'a> {
    /// Records `pattern` and returns the warning if it can never match.
    fn visit(&mut self, pattern: &'a Pattern) -> Option<String> {
        if self.catch_all {
            return Some(
                "unreachable match arm: a previous wildcard/variable arm already covers every \
                 remaining case"
                    .to_string(),
            );
        }
        match pattern {
            Pattern::Constructor { tag, args, .. } => {
                let already = self.covered.contains(tag.as_str());
                // Only an arm matching *every* value of its constructor
                // shadows a later arm with that tag; a refutable one such as
                // `Cons(A(n), t)` leaves cases to the next.
                if args.iter().all(Pattern::is_irrefutable) {
                    self.covered.insert(tag);
                }
                already.then(|| {
                    format!("unreachable match arm: '{tag}' is already covered by an earlier arm")
                })
            }
            Pattern::Var(..) | Pattern::Wildcard(_) => {
                self.catch_all = true;
                None
            }
            Pattern::Lit(..) => None,
        }
    }
}

impl Checker {
    /// Every arm's pattern is checked against the scrutinee and every
    /// result must unify with the first arm's.
    pub(super) fn infer_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let scrutinee_ty = self.infer(scrutinee, env)?;
        let mut coverage = Coverage::default();
        let mut result_ty: Option<Type> = None;

        for arm in arms {
            if let Some(message) = coverage.visit(&arm.pattern) {
                self.warnings.push(Diagnostic::warning(message, arm.span));
            }
            let mut arm_env = env.clone();
            self.check_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env)?;
            let arm_ty = self.infer(&arm.result, &arm_env)?;
            match &result_ty {
                None => result_ty = Some(arm_ty),
                Some(expected) => {
                    self.expect_same_or(expected, &arm_ty, arm.span, |c| {
                        format!(
                            "match arms have incompatible types: expected {}, found {}",
                            c.show(expected),
                            c.show(&arm_ty)
                        )
                    })?;
                }
            }
        }

        let Some(result_ty) = result_ty else {
            return reject(span, "a match needs at least one arm");
        };
        let patterns: Vec<&Pattern> = arms.iter().map(|a| &a.pattern).collect();
        self.check_exhaustive(&scrutinee_ty, &patterns, span)
            .map_err(|d| vec![d])?;
        Ok(result_ty)
    }

    /// Checks `pattern` against `expected_ty` and binds its variables, each
    /// monomorphic at the type its position has.
    pub(super) fn check_pattern(
        &mut self,
        pattern: &Pattern,
        expected_ty: &Type,
        env: &mut TypeEnv,
    ) -> Infer<()> {
        match pattern {
            Pattern::Var(name, _) => {
                env.insert(name.clone(), Scheme::monomorphic(expected_ty.clone()));
                Ok(())
            }
            Pattern::Wildcard(_) => Ok(()),
            Pattern::Lit(literal, span) => {
                let literal_ty = Type::of_literal(literal);
                self.expect_same_or(expected_ty, &literal_ty, *span, |c| {
                    format!(
                        "pattern type mismatch: expected {}, found {}",
                        c.show(expected_ty),
                        literal_ty.describe()
                    )
                })
            }
            Pattern::Constructor { tag, args, span } => {
                let Some(variant) = self.instantiate_variant(tag) else {
                    return reject(*span, format!("unknown constructor '{tag}' in pattern"));
                };
                if args.len() != variant.info.field_types.len() {
                    return reject(
                        *span,
                        format!(
                            "pattern for '{tag}' expects {} argument(s), found {}",
                            variant.info.field_types.len(),
                            args.len()
                        ),
                    );
                }
                self.expect_same_or(expected_ty, &variant.adt_ty, *span, |c| {
                    format!(
                        "pattern for '{tag}' does not match scrutinee type {}",
                        c.show(expected_ty)
                    )
                })?;
                for (index, arg) in args.iter().enumerate() {
                    self.check_pattern(arg, &variant.field_type(index), env)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{boolean, checker, int, span, string, var};
    use super::*;
    use crate::adt::AdtRegistry;
    use obfusku_core::ast::{Literal, TypeDecl, VariantDecl};
    use obfusku_core::types::Type as CoreType;
    use std::rc::Rc;

    fn arm(pattern: Pattern, result: Expr) -> MatchArm {
        MatchArm {
            pattern,
            result: Rc::new(result),
            span: span(),
        }
    }

    fn lit(value: i64) -> Pattern {
        Pattern::Lit(Literal::Int(value), span())
    }

    fn wildcard() -> Pattern {
        Pattern::Wildcard(span())
    }

    fn matching(scrutinee: Expr, arms: Vec<MatchArm>) -> Expr {
        Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: span(),
        }
    }

    fn option_checker() -> Checker {
        let option = TypeDecl {
            tag: "Option".into(),
            type_params: vec!["t".into()],
            variants: vec![
                VariantDecl {
                    tag: "Some".into(),
                    fields: vec![CoreType::Param("t".into())],
                    field_names: None,
                    span: span(),
                },
                VariantDecl {
                    tag: "None".into(),
                    fields: vec![],
                    field_names: None,
                    span: span(),
                },
            ],
            span: span(),
        };
        Checker::new(AdtRegistry::build(&[option]).expect("valid"))
    }

    fn some(args: Vec<Pattern>) -> Pattern {
        Pattern::Constructor {
            tag: "Some".into(),
            args,
            span: span(),
        }
    }

    #[test]
    fn every_arm_result_must_agree() {
        let mut c = checker();
        let good = matching(
            int(1),
            vec![arm(lit(1), string("a")), arm(wildcard(), string("b"))],
        );
        assert!(c.infer(&good, &TypeEnv::new()).is_ok());
        let bad = matching(
            int(1),
            vec![arm(lit(1), string("a")), arm(wildcard(), boolean(true))],
        );
        let errors = c.infer(&bad, &TypeEnv::new()).expect_err("incompatible");
        assert_eq!(
            errors[0].message,
            "match arms have incompatible types: expected String, found Bool"
        );
    }

    #[test]
    fn a_variable_pattern_binds_at_the_scrutinee_type() {
        let mut c = checker();
        let m = matching(int(1), vec![arm(Pattern::Var("n".into(), span()), var("n"))]);
        let ty = c.infer(&m, &TypeEnv::new()).expect("infers");
        assert_eq!(c.resolve(&ty), Type::Int);
    }

    #[test]
    fn a_literal_pattern_of_the_wrong_type_is_rejected() {
        let mut c = checker();
        let m = matching(string("s"), vec![arm(lit(1), int(0)), arm(wildcard(), int(1))]);
        let errors = c.infer(&m, &TypeEnv::new()).expect_err("mismatch");
        assert_eq!(
            errors[0].message,
            "pattern type mismatch: expected String, found Int"
        );
    }

    #[test]
    fn constructor_patterns_bind_their_fields_at_the_instantiated_type() {
        let mut c = option_checker();
        let scrutinee = Expr::Constructor {
            tag: "Some".into(),
            args: vec![int(1)],
            span: span(),
        };
        let m = matching(
            scrutinee,
            vec![
                arm(some(vec![Pattern::Var("n".into(), span())]), var("n")),
                arm(
                    Pattern::Constructor {
                        tag: "None".into(),
                        args: vec![],
                        span: span(),
                    },
                    int(0),
                ),
            ],
        );
        let ty = c.infer(&m, &TypeEnv::new()).expect("infers");
        assert_eq!(c.resolve(&ty), Type::Int);
    }

    #[test]
    fn a_constructor_pattern_with_the_wrong_arity_says_how_many() {
        let mut c = option_checker();
        let m = matching(int(1), vec![arm(some(vec![]), int(0)), arm(wildcard(), int(1))]);
        let errors = c.infer(&m, &TypeEnv::new()).expect_err("arity");
        assert_eq!(
            errors[0].message,
            "pattern for 'Some' expects 1 argument(s), found 0"
        );
    }

    #[test]
    fn an_unknown_constructor_in_a_pattern_is_named() {
        let mut c = checker();
        let ghost = Pattern::Constructor {
            tag: "Ghost".into(),
            args: vec![],
            span: span(),
        };
        let m = matching(int(1), vec![arm(ghost, int(0))]);
        let errors = c.infer(&m, &TypeEnv::new()).expect_err("unknown");
        assert_eq!(errors[0].message, "unknown constructor 'Ghost' in pattern");
    }

    #[test]
    fn an_arm_after_a_catch_all_is_flagged_unreachable() {
        let mut c = checker();
        let m = matching(int(1), vec![arm(wildcard(), int(0)), arm(lit(2), int(1))]);
        c.infer(&m, &TypeEnv::new()).expect("still well typed");
        assert_eq!(c.warnings.len(), 1);
        assert!(c.warnings[0].message.contains("wildcard/variable arm"));
    }

    #[test]
    fn a_repeated_irrefutable_constructor_arm_is_flagged() {
        let mut c = option_checker();
        let any = || some(vec![wildcard()]);
        let m = matching(
            Expr::Constructor {
                tag: "None".into(),
                args: vec![],
                span: span(),
            },
            vec![arm(any(), int(0)), arm(any(), int(1)), arm(wildcard(), int(2))],
        );
        c.infer(&m, &TypeEnv::new()).expect("still well typed");
        assert_eq!(c.warnings.len(), 1);
        assert!(c.warnings[0].message.contains("'Some' is already covered"));
    }

    #[test]
    fn a_refutable_constructor_arm_does_not_shadow_the_next() {
        let mut c = option_checker();
        let narrow = some(vec![lit(1)]);
        let m = matching(
            Expr::Constructor {
                tag: "None".into(),
                args: vec![],
                span: span(),
            },
            vec![arm(narrow, int(0)), arm(some(vec![wildcard()]), int(1)), arm(wildcard(), int(2))],
        );
        c.infer(&m, &TypeEnv::new()).expect("well typed");
        assert!(c.warnings.is_empty());
    }

    #[test]
    fn a_match_with_no_arms_is_an_error_not_a_panic() {
        let mut c = checker();
        let m = matching(Expr::Var("x".into(), span()), vec![]);
        let mut env = TypeEnv::new();
        let v = c.fresh();
        env.insert("x".into(), Scheme::monomorphic(v));
        let errors = c.infer(&m, &env).expect_err("no arms");
        assert_eq!(errors[0].message, "a match needs at least one arm");
    }
}
