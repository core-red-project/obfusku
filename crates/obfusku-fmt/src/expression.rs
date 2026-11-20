//! Printing expressions (`CONCRETE_SYMBOLIC_GRAMMAR.md` §10).
//!
//! The rule that shapes everything here: print the fewest parentheses that
//! still re-parse to the same tree, and when unsure, add one. An extra pair
//! is harmless; a wrong re-parse silently changes a program.

use crate::glyph::{
    self, ARM_SEPARATOR, ARROW, CATCH, FALSE, HOLE, LAMBDA, MATCH, MUTABLE, PIPE, PIPE_SUBJECT,
    RAISE, REBIND, TRUE, UNIT,
};
use crate::precedence::{Prec, operand_minimums};
use crate::printer::Printer;
use obfusku_syntax::ast::{Argument, BinOp, Expression, MatchArm, Parameter, Stage, UnOp};

impl Printer {
    /// Prints `e` at a position that needs at least `min` to appear bare.
    ///
    /// Only the operator forms consult `min` themselves; every other form is
    /// either self-delimiting or owns the rest of the expression, and its
    /// caller decides about parentheses through [`Printer::operand`].
    pub(crate) fn expression(&mut self, e: &Expression, min: Prec) {
        match e {
            Expression::Int(value, _) => self.int(*value),
            Expression::Real(value, _) => self.real(*value),
            Expression::Str(value, _) => self.string_literal(value),
            Expression::Bool(value, _) => self.push(if *value { TRUE } else { FALSE }),
            Expression::Unit(_) => self.push(UNIT),
            Expression::Reference(name, _) => self.push_str(name),
            Expression::PipeRef(_) => self.push(PIPE_SUBJECT),
            Expression::ExplicitRef(name, _) => {
                self.push(MUTABLE);
                self.push_str(name);
            }
            Expression::Lambda { params, body, .. } => self.lambda(params, body),
            Expression::Application { callable, args, .. } => self.application(callable, args),
            Expression::Index { array, index, .. } => {
                self.operand(array, Prec::ATOM);
                self.enclosed('[', ']', |p| p.expression(index, Prec::BOTTOM));
            }
            Expression::Field { base, name, .. } => {
                self.operand(base, Prec::ATOM);
                self.push('.');
                self.push_str(name);
            }
            Expression::Update { base, fields, .. } => {
                self.operand(base, Prec::ATOM);
                self.push('.');
                self.braced_fields(fields.iter().map(|(n, v)| (n, v)), |p, v| {
                    p.expression(v, Prec::BOTTOM);
                });
            }
            Expression::ArrayLiteral { elements, .. } => {
                self.enclosed('[', ']', |p| {
                    p.separated(elements, ", ", |p, el| p.expression(el, Prec::BOTTOM));
                });
            }
            Expression::Pipe { subject, stage, .. } => self.pipe(subject, stage),
            Expression::Rebind {
                name,
                value,
                explicit,
                ..
            } => {
                if *explicit {
                    self.push(MUTABLE);
                }
                self.push_str(name);
                self.spaced(REBIND);
                self.expression(value, Prec::BOTTOM);
            }
            Expression::Match {
                scrutinee, arms, ..
            } => self.match_expression(scrutinee, arms),
            Expression::BinaryOp { op, lhs, rhs, .. } => {
                self.parenthesized_if(Prec::of_binary(*op) < min, |p| p.binary(*op, lhs, rhs));
            }
            Expression::UnaryOp { op, operand, .. } => {
                self.parenthesized_if(Prec::UNARY < min, |p| p.unary(*op, operand));
            }
            Expression::Raise(value, _) => {
                self.push(RAISE);
                self.push(' ');
                self.operand(value, Prec::ATOM);
            }
            Expression::Catch { body, handler, .. } => {
                self.push(CATCH);
                self.push(' ');
                self.operand(body, Prec::ATOM);
                self.push(' ');
                // The handler must stay a bare `λ`: the parser finds where
                // `body` ends by seeing an unparenthesized `λ`, and wrapping
                // it would read as a call continuing `body`.
                self.expression(handler, Prec::BOTTOM);
            }
            Expression::NamedConstruction { tag, fields, .. } => {
                self.push_str(tag);
                self.push(' ');
                self.braced_fields(fields.iter().map(|(n, v)| (n, v)), |p, v| {
                    p.expression(v, Prec::BOTTOM);
                });
            }
            Expression::LocalValue { .. } | Expression::LocalFunctions { .. } => {
                self.local_chain(e);
            }
        }
    }

