//! Patterns and match arms (`SEMANTIC_CORE.md` §10).

use super::expr::Expr;
use super::literal::Literal;
use obfusku_diagnostics::Span;
use std::rc::Rc;

/// One arm of a [`Expr::Match`]: if the scrutinee fits `pattern`, evaluate
/// `result` with the pattern's binders in scope.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// What the scrutinee must look like.
    pub pattern: Pattern,
    /// The arm's body. Shared (`Rc`) because the evaluator keeps it alive in
    /// closures without cloning the tree.
    pub result: Rc<Expr>,
    /// The whole arm, for unreachable-arm warnings (`§17`).
    pub span: Span,
}

/// A pattern.
///
/// Deliberately a **separate grammar** from [`Expr`]: patterns and
/// expressions share a constructor-tag namespace but are not one grammar
/// with patterns as an expression subtype (§10). Keeping them apart means a
/// pattern can never contain a call or a lambda, so matching is total and
/// exhaustiveness (§17) is decidable.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Matches anything and binds it to the name.
    Var(String, Span),
    /// Matches anything and binds nothing.
    Wildcard(Span),
    /// Matches one literal by equality.
    Lit(Literal, Span),
    /// Matches a constructor and, recursively, each of its fields.
    Constructor {
        /// The constructor's tag (`SEMANTIC_CORE.md` §9).
        tag: String,
        /// One sub-pattern per field, in declaration order.
        args: Vec<Pattern>,
        /// The whole pattern.
        span: Span,
    },
}

impl Pattern {
    /// The source span of the whole pattern.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Pattern::Var(_, s) | Pattern::Wildcard(s) | Pattern::Lit(_, s) => *s,
            Pattern::Constructor { span: s, .. } => *s,
        }
    }

    /// The direct sub-patterns: a constructor's fields, otherwise none.
    #[must_use]
    pub fn children(&self) -> &[Pattern] {
        match self {
            Pattern::Constructor { args, .. } => args,
            Pattern::Var(..) | Pattern::Wildcard(_) | Pattern::Lit(..) => &[],
        }
    }

    /// Every name this pattern binds, left to right, with repeats kept.
    ///
    /// The order matters: the evaluator assigns environment slots in this
    /// order, so the checker and the runtime must agree on it. Duplicates are
    /// reported by the caller, not dropped here, since a repeated binder is a
    /// source error the checker wants to see.
    #[must_use]
    pub fn binders(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_binders(&mut out);
        out
    }

    fn collect_binders<'a>(&'a self, out: &mut Vec<&'a str>) {
        if let Pattern::Var(name, _) = self {
            out.push(name);
        }
        for child in self.children() {
            child.collect_binders(out);
        }
    }

    /// Whether the pattern matches every value of any type: a variable or a
    /// wildcard. A constructor pattern never counts, even when all its fields
    /// do, because a sum type may have sibling variants; exhaustiveness (§17)
    /// reasons about those.
    #[must_use]
    pub fn is_irrefutable(&self) -> bool {
        matches!(self, Pattern::Var(..) | Pattern::Wildcard(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(n: &str) -> Pattern {
        Pattern::Var(n.to_string(), Span::default())
    }

    fn ctor(tag: &str, args: Vec<Pattern>) -> Pattern {
        Pattern::Constructor {
            tag: tag.to_string(),
            args,
            span: Span::default(),
        }
    }

    #[test]
    fn binders_are_collected_left_to_right_through_nesting() {
        let p = ctor(
            "Pair",
            vec![var("a"), ctor("Some", vec![var("b")]), var("c")],
        );
        assert_eq!(p.binders(), ["a", "b", "c"]);
    }

    #[test]
    fn wildcards_and_literals_bind_nothing() {
        assert!(Pattern::Wildcard(Span::default()).binders().is_empty());
        assert!(
            Pattern::Lit(Literal::Int(1), Span::default())
                .binders()
                .is_empty()
        );
    }

    #[test]
    fn a_repeated_binder_is_kept_for_the_checker_to_report() {
        assert_eq!(ctor("P", vec![var("x"), var("x")]).binders(), ["x", "x"]);
    }

    #[test]
    fn children_are_a_constructors_fields_only() {
        assert_eq!(ctor("P", vec![var("a"), var("b")]).children().len(), 2);
        assert!(var("a").children().is_empty());
    }

    #[test]
    fn only_variables_and_wildcards_are_irrefutable() {
        assert!(var("a").is_irrefutable());
        assert!(Pattern::Wildcard(Span::default()).is_irrefutable());
        assert!(!ctor("Unit", vec![]).is_irrefutable());
        assert!(!Pattern::Lit(Literal::Unit, Span::default()).is_irrefutable());
    }

    #[test]
    fn span_is_the_whole_patterns() {
        let s = Span {
            source: obfusku_diagnostics::SourceId(0),
            start: 3,
            end: 9,
        };
        assert_eq!(ctor_at("P", s).span(), s);
    }

    fn ctor_at(tag: &str, span: Span) -> Pattern {
        Pattern::Constructor {
            tag: tag.to_string(),
            args: vec![],
            span,
        }
    }
}
