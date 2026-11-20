//! Match arms and patterns as written (`CONCRETE_SYMBOLIC_GRAMMAR.md` §10–§11).

use super::Expression;
use obfusku_diagnostics::Span;

/// `Pattern "→" Expression`.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// What the scrutinee must look like.
    pub pattern: Pattern,
    /// The arm's body.
    pub result: Expression,
    /// The whole arm.
    pub span: Span,
}

/// A pattern.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// A lowercase name: matches anything and binds it.
    Var(String, Span),
    /// `_`: matches anything, binds nothing.
    Wildcard(Span),
    /// An integer literal.
    Int(i64, Span),
    /// A real literal.
    Real(f64, Span),
    /// A string literal.
    Str(String, Span),
    /// `◉` or `◎`.
    Bool(bool, Span),
    /// `Tag "(" Pattern? ("," Pattern)* ")"`: positional fields.
    Constructor {
        /// The constructor name.
        tag: String,
        /// One sub-pattern per field, in order.
        args: Vec<Pattern>,
        /// The whole pattern.
        span: Span,
    },
    /// `Tag "{" name ":" Pattern … "}"`: a record pattern.
    ///
    /// Kept apart from [`Pattern::Constructor`] because the fields are in
    /// *source* order until the desugarer reorders them by the record's
    /// declaration, which it knows and the parser does not.
    NamedConstructor {
        /// The record's tag.
        tag: String,
        /// `(field name, pattern)` in the order written.
        fields: Vec<(String, Pattern)>,
        /// The whole pattern.
        span: Span,
    },
}

impl Pattern {
    /// The whole pattern.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Pattern::Var(_, s)
            | Pattern::Wildcard(s)
            | Pattern::Int(_, s)
            | Pattern::Real(_, s)
            | Pattern::Str(_, s)
            | Pattern::Bool(_, s)
            | Pattern::Constructor { span: s, .. }
            | Pattern::NamedConstructor { span: s, .. } => *s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::SourceId;

    #[test]
    fn every_pattern_reports_its_span() {
        let at = Span::new(SourceId(0), 3, 8);
        let patterns = [
            Pattern::Var("x".into(), at),
            Pattern::Wildcard(at),
            Pattern::Int(1, at),
            Pattern::Real(1.0, at),
            Pattern::Str(String::new(), at),
            Pattern::Bool(true, at),
            Pattern::Constructor {
                tag: "T".into(),
                args: vec![],
                span: at,
            },
            Pattern::NamedConstructor {
                tag: "T".into(),
                fields: vec![],
                span: at,
            },
        ];
        assert!(patterns.iter().all(|p| p.span() == at));
    }
}
