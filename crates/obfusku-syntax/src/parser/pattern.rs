//! Match patterns (`CONCRETE_SYMBOLIC_GRAMMAR.md` §10).

use super::{PResult, Parser, starts_uppercase};
use crate::ast::Pattern;
use crate::lexer::TokenKind;
use obfusku_diagnostics::Span;

impl Parser<'_> {
    pub(super) fn parse_pattern(&mut self) -> PResult<Pattern> {
        self.nested(Self::parse_pattern_unguarded)
    }

    fn parse_pattern_unguarded(&mut self) -> PResult<Pattern> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Wildcard => {
                self.advance();
                Ok(Pattern::Wildcard(span))
            }
            // `2^63` lexes as `i64::MIN`, valid only behind a unary minus,
            // and a pattern has no unary minus.
            TokenKind::Int(i64::MIN) => {
                Err(self.error("9223372036854775808 is out of range as a literal"))
            }
            TokenKind::Int(v) => {
                self.advance();
                Ok(Pattern::Int(v, span))
            }
            TokenKind::Real(v) => {
                self.advance();
                Ok(Pattern::Real(v, span))
            }
            TokenKind::Str(v) => {
                self.advance();
                Ok(Pattern::Str(v, span))
            }
            TokenKind::Bool(v) => {
                self.advance();
                Ok(Pattern::Bool(v, span))
            }
            TokenKind::Ident(name) if starts_uppercase(&name) => {
                self.advance();
                if matches!(self.peek(), TokenKind::LBrace) {
                    return self.parse_named_constructor_pattern(name, span);
                }
                let mut args = Vec::new();
                let mut end = span;
                if self.eat(&TokenKind::LParen) {
                    if !matches!(self.peek(), TokenKind::RParen) {
                        args = self.comma_list(Self::parse_pattern)?;
                    }
                    end = self.expect(&TokenKind::RParen, "')'")?;
                }
                Ok(Pattern::Constructor {
                    tag: name,
                    args,
                    span: span.merge(end),
                })
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Pattern::Var(name, span))
            }
            other => Err(self.error(format!("expected a pattern, found {other:?}"))),
        }
    }

    /// `PNamedConstructor ::= Tag "{" name ":" Pattern … "}"`; the `{` is
    /// peeked, not yet consumed.
    fn parse_named_constructor_pattern(&mut self, tag: String, tag_span: Span) -> PResult<Pattern> {
        self.advance(); // consume '{'
        let fields = self.named_fields(Self::parse_pattern)?;
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Pattern::NamedConstructor {
            tag,
            fields,
            span: tag_span.merge(end),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Expression, Pattern};
    use crate::parser::testing::{error, expression};

    fn first_pattern(pattern: &str) -> Pattern {
        match expression(&format!("⟡ 0 {{ {pattern} → 1 }}")) {
            Expression::Match { mut arms, .. } => arms.remove(0).pattern,
            other => panic!("expected a match, got {other:?}"),
        }
    }

    #[test]
    fn literals_variables_and_wildcards() {
        assert!(matches!(first_pattern("_"), Pattern::Wildcard(_)));
        assert!(matches!(first_pattern("x"), Pattern::Var(n, _) if n == "x"));
        assert!(matches!(first_pattern("7"), Pattern::Int(7, _)));
        assert!(matches!(first_pattern("1.5"), Pattern::Real(r, _) if r == 1.5));
        assert!(matches!(first_pattern("\"s\""), Pattern::Str(s, _) if s == "s"));
        assert!(matches!(first_pattern("◉"), Pattern::Bool(true, _)));
    }

    #[test]
    fn a_bare_tag_is_a_nullary_constructor() {
        assert!(
            matches!(first_pattern("Nil"), Pattern::Constructor { args, .. } if args.is_empty())
        );
    }

    #[test]
    fn a_constructor_pattern_lists_its_arguments() {
        let Pattern::Constructor { tag, args, .. } = first_pattern("Pair(a, Some(b))") else {
            panic!("expected a constructor");
        };
        assert_eq!(tag, "Pair");
        assert!(matches!(&args[1], Pattern::Constructor { tag, .. } if tag == "Some"));
    }

    #[test]
    fn empty_parentheses_after_a_tag_are_allowed() {
        assert!(
            matches!(first_pattern("Unit()"), Pattern::Constructor { args, .. } if args.is_empty())
        );
    }

    #[test]
    fn a_record_pattern_keeps_source_field_order() {
        let Pattern::NamedConstructor { fields, .. } = first_pattern("P { y: b ⟢ x: a }") else {
            panic!("expected a named constructor");
        };
        let names: Vec<_> = fields.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["y", "x"]);
    }

    #[test]
    fn the_magnitude_of_the_minimum_integer_is_not_a_pattern() {
        assert!(error("v ≔ ⟡ 0 { 9223372036854775808 → 1 }\n❧").contains("out of range"));
    }

    #[test]
    fn a_missing_pattern_is_reported() {
        assert!(error("v ≔ ⟡ 0 { → 1 }\n❧").contains("expected a pattern"));
    }
}