    /// Prints `e` as an operand that needs at least `min` to appear bare,
    /// parenthesizing it otherwise.
    ///
    /// A local-binding chain is exempt: it always brings its own parentheses.
    pub(crate) fn operand(&mut self, e: &Expression, min: Prec) {
        let brings_own_parentheses = matches!(
            e,
            Expression::LocalValue { .. } | Expression::LocalFunctions { .. }
        );
        if Prec::of_expression(e) < min && !brings_own_parentheses {
            self.parenthesized(|p| p.expression(e, Prec::BOTTOM));
        } else {
            self.expression(e, min);
        }
    }

    fn parenthesized_if(&mut self, condition: bool, inner: impl FnOnce(&mut Self)) {
        if condition {
            self.parenthesized(inner);
        } else {
            inner(self);
        }
    }

    fn lambda(&mut self, params: &[Parameter], body: &Expression) {
        self.push(LAMBDA);
        self.parenthesized(|p| p.separated(params, ", ", Printer::parameter));
        self.spaced(ARROW);
        self.expression(body, Prec::BOTTOM);
    }

    fn parameter(&mut self, parameter: &Parameter) {
        self.push_str(&parameter.name);
        if let Some(ty) = &parameter.ty {
            self.push_str(": ");
            self.type_ref(ty);
        }
    }

    fn application(&mut self, callable: &Expression, args: &[Argument]) {
        self.operand(callable, Prec::ATOM);
        self.parenthesized(|p| {
            p.separated(args, ", ", |p, arg| match arg {
                Argument::Expr(e) => p.expression(e, Prec::BOTTOM),
                Argument::Hole(_) => p.push(HOLE),
            });
        });
    }

    fn pipe(&mut self, subject: &Expression, stage: &Stage) {
        self.operand(subject, Prec::ATOM);
        self.spaced(PIPE);
        match stage {
            Stage::Callable(callable) => self.operand(callable, Prec::ATOM),
            Stage::Expr(inner) => self.parenthesized(|p| p.expression(inner, Prec::BOTTOM)),
        }
    }

    /// `⟡ scrutinee {` one arm per line, one level deeper `}`.
    fn match_expression(&mut self, scrutinee: &Expression, arms: &[MatchArm]) {
        self.push(MATCH);
        self.push(' ');
        self.operand(scrutinee, Prec::ATOM);
        self.push_str(" {\n");
        let at = self.depth();
        self.nested(|p| {
            p.separated(arms, &format!(" {ARM_SEPARATOR}\n"), |p, arm| {
                p.indent(at + 1);
                p.pattern(&arm.pattern);
                p.spaced(ARROW);
                p.expression(&arm.result, Prec::BOTTOM);
            });
        });
        self.push('\n');
        self.indent(at);
        self.push('}');
    }

    /// A run of local bindings and the expression they scope over, always
    /// parenthesized, one binding per line, one level deeper:
    /// `(` / `x ≔ …` / `λg(…) → …` / body / `)`.
    fn local_chain(&mut self, e: &Expression) {
        self.push_str("(\n");
        let at = self.depth();
        self.nested(|p| {
            let mut current = e;
            loop {
                match current {
                    Expression::LocalValue { decl, body, .. } => {
                        p.indent(at + 1);
                        p.value_declaration(decl, 0);
                        p.push('\n');
                        current = body;
                    }
                    Expression::LocalFunctions { decls, body, .. } => {
                        for function in decls {
                            p.indent(at + 1);
                            p.function_declaration(function, 0);
                            p.push('\n');
                        }
                        current = body;
                    }
                    last => {
                        p.indent(at + 1);
                        p.expression(last, Prec::BOTTOM);
                        break;
                    }
                }
            }
        });
        self.push('\n');
        self.indent(at);
        self.push(')');
    }

    fn binary(&mut self, op: BinOp, lhs: &Expression, rhs: &Expression) {
        let (left_min, right_min) = operand_minimums(op);
        self.operand(lhs, left_min);
        self.spaced(glyph::binary(op));
        self.operand(rhs, right_min);
    }

