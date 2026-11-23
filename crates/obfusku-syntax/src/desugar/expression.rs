//! Expressions: surface to Core.
//!
//! One method per surface form that needs real work; the trivial ones are
//! inline in [`Lowerer::lower_expr`]. `pipe_ref` is the name `◈` currently
//! stands for, if any (see the module documentation).

use super::lowerer::{Lowered, Lowerer, both, collect_all, fail};
use super::mutability::{explicit_captures, rebind_targets};
use super::types::{declared_function_type, lower_type_ref, rename_type_params, type_var_names};
use crate::ast::{
    Argument, BinOp, Expression, FunctionDeclaration, MatchArm, Parameter, Stage, UnOp,
    ValueDeclaration,
};
use obfusku_core::ast::{self as core, Literal};
use obfusku_diagnostics::Span;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

fn lambda(param: String, body: core::Expr, span: Span) -> core::Expr {
    core::Expr::Lambda {
        param,
        param_ty: None,
        body: Rc::new(body),
        span,
    }
}

fn apply(func: core::Expr, arg: core::Expr, span: Span) -> core::Expr {
    core::Expr::Apply {
        func: Box::new(func),
        arg: Box::new(arg),
        span,
    }
}

fn bool_arm(is_true: bool, result: core::Expr, span: Span) -> core::MatchArm {
    core::MatchArm {
        pattern: core::Pattern::Lit(Literal::Bool(is_true), span),
        result: Rc::new(result),
        span,
    }
}

/// `And`/`Or` never reach here: they lower to `Match` first and are omitted
/// from these arms deliberately, not left to a wildcard.
fn lower_binop(op: BinOp) -> core::BinOp {
    match op {
        BinOp::Add => core::BinOp::Add,
        BinOp::Sub => core::BinOp::Sub,
        BinOp::Mul => core::BinOp::Mul,
        BinOp::Div => core::BinOp::Div,
        BinOp::Mod => core::BinOp::Mod,
        BinOp::Lt => core::BinOp::Lt,
        BinOp::Gt => core::BinOp::Gt,
        BinOp::Le => core::BinOp::Le,
        BinOp::Ge => core::BinOp::Ge,
        BinOp::Eq => core::BinOp::Eq,
        BinOp::NotEq => core::BinOp::NotEq,
        BinOp::Xor => core::BinOp::Xor,
        BinOp::And | BinOp::Or => {
            unreachable!("And/Or are lowered to Match by lower_binary, never reach here")
        }
    }
}

fn lower_unop(op: UnOp) -> core::UnOp {
    match op {
        UnOp::Not => core::UnOp::Not,
        UnOp::Neg => core::UnOp::Neg,
    }
}

