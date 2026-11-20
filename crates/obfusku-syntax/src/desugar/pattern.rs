//! Patterns: surface to Core, reordering record patterns.

use super::lowerer::{Lowered, Lowerer, collect_all};
use crate::ast::Pattern;
use obfusku_core::ast::{self as core, Literal};

impl Lowerer {
    /// A Core pattern. A record pattern (`P { y: b ⟢ x: a }`) becomes the
    /// ordinary positional constructor pattern in the record's declared order
    /// (`ABSTRACT_GRAMMAR.md` §4), so the checker and the evaluator never see
    /// record patterns at all.
    pub(super) fn lower_pattern(&self, p: &Pattern) -> Lowered<core::Pattern> {
        Ok(match p {
            Pattern::Var(name, span) => core::Pattern::Var(name.clone(), *span),
            Pattern::Wildcard(span) => core::Pattern::Wildcard(*span),
            Pattern::Int(v, span) => core::Pattern::Lit(Literal::Int(*v), *span),
            Pattern::Real(v, span) => core::Pattern::Lit(Literal::Real(*v), *span),
            Pattern::Str(v, span) => core::Pattern::Lit(Literal::Str(v.clone()), *span),
            Pattern::Bool(v, span) => core::Pattern::Lit(Literal::Bool(*v), *span),
            Pattern::Constructor { tag, args, span } => core::Pattern::Constructor {
                tag: tag.clone(),
                args: self.lower_patterns(args.iter())?,
                span: *span,
            },
            Pattern::NamedConstructor { tag, fields, span } => core::Pattern::Constructor {
                tag: tag.clone(),
                args: self
                    .lower_patterns(self.reorder_named_fields(tag, fields, *span)?.into_iter())?,
                span: *span,
            },
        })
    }

    fn lower_patterns<'a>(
        &self,
        patterns: impl Iterator<Item = &'a Pattern>,
    ) -> Lowered<Vec<core::Pattern>> {
        collect_all(patterns.map(|p| self.lower_pattern(p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desugar::desugar;
    use crate::lexer::tokenize;
    use crate::parser::parse;
    use obfusku_diagnostics::SourceId;

    fn lower(source: &str) -> Result<core::Module, Vec<obfusku_diagnostics::Diagnostic>> {
        let tokens = tokenize(source, SourceId(0)).unwrap();
        desugar(&parse(&tokens, SourceId(0)).unwrap())
    }

    fn first_arm_pattern(module: &core::Module) -> &core::Pattern {
        let binding = module.all_bindings().last().expect("a binding");
        match &binding.value {
            core::Expr::Match { arms, .. } => &arms[0].pattern,
            other => panic!("expected a match, got {other:?}"),
        }
    }

    #[test]
    fn literals_variables_and_wildcards_lower_directly() {
        let m = lower("v ≔ ⟡ 1 { 7 → 0 ⟢ _ → 1 }\n❧").unwrap();
        assert!(matches!(
            first_arm_pattern(&m),
            core::Pattern::Lit(Literal::Int(7), _)
        ));
    }

    #[test]
    fn a_record_pattern_becomes_a_positional_one_in_declared_order() {
        let m = lower("P ≔ { x: ⟁ ⟢ y: ⟁ }\nv ≔ ⟡ P { x: 1 ⟢ y: 2 } { P { y: b ⟢ x: a } → a }\n❧")
            .unwrap();
        let core::Pattern::Constructor { args, .. } = first_arm_pattern(&m) else {
            panic!("expected a constructor pattern");
        };
        assert!(matches!(&args[0], core::Pattern::Var(n, _) if n == "a"));
        assert!(matches!(&args[1], core::Pattern::Var(n, _) if n == "b"));
    }

    #[test]
    fn a_record_pattern_of_an_unknown_record_is_an_error() {
        let errs = lower("v ≔ ⟡ 1 { Q { x: a } → 0 }\n❧").unwrap_err();
        assert!(errs[0].message.contains("not a known record type"));
    }

    #[test]
    fn errors_in_sibling_sub_patterns_are_all_reported() {
        let errs = lower("v ≔ ⟡ 1 { Pair(Q { a: x }, R { b: y }) → 0 }\n❧").unwrap_err();
        assert_eq!(errs.len(), 2);
    }
}