    fn unary(&mut self, op: UnOp, operand: &Expression) {
        self.push_str(glyph::unary(op));
        self.operand(operand, Prec::UNARY);
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::format_source;

    /// Formats `v ≔ <expression>` and returns just the expression text.
    fn expr(expression: &str) -> String {
        format_source(&format!("v ≔ {expression}\n❧"))
            .strip_prefix("v ≔ ")
            .and_then(|s| s.strip_suffix("\n❧\n"))
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn literals_print_with_their_glyphs() {
        assert_eq!(expr("◉"), "◉");
        assert_eq!(expr("◎"), "◎");
        assert_eq!(expr("∅"), "∅");
        assert_eq!(expr("2.0"), "2.0");
        assert_eq!(expr("\"a\""), "\"a\"");
    }

    #[test]
    fn the_minimum_integer_round_trips() {
        assert_eq!(expr("−9223372036854775808"), "−9223372036854775808");
    }

    #[test]
    fn left_associative_chains_need_no_parentheses() {
        assert_eq!(expr("1 ☠︎ 2 ☠︎ 3"), "1 ☠︎ 2 ☠︎ 3");
    }

    #[test]
    fn a_right_nested_same_level_operand_is_grouped() {
        assert_eq!(expr("1 ☠︎ (2 ☠︎ 3)"), "1 ☠︎ (2 ☠︎ 3)");
    }

    #[test]
    fn tighter_operators_need_no_parentheses_looser_ones_do() {
        assert_eq!(expr("1 ✚ 2 ✱ 3"), "1 ✚ 2 ✱ 3");
        assert_eq!(expr("(1 ✚ 2) ✱ 3"), "(1 ✚ 2) ✱ 3");
    }

    #[test]
    fn comparisons_do_not_chain() {
        assert_eq!(expr("(1 < 2) < 3"), "(1 < 2) < 3");
        assert_eq!(expr("1 < 2 ≡ ◉"), "1 < 2 ≡ ◉");
    }

    #[test]
    fn unary_operators_bind_tighter_than_binary_ones() {
        assert_eq!(expr("−1 ✚ 2"), "−1 ✚ 2");
        assert_eq!(expr("−(1 ✚ 2)"), "−(1 ✚ 2)");
    }

    #[test]
    fn calls_indexing_and_fields_print_compactly() {
        assert_eq!(expr("f(1,  2)"), "f(1, 2)");
        assert_eq!(expr("a[ 0 ]"), "a[0]");
        assert_eq!(expr("p.x"), "p.x");
        assert_eq!(expr("[1,2,3]"), "[1, 2, 3]");
    }

    #[test]
    fn partial_application_keeps_its_holes() {
        assert_eq!(expr("f(•, 2)"), "f(•, 2)");
    }

    #[test]
    fn lambda_as_a_callee_is_parenthesized() {
        assert_eq!(expr("(λ(x) → x)(1)"), "(λ(x) → x)(1)");
    }

    #[test]
    fn lambda_parameters_keep_optional_annotations() {
        assert_eq!(expr("λ(x: ⟁, y) → x"), "λ(x: ⟁, y) → x");
    }

    #[test]
    fn named_construction_and_update_print_their_fields() {
        assert_eq!(expr("P { x: 1 ⟢ y: 2 }"), "P { x: 1 \u{27E2} y: 2 }");
        assert_eq!(expr("p.{ x: 1 }"), "p.{ x: 1 }");
    }

    #[test]
    fn pipes_print_a_callable_bare_and_anything_else_parenthesized() {
        assert_eq!(expr("1 ▷ f"), "1 ▷ f");
        assert_eq!(expr("1 ▷ (f(◈))"), "1 ▷ (f(◈))");
    }

    #[test]
    fn match_prints_one_arm_per_line_indented() {
        assert_eq!(
            expr("⟡ 1 { 1 → ◉ ⟢ _ → ◎ }"),
            "⟡ 1 {\n  1 → ◉ \u{27E2}\n  _ → ◎\n}"
        );
    }

    #[test]
    fn nested_match_indents_one_level_deeper() {
        let out = expr("⟡ 1 { _ → ⟡ 2 { _ → 3 } }");
        assert_eq!(out, "⟡ 1 {\n  _ → ⟡ 2 {\n    _ → 3\n  }\n}");
    }

    #[test]
    fn raise_and_catch_print_their_glyphs() {
        assert_eq!(expr("☄ Boom"), "☄ Boom");
        assert_eq!(expr("☊ f λ(e) → 0"), "☊ f λ(e) → 0");
    }

    #[test]
    fn rebind_prints_with_its_glyph() {
        assert_eq!(expr("x ⚙︎ 1"), "x ⚙︎ 1");
    }
}