impl Lowerer {
    pub(super) fn lower_expr(
        &mut self,
        expr: &Expression,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        match expr {
            Expression::Int(v, span) => Ok(core::Expr::Lit(Literal::Int(*v), *span)),
            Expression::Real(v, span) => Ok(core::Expr::Lit(Literal::Real(*v), *span)),
            Expression::Str(v, span) => Ok(core::Expr::Lit(Literal::Str(v.clone()), *span)),
            Expression::Bool(v, span) => Ok(core::Expr::Lit(Literal::Bool(*v), *span)),
            Expression::Unit(span) => Ok(core::Expr::Lit(Literal::Unit, *span)),

            // §12/§7.7 ReadForm: an ordinary reference to a mutable name
            // reads its cell at this use site; anything else is a plain Var.
            Expression::Reference(name, span) => {
                let var = core::Expr::Var(name.clone(), *span);
                Ok(if self.mut_names.contains(name) {
                    core::Expr::MutRead {
                        cell: Box::new(var),
                        span: *span,
                    }
                } else {
                    var
                })
            }

            // §7.7 ReferenceForm: always the raw binding, never a read.
            Expression::ExplicitRef(name, span) => Ok(core::Expr::Var(name.clone(), *span)),

            // §7.7 RebindForm. `explicit` does not change the Core shape
            // (both spellings lower to the same `MutRebind`); it only affects
            // whether the occurrence counts as a capture for the enclosing
            // lambda's check, made by `lower_lambda`.
            Expression::Rebind {
                name,
                name_span,
                value,
                span,
                ..
            } => self.lower_rebind(name, *name_span, value, *span, pipe_ref),

            Expression::PipeRef(span) => match pipe_ref {
                Some(name) => Ok(core::Expr::Var(name.to_string(), *span)),
                None => fail(
                    *span,
                    "'◈' used outside of a pipeline stage — it has no enclosing '▷' to bind to",
                ),
            },

            Expression::Lambda { params, body, span } => {
                self.lower_lambda(params, body, *span, pipe_ref)
            }
            Expression::Application {
                callable,
                args,
                span,
            } => self.lower_application(callable, args, *span, pipe_ref),
            Expression::Pipe {
                subject,
                stage,
                span,
            } => self.lower_pipe(subject, stage, *span, pipe_ref),
            Expression::Match {
                scrutinee,
                arms,
                span,
            } => self.lower_match(scrutinee, arms, *span, pipe_ref),
            Expression::BinaryOp { op, lhs, rhs, span } => {
                self.lower_binary(*op, lhs, rhs, *span, pipe_ref)
            }
            Expression::UnaryOp { op, operand, span } => Ok(core::Expr::UnaryOp {
                op: lower_unop(*op),
                operand: Box::new(self.lower_expr(operand, pipe_ref)?),
                span: *span,
            }),
            Expression::Raise(value, span) => Ok(core::Expr::Raise {
                value: Box::new(self.lower_expr(value, pipe_ref)?),
                span: *span,
            }),
            Expression::Catch {
                body,
                handler,
                span,
            } => self.lower_catch(body, handler, *span, pipe_ref),

            // §4: reorders source-order fields into declared positional order,
            // producing an ordinary `Constructor`.
            Expression::NamedConstruction { tag, fields, span } => {
                let ordered = self.reorder_named_fields(tag, fields, *span)?;
                let args = collect_all(ordered.into_iter().map(|v| self.lower_expr(v, pipe_ref)))?;
                Ok(core::Expr::Constructor {
                    tag: tag.clone(),
                    args,
                    span: *span,
                })
            }

            Expression::LocalValue { decl, body, span } => {
                self.lower_local_value(decl, body, pipe_ref, *span)
            }
            Expression::LocalFunctions { decls, body, span } => {
                self.lower_local_functions(decls, body, pipe_ref, *span)
            }

            Expression::ArrayLiteral { elements, span } => Ok(core::Expr::ArrayLiteral {
                elements: collect_all(elements.iter().map(|e| self.lower_expr(e, pipe_ref)))?,
                span: *span,
            }),
            Expression::Index { array, index, span } => Ok(core::Expr::Index {
                array: Box::new(self.lower_expr(array, pipe_ref)?),
                index: Box::new(self.lower_expr(index, pipe_ref)?),
                span: *span,
            }),
            Expression::Field { base, name, span } => Ok(core::Expr::Field {
                base: Box::new(self.lower_expr(base, pipe_ref)?),
                name: name.clone(),
                span: *span,
            }),
            Expression::Update { base, fields, span } => {
                let base = self.lower_expr(base, pipe_ref)?;
                let fields = fields
                    .iter()
                    .map(|(name, value)| Ok((name.clone(), self.lower_expr(value, pipe_ref)?)))
                    .collect::<Lowered<Vec<_>>>()?;
                Ok(core::Expr::Update {
                    base: Box::new(base),
                    fields,
                    span: *span,
                })
            }
        }
    }

    fn lower_rebind(
        &mut self,
        name: &str,
        name_span: Span,
        value: &Expression,
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        if !self.mut_names.contains(name) {
            return fail(
                name_span,
                format!("cannot rebind '{name}': not a mutable binding (declared with '≔˚')"),
            );
        }
        Ok(core::Expr::MutRebind {
            cell: Box::new(core::Expr::Var(name.to_string(), name_span)),
            new_value: Box::new(self.lower_expr(value, pipe_ref)?),
            span,
        })
    }

