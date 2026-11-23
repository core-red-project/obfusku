//! Expressions: atoms, calls, lambdas, matches, locals and records
//! (`CONCRETE_SYMBOLIC_GRAMMAR.md` §7, §11).
//!
//! Operators live in `operator`, pipelines in `pipe`; everything here is
//! reached from [`Parser::parse_pipe`] down through [`Parser::parse_primary`].

use super::{PResult, Parser, starts_uppercase};
use crate::ast::{Argument, Expression, MatchArm, Parameter, ValueDeclaration};
use crate::lexer::TokenKind;
use obfusku_diagnostics::Span;

impl Parser<'_> {
    /// `Expression`, with `LocalBinding` as one alternative.
    pub(super) fn parse_expression(&mut self) -> PResult<Expression> {
        self.nested(Self::parse_expression_unguarded)
    }

    /// One token of lookahead decides whether a local binding starts here
    /// before falling through to the operator ladder: `name ≔` is a local
    /// value, and `λ name` (as opposed to `λ(`) is a local function. The two
    /// lambda forms differ only in whether a name follows the `λ`.
    fn parse_expression_unguarded(&mut self) -> PResult<Expression> {
        if let TokenKind::Ident(name) = self.peek().clone()
            && !starts_uppercase(&name)
        {
            if matches!(self.peek_at(1), TokenKind::Bind) {
                return self.parse_local_value_binding();
            }
            // §8.1: a type annotation is module-boundary-only. A local
            // `name : Type ≔ …` gets a clear error here rather than falling
            // through the operator ladder and failing on an unrelated token.
            if matches!(self.peek_at(1), TokenKind::Colon) {
                return Err(self.error(
                    "a type annotation (':') is not valid on a local binding — \
                     CONCRETE_SYMBOLIC_GRAMMAR.md §8.1's annotation is module-boundary-only \
                     (§9); locally the type is always inferred",
                ));
            }
        }
        if self.at_function_declaration() {
            return self.parse_local_function_bindings();
        }
        self.parse_pipe()
    }

    /// Whether the next tokens are `λ name`, a function declaration rather
    /// than an anonymous lambda.
    fn at_function_declaration(&self) -> bool {
        matches!(self.peek(), TokenKind::Lambda) && matches!(self.peek_at(1), TokenKind::Ident(_))
    }

    /// `LocalBinding ::= ValueDeclaration Expression`. Chains of local
    /// bindings fall out of the continuation being a full expression.
    fn parse_local_value_binding(&mut self) -> PResult<Expression> {
        let (name, name_span) = self.ident("a name")?;
        self.expect(&TokenKind::Bind, "'≔'")?;
        let is_mutable = self.eat(&TokenKind::Mut);
        if matches!(self.peek(), TokenKind::Export) {
            return Err(self.error(
                "'⟳' (export) is not valid on a local binding — CONCRETE_SYMBOLIC_GRAMMAR.md \
                 §7.4's LocalBinding has no export modifier; only module-level ValueDeclaration \
                 does (§8.1)",
            ));
        }
        let value = self.parse_expression()?;
        let decl_span = name_span.merge(value.span());
        let decl = ValueDeclaration {
            name,
            name_span,
            ty: None,
            is_mutable,
            is_exported: false,
            value,
            span: decl_span,
        };
        let body = self.parse_expression()?;
        let span = decl_span.merge(body.span());
        Ok(Expression::LocalValue {
            decl: Box::new(decl),
            body: Box::new(body),
            span,
        })
    }

    /// `LocalBinding ::= FunctionDeclaration+ Expression`: one *maximal* run
    /// of adjacent function declarations, since the run's end must be known
    /// before the trailing body can start.
    fn parse_local_function_bindings(&mut self) -> PResult<Expression> {
        let mut decls = Vec::new();
        loop {
            let function = self.parse_function_declaration()?;
            if function.is_exported {
                return Err(self.error(
                    "'⟳' (export) is not valid on a local function declaration — \
                     CONCRETE_SYMBOLIC_GRAMMAR.md §8.4 restricts export to top-level Module \
                     declarations; only its position (top-level vs. nested), not its grammar \
                     shape, makes this invalid",
                ));
            }
            decls.push(function);
            if !self.at_function_declaration() {
                break;
            }
        }
        let body = self.parse_expression()?;
        let span = decls[0].span.merge(body.span());
        Ok(Expression::LocalFunctions {
            decls,
            body: Box::new(body),
            span,
        })
    }

    /// A primary followed by any number of `(args)`, `[index]`, `.field` and
    /// `.{ update }` suffixes. Greedy (§7.2, §7.9): all four continue the
    /// same chain, so `f(x)[i].name` and `arr[i][j]` need no parentheses.
    pub(super) fn parse_application(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.peek(), TokenKind::LParen | TokenKind::LBracket) {
                self.link()?;
            }
            expr = match self.peek() {
                TokenKind::LParen => self.parse_call(expr)?,
                TokenKind::Dot => self.parse_field_or_update(expr)?,
                TokenKind::LBracket => self.parse_index(expr)?,
                _ => return Ok(expr),
            };
        }
    }

    fn parse_call(&mut self, callable: Expression) -> PResult<Expression> {
        self.advance(); // consume '('
        if matches!(self.peek(), TokenKind::RParen) {
            return Err(self.error(
                "expected at least one argument: zero-argument application ('f()') is not \
                 valid Obfusku — SEMANTIC_CORE.md's Apply is always single-argument and \
                 spec defines no Unit value to stand in for a zero-arity call",
            ));
        }
        let args = self.comma_list(Self::parse_argument)?;
        let end = self.expect(&TokenKind::RParen, "')'")?;
        let span = callable.span().merge(end);
        Ok(Expression::Application {
            callable: Box::new(callable),
            args,
            span,
        })
    }

    /// `Argument ::= Expression | "•"` (§7.2).
    fn parse_argument(&mut self) -> PResult<Argument> {
        if matches!(self.peek(), TokenKind::Hole) {
            Ok(Argument::Hole(self.advance().span))
        } else {
            Ok(Argument::Expr(self.parse_expression()?))
        }
    }

    /// `.name` (a field read) or `.{ name: value … }` (a record update).
    fn parse_field_or_update(&mut self, base: Expression) -> PResult<Expression> {
        self.advance(); // consume '.'
        match self.peek().clone() {
            TokenKind::Ident(name) if !starts_uppercase(&name) => {
                let end = self.advance().span;
                let span = base.span().merge(end);
                Ok(Expression::Field {
                    base: Box::new(base),
                    name,
                    span,
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let fields = self.named_fields(Self::parse_expression)?;
                let end = self.expect(&TokenKind::RBrace, "'}'")?;
                let span = base.span().merge(end);
                Ok(Expression::Update {
                    base: Box::new(base),
                    fields,
                    span,
                })
            }
            other => Err(self.error(format!(
                "expected a field name or '{{' after '.', found {other:?}"
            ))),
        }
    }

    fn parse_index(&mut self, array: Expression) -> PResult<Expression> {
        self.advance(); // consume '['
        let index = self.parse_expression()?;
        let end = self.expect(&TokenKind::RBracket, "']'")?;
        let span = array.span().merge(end);
        Ok(Expression::Index {
            array: Box::new(array),
            index: Box::new(index),
            span,
        })
    }

    /// Everything that starts an expression without a prefix operator.
    fn parse_primary(&mut self) -> PResult<Expression> {
        let span = self.peek_span();
        match self.peek().clone() {
            // `2^63` lexes as `i64::MIN`; it is legal only behind `−`, which
            // `parse_unary` handles before reaching here.
            TokenKind::Int(i64::MIN) => Err(self.error(
                "9223372036854775808 is out of range as a literal; write −9223372036854775808",
            )),
            TokenKind::Int(v) => Ok(self.atom(Expression::Int(v, span))),
            TokenKind::Real(v) => Ok(self.atom(Expression::Real(v, span))),
            TokenKind::Str(v) => Ok(self.atom(Expression::Str(v, span))),
            TokenKind::Bool(v) => Ok(self.atom(Expression::Bool(v, span))),
            // `∅` in expression position is the one Unit value; a type
            // position never reaches `parse_primary`.
            TokenKind::TyUnit => Ok(self.atom(Expression::Unit(span))),
            TokenKind::PipeRef => Ok(self.atom(Expression::PipeRef(span))),
            TokenKind::Mut => self.parse_explicit_reference(span),
            TokenKind::Ident(name) => self.parse_name(name, span),
            TokenKind::Lambda => self.parse_lambda(),
            TokenKind::Match => self.parse_match(),
            TokenKind::Raise => self.parse_raise(),
            TokenKind::Catch => self.parse_catch(),
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expression()?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            TokenKind::LBracket => self.parse_array_literal(span),
            other => Err(self.error(format!("expected an expression, found {other:?}"))),
        }
    }

    fn atom(&mut self, expression: Expression) -> Expression {
        self.advance();
        expression
    }

    /// §7.7: `˚name` (the raw cell), or `˚name ⚙︎ value` when a rebind
    /// follows, in which case the `˚` also marks an explicit capture.
    fn parse_explicit_reference(&mut self, start: Span) -> PResult<Expression> {
        self.advance(); // consume '˚'
        let (name, name_span) = self.ident("a name after '˚'")?;
        if self.eat(&TokenKind::Rebind) {
            let value = self.parse_expression()?;
            let span = start.merge(value.span());
            Ok(Expression::Rebind {
                name,
                name_span,
                value: Box::new(value),
                is_explicit: true,
                span,
            })
        } else {
            Ok(Expression::ExplicitRef(name, start.merge(name_span)))
        }
    }

    /// A name: a plain reference, a record construction (`Tag { … }`, only
    /// right after an uppercase name; a tag with no `{` is a nullary
    /// constructor reference), or a rebind (`name ⚙︎ value`).
    fn parse_name(&mut self, name: String, span: Span) -> PResult<Expression> {
        self.advance();
        if starts_uppercase(&name) && matches!(self.peek(), TokenKind::LBrace) {
            return self.parse_named_construction(name, span);
        }
        if self.eat(&TokenKind::Rebind) {
            let value = self.parse_expression()?;
            let full_span = span.merge(value.span());
            return Ok(Expression::Rebind {
                name,
                name_span: span,
                value: Box::new(value),
                is_explicit: false,
                span: full_span,
            });
        }
        Ok(Expression::Reference(name, span))
    }

    /// §7.9: `[ (Expression ("," Expression)*)? ]`; empty is legal, unlike a
    /// call or a lambda.
    fn parse_array_literal(&mut self, start: Span) -> PResult<Expression> {
        self.advance(); // consume '['
        let elements = if matches!(self.peek(), TokenKind::RBracket) {
            Vec::new()
        } else {
            self.comma_list(Self::parse_expression)?
        };
        let end = self.expect(&TokenKind::RBracket, "']'")?;
        Ok(Expression::ArrayLiteral {
            elements,
            span: start.merge(end),
        })
    }

    /// §7.3: `"λ" "(" Parameter ("," Parameter)* ")" "→" Expression`.
    ///
    /// At least one parameter is mandatory, for the same reason a call needs
    /// an argument: Core's lambda and apply are single-argument, and the
    /// spec defines no unit value to stand in for an empty list.
    fn parse_lambda(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume 'λ'
        self.expect(&TokenKind::LParen, "'(' after 'λ'")?;
        if matches!(self.peek(), TokenKind::RParen) {
            return Err(self.error(
                "expected at least one parameter: zero-parameter lambdas ('λ() → …') are not \
                 valid Obfusku — SEMANTIC_CORE.md's Lambda is always single-parameter and spec \
                 defines no Unit value to stand in for a zero-arity parameter list",
            ));
        }
        let params = self.comma_list(Self::parse_parameter)?;
        self.expect(&TokenKind::RParen, "')'")?;
        self.expect(&TokenKind::Arrow, "'→'")?;
        let body = self.parse_expression()?;
        let span = start.merge(body.span());
        Ok(Expression::Lambda {
            params,
            body: Box::new(body),
            span,
        })
    }

    /// `Parameter ::= name (":" Type)?` (§7.3): the annotation is optional.
    fn parse_parameter(&mut self) -> PResult<Parameter> {
        let (name, span) = self.ident("a parameter name")?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_ref()?)
        } else {
            None
        };
        Ok(Parameter { name, ty, span })
    }

    /// §11: `"⟡" Expression "{" MatchArm ("⟢" MatchArm)* "}"`, closed by the
    /// generic list closer `}` (§6) rather than a bespoke end-of-match glyph.
    fn parse_match(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume '⟡'
        let scrutinee = self.parse_expression()?;
        self.expect(&TokenKind::LBrace, "'{' to start the match arms")?;
        let mut arms = vec![self.parse_match_arm()?];
        while self.eat(&TokenKind::ArmSep) {
            arms.push(self.parse_match_arm()?);
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Expression::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: start.merge(end),
        })
    }

    /// `MatchArm ::= Pattern "→" Expression`.
    fn parse_match_arm(&mut self) -> PResult<MatchArm> {
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Arrow, "'→'")?;
        let result = self.parse_expression()?;
        let span = pattern.span().merge(result.span());
        Ok(MatchArm {
            pattern,
            result,
            span,
        })
    }

    /// `NamedConstruction ::= Tag "{" name ":" Expression (⟢ …)* "}"` (§7.6);
    /// the `{` is peeked, not yet consumed.
    fn parse_named_construction(&mut self, tag: String, tag_span: Span) -> PResult<Expression> {
        self.advance(); // consume '{'
        let fields = self.named_fields(Self::parse_expression)?;
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Expression::NamedConstruction {
            tag,
            fields,
            span: tag_span.merge(end),
        })
    }

    /// §7.8: `"☄" Expression`.
    fn parse_raise(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume '☄'
        let value = self.parse_expression()?;
        let span = start.merge(value.span());
        Ok(Expression::Raise(Box::new(value), span))
    }

    /// §7.8: `"☊" Expression Lambda`. The body stops cleanly at `λ`, which
    /// cannot continue any expression, so the next parse picks up exactly the
    /// handler.
    fn parse_catch(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume '☊'
        let body = self.parse_expression()?;
        let handler_expr = self.parse_expression()?;
        let span = start.merge(handler_expr.span());
        Ok(Expression::Catch {
            body: Box::new(body),
            handler: Box::new(handler_expr),
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Argument, Expression};
    use crate::parser::testing::{error, expression};

    #[test]
    fn literals_and_names() {
        assert!(matches!(expression("5"), Expression::Int(5, _)));
        assert!(matches!(expression("2.5"), Expression::Real(r, _) if r == 2.5));
        assert!(matches!(expression("\"s\""), Expression::Str(s, _) if s == "s"));
        assert!(matches!(expression("◎"), Expression::Bool(false, _)));
        assert!(matches!(expression("∅"), Expression::Unit(_)));
        assert!(matches!(expression("x"), Expression::Reference(n, _) if n == "x"));
    }

    #[test]
    fn a_call_collects_comma_separated_arguments() {
        let Expression::Application { args, .. } = expression("f(1, 2, 3)") else {
            panic!("expected a call");
        };
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn calls_chain_greedily() {
        let Expression::Application { callable, .. } = expression("f(1)(2)") else {
            panic!("expected a call");
        };
        assert!(matches!(*callable, Expression::Application { .. }));
    }

    #[test]
    fn a_zero_argument_call_is_rejected() {
        assert!(error("v ≔ f()\n❧").contains("at least one argument"));
    }

    #[test]
    fn a_hole_is_an_argument() {
        let Expression::Application { args, .. } = expression("f(•, 2)") else {
            panic!("expected a call");
        };
        assert!(matches!(args[0], Argument::Hole(_)));
        assert!(matches!(args[1], Argument::Expr(_)));
    }

    #[test]
    fn suffixes_chain_in_any_order() {
        let e = expression("f(x)[0].name");
        let Expression::Field { base, name, .. } = e else {
            panic!("expected a field read");
        };
        assert_eq!(name, "name");
        let Expression::Index { array, .. } = *base else {
            panic!("expected an index");
        };
        assert!(matches!(*array, Expression::Application { .. }));
    }

    #[test]
    fn a_record_update_lists_fields_in_source_order() {
        let Expression::Update { fields, .. } = expression("p.{ y: 1 ⟢ x: 2 }") else {
            panic!("expected an update");
        };
        let names: Vec<_> = fields.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["y", "x"]);
    }

    #[test]
    fn a_dot_must_be_followed_by_a_name_or_a_brace() {
        assert!(error("v ≔ p.1\n❧").contains("expected a field name or '{'"));
        assert!(error("v ≔ p.Foo\n❧").contains("expected a field name or '{'"));
    }

    #[test]
    fn array_literals_may_be_empty() {
        assert!(
            matches!(expression("[]"), Expression::ArrayLiteral { elements, .. } if elements.is_empty())
        );
        assert!(
            matches!(expression("[1, 2]"), Expression::ArrayLiteral { elements, .. } if elements.len() == 2)
        );
    }

    #[test]
    fn indexing_takes_one_expression() {
        assert!(matches!(expression("a[i ✚ 1]"), Expression::Index { .. }));
        assert!(error("v ≔ a[]\n❧").contains("expected an expression"));
    }

    #[test]
    fn a_lambda_lists_parameters_with_optional_annotations() {
        let Expression::Lambda { params, .. } = expression("λ(x: ⟁, y) → x") else {
            panic!("expected a lambda");
        };
        assert!(params[0].ty.is_some());
        assert!(params[1].ty.is_none());
    }

    #[test]
    fn a_lambda_needs_a_parameter_and_an_arrow() {
        assert!(error("v ≔ λ() → 1\n❧").contains("at least one parameter"));
        assert!(error("v ≔ λ(x) 1\n❧").contains("expected '→'"));
    }

    #[test]
    fn a_match_lists_arms_separated_by_the_generic_separator() {
        let Expression::Match { arms, .. } = expression("⟡ x { 1 → 2 ⟢ _ → 3 }") else {
            panic!("expected a match");
        };
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn a_match_needs_an_opening_brace() {
        assert!(error("v ≔ ⟡ x 1 → 2\n❧").contains("'{' to start the match arms"));
    }

    #[test]
    fn a_tag_with_braces_is_a_record_construction() {
        let Expression::NamedConstruction { tag, fields, .. } = expression("P { x: 1 ⟢ y: 2 }")
        else {
            panic!("expected a construction");
        };
        assert_eq!(tag, "P");
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn a_tag_without_braces_is_a_plain_reference() {
        assert!(matches!(expression("None"), Expression::Reference(n, _) if n == "None"));
    }

    #[test]
    fn raise_and_catch_parse_with_their_operands() {
        assert!(matches!(expression("☄ Boom"), Expression::Raise(..)));
        let Expression::Catch { handler, .. } = expression("☊ f(1) λ(e) → 0") else {
            panic!("expected a catch");
        };
        assert!(matches!(*handler, Expression::Lambda { .. }));
    }

    #[test]
    fn rebind_forms() {
        assert!(matches!(
            expression("x ⚙︎ 1"),
            Expression::Rebind {
                is_explicit: false,
                ..
            }
        ));
        assert!(matches!(
            expression("(˚x ⚙︎ 1)"),
            Expression::Rebind {
                is_explicit: true,
                ..
            }
        ));
        // `v ≔ ˚…` would read as the mutability marker (§8.1), so these are
        // parenthesized.
        assert!(matches!(expression("(˚x)"), Expression::ExplicitRef(n, _) if n == "x"));
        assert!(error("v ≔ (˚ 1)\n❧").contains("expected a name after '˚'"));
    }

    #[test]
    fn a_local_value_scopes_over_the_next_expression() {
        let Expression::LocalValue { decl, body, .. } = expression("a ≔ 1 a ✚ 1") else {
            panic!("expected a local value");
        };
        assert_eq!(decl.name, "a");
        assert!(!decl.is_exported);
        assert!(matches!(*body, Expression::BinaryOp { .. }));
    }

    #[test]
    fn local_bindings_forbid_annotations_and_exports() {
        assert!(error("v ≔ a: ⟁ ≔ 1 a\n❧").contains("not valid on a local binding"));
        assert!(error("v ≔ a ≔ ⟳ 1 a\n❧").contains("(export) is not valid on a local binding"));
    }

    #[test]
    fn a_local_function_run_is_collected_whole() {
        let src = "λf(a: ⟁): ⟁ → a λg(b: ⟁): ⟁ → b f(1)";
        let Expression::LocalFunctions { decls, body, .. } = expression(src) else {
            panic!("expected local functions");
        };
        assert_eq!(decls.len(), 2);
        assert!(matches!(*body, Expression::Application { .. }));
    }

    #[test]
    fn a_local_function_cannot_be_exported() {
        assert!(error("v ≔ λf(a: ⟁): ⟁ →⟳ a f(1)\n❧").contains("not valid on a local function"));
    }

    #[test]
    fn parenthesized_expressions_are_transparent() {
        assert!(matches!(expression("(1)"), Expression::Int(1, _)));
    }

    #[test]
    fn an_expression_is_required_where_one_is_expected() {
        assert!(error("v ≔ )\n❧").contains("expected an expression, found RParen"));
    }
}
