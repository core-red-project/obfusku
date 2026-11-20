//! Pipelines (`CONCRETE_SYMBOLIC_GRAMMAR.md` §12).

use super::{PResult, Parser};
use crate::ast::{Argument, Expression, Stage};
use crate::lexer::TokenKind;

impl Parser<'_> {
    /// `subject (▷ stage)*`, the loosest expression form, left-associative.
    pub(super) fn parse_pipe(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_or()?;
        while self.eat(&TokenKind::Pipe) {
            self.link()?;
            let stage = self.parse_stage()?;
            let span = expr.span().merge(stage.span());
            expr = Expression::Pipe {
                subject: Box::new(expr),
                stage: Box::new(stage),
                span,
            };
        }
        Ok(expr)
    }

    /// A bare callable, or a parenthesized expression (which may use `◈`).
    ///
    /// The classification is purely syntactic: parentheses always mean an
    /// expression stage, whether or not `◈` actually occurs inside, so what
    /// `x ▷ (f)` means never depends on the contents of `f`.
    fn parse_stage(&mut self) -> PResult<Stage> {
        if self.eat(&TokenKind::LParen) {
            let inner = self.parse_expression()?;
            self.expect(&TokenKind::RParen, "')'")?;
            if pipe_ref_and_hole_conflict(&inner) {
                return Err(self.error(
                    "'◈' and '•' cannot appear in the same pipeline stage — presence of '◈' \
                     forces expression-stage interpretation, and '•' has no meaning there \
                     (CONCRETE_SYMBOLIC_GRAMMAR.md §12.1/§16.1)",
                ));
            }
            Ok(Stage::Expr(inner))
        } else {
            Ok(Stage::Callable(self.parse_application()?))
        }
    }
}

/// The reserved-invalid combination §12.1/§16.1 names: `◈` and `•` in the
/// same stage.
///
/// A nested pipe's *stage* is a separate binding region (§12's scoping
/// rule), so the walk does not enter one; only its subject is still
/// lexically part of the current stage.
fn pipe_ref_and_hole_conflict(stage: &Expression) -> bool {
    let (mut has_ref, mut has_hole) = (false, false);
    scan(stage, &mut has_ref, &mut has_hole);
    has_ref && has_hole
}

fn scan(expr: &Expression, has_ref: &mut bool, has_hole: &mut bool) {
    match expr {
        Expression::PipeRef(_) => *has_ref = true,
        Expression::Application { args, .. } => {
            *has_hole |= args.iter().any(|a| matches!(a, Argument::Hole(_)));
        }
        Expression::Pipe { subject, .. } => return scan(subject, has_ref, has_hole),
        _ => {}
    }
    for child in expr.children() {
        scan(child, has_ref, has_hole);
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Expression, Stage};
    use crate::parser::testing::{error, expression};

    fn stage_of(source: &str) -> Stage {
        match expression(source) {
            Expression::Pipe { stage, .. } => *stage,
            other => panic!("expected a pipe, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_stage_is_a_callable() {
        assert!(matches!(stage_of("x ▷ f"), Stage::Callable(_)));
    }

    #[test]
    fn a_parenthesized_stage_is_an_expression_even_without_a_pipe_ref() {
        assert!(matches!(stage_of("x ▷ (f)"), Stage::Expr(_)));
        assert!(matches!(stage_of("x ▷ (g(◈))"), Stage::Expr(_)));
    }

    #[test]
    fn a_callable_stage_may_be_a_call() {
        assert!(matches!(
            stage_of("x ▷ f(1)"),
            Stage::Callable(Expression::Application { .. })
        ));
    }

    #[test]
    fn pipes_group_to_the_left() {
        let Expression::Pipe { subject, .. } = expression("a ▷ f ▷ g") else {
            panic!("expected a pipe");
        };
        assert!(matches!(*subject, Expression::Pipe { .. }));
    }

    #[test]
    fn a_pipe_is_looser_than_every_operator() {
        let Expression::Pipe { subject, .. } = expression("1 ✚ 2 ▷ f") else {
            panic!("expected a pipe");
        };
        assert!(matches!(*subject, Expression::BinaryOp { .. }));
    }

    #[test]
    fn a_hole_and_a_pipe_ref_in_one_stage_conflict() {
        let msg = error("v ≔ x ▷ (f(•, ◈))\n❧");
        assert!(msg.contains("cannot appear in the same pipeline stage"));
    }

    #[test]
    fn a_hole_alone_in_a_stage_is_not_a_conflict_at_parse_time() {
        assert!(matches!(stage_of("x ▷ (f(•))"), Stage::Expr(_)));
    }

    #[test]
    fn a_nested_pipes_own_stage_is_a_separate_region() {
        // `◈` is in the inner stage `(g(◈))`; `•` is in the outer one.
        assert!(matches!(
            stage_of("x ▷ (y ▷ (g(◈)) ▷ f(•))"),
            Stage::Expr(_)
        ));
    }

    #[test]
    fn the_conflict_is_found_inside_lambdas_and_matches() {
        assert!(error("v ≔ x ▷ (⟡ ◈ { _ → f(•) })\n❧").contains("cannot appear"));
    }
}