    /// `λ(a, b) → body` becomes `λa → λb → body`.
    fn lower_lambda(
        &mut self,
        params: &[Parameter],
        body: &Expression,
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        self.check_closure_rebinds(body)?;
        let rename = self.rename_type_variables(params);
        let mut result = self.lower_expr(body, pipe_ref)?;
        for p in params.iter().rev() {
            result = core::Expr::Lambda {
                param: p.name.clone(),
                param_ty: p
                    .ty
                    .as_ref()
                    .map(|t| rename_type_params(lower_type_ref(t), &rename)),
                body: Rc::new(result),
                span,
            };
        }
        Ok(result)
    }

    /// §13: rejects rebinding a mutable free variable the lambda does not
    /// `˚`-capture. Nested lambdas are checked when lowering reaches them.
    fn check_closure_rebinds(&self, body: &Expression) -> Lowered<()> {
        let mut captured = HashSet::new();
        explicit_captures(body, &mut captured);
        let mut targets = Vec::new();
        rebind_targets(body, &mut targets);
        for (name, span) in targets {
            if self.mut_names.contains(&name) && !captured.contains(&name) {
                return fail(
                    span,
                    format!(
                        "cannot rebind '{name}' here: an ordinary reference to it \
                         desugars to a read of its current value, not the cell itself — \
                         write '˚{name}' where this closure captures it, to get the \
                         cell reference `⚙︎` needs to rebind through \
                         (SEMANTIC_CORE.md §13)"
                    ),
                );
            }
        }
        Ok(())
    }

    /// A fresh, module-unique name per type variable in `params`.
    ///
    /// A variable shared across the parameters of *one* list (`λ(x: t, y: t)
    /// → …`) must resolve to one declared variable, as it already does for
    /// `λf(x: t, y: t): t → …`. But two *separately written* lambdas that
    /// reuse a spelling (an inner `λ(y: t)` in this one's body) must not
    /// share. Once curried, the checker cannot tell those apart, so the
    /// distinction is made here, while `params` is still one syntactic unit.
    fn rename_type_variables(&mut self, params: &[Parameter]) -> HashMap<String, String> {
        let mut rename = HashMap::new();
        for ty in params.iter().filter_map(|p| p.ty.as_ref()) {
            let mut free = Vec::new();
            type_var_names(ty, &mut free);
            for name in free {
                rename.entry(name).or_insert_with(|| self.fresh_type_var());
            }
        }
        rename
    }

    /// §7.2/§12.1: each hole becomes one fresh parameter, left to right, and
    /// the whole call is curried over just those (outermost = first hole).
    /// A hole never opens a scope of its own: one inside an argument's own
    /// call belongs to that inner application, which lowers its own `args`.
    fn lower_application(
        &mut self,
        callable: &Expression,
        args: &[Argument],
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        let hole_names: Vec<String> = args
            .iter()
            .filter(|a| matches!(a, Argument::Hole(_)))
            .map(|_| self.fresh())
            .collect();
        let mut holes = hole_names.iter();

        let mut result = self.lower_expr(callable, pipe_ref)?;
        for arg in args {
            let lowered = match arg {
                Argument::Expr(e) => self.lower_expr(e, pipe_ref)?,
                Argument::Hole(hole_span) => core::Expr::Var(
                    holes.next().expect("one fresh name per hole").clone(),
                    *hole_span,
                ),
            };
            result = apply(result, lowered, span);
        }
        Ok(hole_names
            .into_iter()
            .rev()
            .fold(result, |inner, name| lambda(name, inner, span)))
    }

    /// `subject ▷ f` is `f(subject)`; `subject ▷ (expr)` is
    /// `(λ$p → expr)(subject)` with `◈` bound to `$p`.
    fn lower_pipe(
        &mut self,
        subject: &Expression,
        stage: &Stage,
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        let subject = self.lower_expr(subject, pipe_ref)?;
        match stage {
            Stage::Callable(callable) => {
                let callee = self.lower_expr(callable, pipe_ref)?;
                Ok(apply(callee, subject, span))
            }
            Stage::Expr(inner) => {
                let fresh = self.fresh();
                let body = self.lower_expr(inner, Some(&fresh))?;
                Ok(apply(lambda(fresh, body, span), subject, span))
            }
        }
    }

    fn lower_match(
        &mut self,
        scrutinee: &Expression,
        arms: &[MatchArm],
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        let scrutinee = self.lower_expr(scrutinee, pipe_ref)?;
        let arms = collect_all(arms.iter().map(|arm| {
            // A bad pattern skips its arm's result: its binders are unknown.
            let pattern = self.lower_pattern(&arm.pattern)?;
            let result = self.lower_expr(&arm.result, pipe_ref)?;
            Ok(core::MatchArm {
                pattern,
                result: Rc::new(result),
                span: arm.span,
            })
        }))?;
        Ok(core::Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span,
        })
    }

    /// `∧` and `∨` become `Match` (§16, §20.2), never a binary operator:
    /// short-circuiting then falls out of `Match`'s own evaluation rule, with
    /// no special case in the evaluator. Every other operator is strict and
    /// lowers uniformly.
    fn lower_binary(
        &mut self,
        op: BinOp,
        lhs: &Expression,
        rhs: &Expression,
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        let lhs = self.lower_expr(lhs, pipe_ref)?;
        let rhs = self.lower_expr(rhs, pipe_ref)?;
        let literal = |value| core::Expr::Lit(Literal::Bool(value), span);
        let arms = match op {
            // `a ∧ b` is `⟡ a { ◉ → b ⟢ ◎ → ◎ }`.
            BinOp::And => vec![
                bool_arm(true, rhs, span),
                bool_arm(false, literal(false), span),
            ],
            // `a ∨ b` is `⟡ a { ◉ → ◉ ⟢ ◎ → b }`.
            BinOp::Or => vec![
                bool_arm(true, literal(true), span),
                bool_arm(false, rhs, span),
            ],
            other => {
                return Ok(core::Expr::BinaryOp {
                    op: lower_binop(other),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    span,
                });
            }
        };
        Ok(core::Expr::Match {
            scrutinee: Box::new(lhs),
            arms,
            span,
        })
    }

    /// §7.8: the handler must be a single-parameter lambda; its parameter
    /// and body become `handler_param` and `handler_body`, and it gets the
    /// same capture and rebind checks as any lambda.
    fn lower_catch(
        &mut self,
        body: &Expression,
        handler: &Expression,
        span: Span,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        let body = self.lower_expr(body, pipe_ref)?;
        let Expression::Lambda { params, .. } = handler else {
            return fail(
                handler.span(),
                "a 'catch' handler must be a single-parameter Lambda (e.g. 'λ(e) → ...') \
                 (CONCRETE_SYMBOLIC_GRAMMAR.md §7.8)",
            );
        };
        if params.len() != 1 {
            return fail(
                handler.span(),
                format!(
                    "a 'catch' handler must take exactly one parameter, found {}",
                    params.len()
                ),
            );
        }
        let core::Expr::Lambda {
            param: handler_param,
            body: handler_body,
            ..
        } = self.lower_expr(handler, pipe_ref)?
        else {
            unreachable!("a single-parameter surface lambda lowers to one Core lambda")
        };
        Ok(core::Expr::Catch {
            body: Box::new(body),
            handler_param,
            handler_body,
            span,
        })
    }

    /// A `˚` binding's value is wrapped in `MutCell` (§12).
    pub(super) fn lower_binding_value(
        &mut self,
        decl: &ValueDeclaration,
        pipe_ref: Option<&str>,
    ) -> Lowered<core::Expr> {
        let value = self.lower_expr(&decl.value, pipe_ref)?;
        Ok(if decl.is_mutable {
            core::Expr::MutCell {
                initial: Box::new(value),
                span: decl.span,
            }
        } else {
            value
        })
    }

    /// `λf(x: A, y: B): C → body`: a lambda with a name and mandatory
    /// annotations, not a distinct expression shape, so it reuses lambda
    /// lowering (and its capture checks, §13).
    ///
    /// The parameter annotations are deliberately *not* copied to `param_ty`:
    /// they reach the checker through the binding's `declared_type`, and
    /// stating them twice would check one annotation by two mechanisms.
    pub(super) fn lower_function_declaration(
        &mut self,
        function: &FunctionDeclaration,
    ) -> Lowered<core::Expr> {
        let params: Vec<Parameter> = function
            .params
            .iter()
            .map(|p| Parameter {
                name: p.name.clone(),
                ty: None,
                span: p.span,
            })
            .collect();
        self.lower_lambda(&params, &function.body, function.span, None)
    }

    /// The Core binding for a function declaration, local or top-level.
    pub(super) fn lower_function_binding(
        &mut self,
        function: &FunctionDeclaration,
        is_exported: bool,
    ) -> Lowered<core::Binding> {
        Ok(core::Binding {
            name: function.name.clone(),
            value: self.lower_function_declaration(function)?,
            is_exported,
            span: function.span,
            declared_type: Some(declared_function_type(function)),
        })
    }

    /// §11: `Let(name, value, body)`. The value is lowered in the *outer*
    /// mutability scope; the name's own mutability applies only to `body`.
    fn lower_local_value(
        &mut self,
        decl: &ValueDeclaration,
        body: &Expression,
        pipe_ref: Option<&str>,
        span: Span,
    ) -> Lowered<core::Expr> {
        let value = self.lower_binding_value(decl, pipe_ref);
        let body = self.with_mutability([(decl.name.clone(), decl.is_mutable)], |l| {
            l.lower_expr(body, pipe_ref)
        });
        let (value, body) = both(value, body)?;
        Ok(core::Expr::Let {
            name: decl.name.clone(),
            value: Box::new(value),
            body: Rc::new(body),
            span,
        })
    }

    /// §11: `LetRec(bindings, body)` for a run of local functions. The names
    /// are immutable in their own scope even if they shadow a mutable one.
    fn lower_local_functions(
        &mut self,
        decls: &[FunctionDeclaration],
        body: &Expression,
        pipe_ref: Option<&str>,
        span: Span,
    ) -> Lowered<core::Expr> {
        let names = decls.iter().map(|d| (d.name.clone(), false));
        let (bindings, body) = self.with_mutability(names, |l| {
            let bindings = collect_all(decls.iter().map(|d| l.lower_function_binding(d, false)));
            (bindings, l.lower_expr(body, pipe_ref))
        });
        let (bindings, body) = both(bindings, body)?;
        Ok(core::Expr::LetRec {
            bindings,
            body: Rc::new(body),
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::desugar::desugar;
    use crate::lexer::tokenize;
    use crate::parser::parse;
    use obfusku_core::ast::{self as core, Literal};
    use obfusku_diagnostics::{Diagnostic, SourceId};

    fn lower(source: &str) -> Result<core::Module, Vec<Diagnostic>> {
        let tokens = tokenize(source, SourceId(0)).unwrap();
        desugar(&parse(&tokens, SourceId(0)).unwrap())
    }

    /// The Core of the last binding in `source`.
    fn last_value(source: &str) -> core::Expr {
        lower(source)
            .unwrap_or_else(|d| panic!("{source:?}: {d:?}"))
            .all_bindings()
            .last()
            .expect("a binding")
            .value
            .clone()
    }

    fn message(source: &str) -> String {
        lower(source).unwrap_err().remove(0).message
    }

    #[test]
    fn short_circuit_and_becomes_a_match_that_skips_the_rhs() {
        let core::Expr::Match { arms, .. } = last_value("v ≔ ◉ ∧ ◎\n❧") else {
            panic!("expected a match");
        };
        assert!(matches!(
            arms[0].pattern,
            core::Pattern::Lit(Literal::Bool(true), _)
        ));
        assert!(matches!(
            &*arms[1].result,
            core::Expr::Lit(Literal::Bool(false), _)
        ));
    }

    #[test]
    fn short_circuit_or_returns_true_without_evaluating_the_rhs() {
        let core::Expr::Match { arms, .. } = last_value("v ≔ ◉ ∨ ◎\n❧") else {
            panic!("expected a match");
        };
        assert!(matches!(
            &*arms[0].result,
            core::Expr::Lit(Literal::Bool(true), _)
        ));
    }

    #[test]
    fn strict_operators_stay_binary_operations() {
        assert!(matches!(
            last_value("v ≔ 1 ✚ 2\n❧"),
            core::Expr::BinaryOp { .. }
        ));
        assert!(matches!(
            last_value("v ≔ ◉ ⊻ ◎\n❧"),
            core::Expr::BinaryOp { .. }
        ));
    }

    #[test]
    fn a_multi_parameter_lambda_is_curried() {
        let core::Expr::Lambda { param, body, .. } = last_value("v ≔ λ(a, b) → a\n❧") else {
            panic!("expected a lambda");
        };
        assert_eq!(param, "a");
        assert!(matches!(&*body, core::Expr::Lambda { param, .. } if param == "b"));
    }

    #[test]
    fn a_pipe_with_a_callable_stage_applies_it() {
        let core::Expr::Apply { func, arg, .. } = last_value("f ≔ λ(x) → x\nv ≔ 1 ▷ f\n❧")
        else {
            panic!("expected an application");
        };
        assert!(matches!(*func, core::Expr::Var(n, _) if n == "f"));
        assert!(matches!(*arg, core::Expr::Lit(Literal::Int(1), _)));
    }

    #[test]
    fn an_expression_stage_binds_the_pipe_ref_to_a_fresh_parameter() {
        let core::Expr::Apply { func, .. } = last_value("f ≔ λ(x) → x\nv ≔ 1 ▷ (f(◈))\n❧")
        else {
            panic!("expected an application");
        };
        let core::Expr::Lambda { param, body, .. } = *func else {
            panic!("expected a lambda");
        };
        assert!(param.starts_with("$pipe"));
        let core::Expr::Apply { arg, .. } = &*body else {
            panic!("expected the call");
        };
        assert!(matches!(&**arg, core::Expr::Var(n, _) if *n == param));
    }

    #[test]
    fn the_pipe_ref_outside_a_pipe_is_an_error() {
        assert!(message("v ≔ ◈\n❧").contains("outside of a pipeline stage"));
    }

    #[test]
    fn a_nested_stage_shadows_the_outer_pipe_ref() {
        let src = "f ≔ λ(x) → x\nv ≔ 1 ▷ (2 ▷ (f(◈)))\n❧";
        assert!(lower(src).is_ok());
    }

    #[test]
    fn holes_become_curried_parameters_in_left_to_right_order() {
        let core::Expr::Lambda {
            param: first, body, ..
        } = last_value("f ≔ λ(a, b, c) → a\nv ≔ f(•, 1, •)\n❧")
        else {
            panic!("expected a lambda");
        };
        let core::Expr::Lambda { param: second, .. } = &*body else {
            panic!("expected a second lambda");
        };
        assert!(first.starts_with("$pipe") && second.starts_with("$pipe"));
        assert_ne!(first, *second);
    }

    #[test]
    fn a_reference_to_a_mutable_name_reads_its_cell() {
        let m = lower("c ≔ ˚ 1\nv ≔ c\n❧").unwrap();
        let last = m.all_bindings().last().unwrap();
        assert!(matches!(last.value, core::Expr::MutRead { .. }));
    }

    #[test]
    fn a_mutable_binding_wraps_its_value_in_a_cell() {
        let m = lower("c ≔ ˚ 1\n❧").unwrap();
        let binding = m.all_bindings().last().unwrap();
        assert!(matches!(binding.value, core::Expr::MutCell { .. }));
    }

    #[test]
    fn rebinding_an_immutable_name_is_an_error() {
        assert!(message("c ≔ 1\nv ≔ c ⚙︎ 2\n❧").contains("not a mutable binding"));
    }

    #[test]
    fn rebinding_a_captured_mutable_without_the_marker_is_an_error() {
        let src = "c ≔ ˚ 1\nv ≔ λ(x) → c ⚙︎ x\n❧";
        assert!(message(src).contains("cannot rebind 'c' here"));
    }

    #[test]
    fn rebinding_with_the_explicit_marker_is_accepted() {
        assert!(lower("c ≔ ˚ 1\nv ≔ λ(x) → (˚c ⚙︎ x)\n❧").is_ok());
    }

    #[test]
    fn a_catch_handler_must_be_a_one_parameter_lambda() {
        assert!(message("v ≔ ☊ 1 2\n❧").contains("must be a single-parameter Lambda"));
        assert!(message("v ≔ ☊ 1 λ(a, b) → a\n❧").contains("exactly one parameter, found 2"));
    }

    #[test]
    fn a_catch_lowers_its_handler_into_param_and_body() {
        let core::Expr::Catch { handler_param, .. } = last_value("v ≔ ☊ 1 λ(e) → 0\n❧")
        else {
            panic!("expected a catch");
        };
        assert_eq!(handler_param, "e");
    }

    #[test]
    fn named_construction_reorders_fields_to_declaration_order() {
        let src = "P ≔ { x: ⟁ ⟢ y: ⟁ }\nv ≔ P { y: 2 ⟢ x: 1 }\n❧";
        let core::Expr::Constructor { args, .. } = last_value(src) else {
            panic!("expected a constructor");
        };
        assert!(matches!(args[0], core::Expr::Lit(Literal::Int(1), _)));
        assert!(matches!(args[1], core::Expr::Lit(Literal::Int(2), _)));
    }

    #[test]
    fn a_local_value_lowers_to_a_let_and_does_not_leak_mutability() {
        assert!(matches!(
            last_value("v ≔ a ≔ 1 a\n❧"),
            core::Expr::Let { .. }
        ));
        let leak = "c ≔ ˚ 0\nv ≔ (c ≔ 5 c)\n❧";
        let core::Expr::Let { body, .. } = last_value(leak) else {
            panic!("expected a let");
        };
        assert!(
            matches!(&*body, core::Expr::Var(..)),
            "the shadowing local is immutable"
        );
    }

    #[test]
    fn a_local_function_run_lowers_to_a_letrec() {
        let src = "v ≔ λf(a: ⟁): ⟁ → a λg(b: ⟁): ⟁ → b f(1)\n❧";
        let core::Expr::LetRec { bindings, .. } = last_value(src) else {
            panic!("expected a letrec");
        };
        assert_eq!(bindings.len(), 2);
        assert!(
            bindings
                .iter()
                .all(|b| !b.is_exported && b.declared_type.is_some())
        );
    }

    #[test]
    fn sibling_errors_are_all_reported() {
        let errs = lower("v ≔ [◈, ◈]\n❧").unwrap_err();
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn two_separate_lambdas_do_not_share_type_variables_but_one_list_does() {
        let shared = last_value("v ≔ λ(x: t, y: t) → x\n❧");
        let core::Expr::Lambda {
            param_ty: Some(first),
            body,
            ..
        } = shared
        else {
            panic!("expected an annotated lambda");
        };
        let core::Expr::Lambda {
            param_ty: Some(second),
            ..
        } = &*body
        else {
            panic!("expected a second lambda");
        };
        assert_eq!(first, *second);

        let separate = last_value("v ≔ λ(x: t) → λ(y: t) → x\n❧");
        let core::Expr::Lambda {
            param_ty: Some(outer),
            body,
            ..
        } = separate
        else {
            panic!("expected an annotated lambda");
        };
        let core::Expr::Lambda {
            param_ty: Some(inner),
            ..
        } = &*body
        else {
            panic!("expected an inner lambda");
        };
        assert_ne!(outer, *inner);
    }
}
