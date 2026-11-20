//! The typing rule of each core form (`SEMANTIC_CORE.md` §17).
//!
//! `infer` is only a dispatch: every form delegates to a method named for it,
//! so each rule reads on its own. Forms with enough structure to need their
//! own file live elsewhere (`data`, `operator`, `matching`, `group`).

use super::{Checker, Infer, TypeEnv, reject};
use crate::types::{Scheme, Type};
use obfusku_core::ast::Expr;
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::Span;
use std::rc::Rc;

impl Checker {
    /// Infers the type of `expr` in `env`, extending the substitution as it
    /// learns.
    pub(crate) fn infer(&mut self, expr: &Expr, env: &TypeEnv) -> Infer<Type> {
        match expr {
            Expr::Lit(literal, _) => Ok(Type::of_literal(literal)),
            Expr::Var(name, span) => self.infer_var(name, *span, env),
            Expr::Lambda {
                param,
                param_ty,
                body,
                span,
            } => self.infer_lambda(param, param_ty.as_ref(), body, *span, env),
            Expr::Apply { func, arg, span } => self.infer_apply(func, arg, *span, env),
            Expr::MutCell { initial, .. } => Ok(Type::cell(self.infer(initial, env)?)),
            Expr::MutRead { cell, span } => self.infer_read(cell, *span, env),
            Expr::MutRebind {
                cell,
                new_value,
                span,
            } => self.infer_rebind(cell, new_value, *span, env),
            Expr::Constructor { tag, args, span } => self.infer_constructor(tag, args, *span, env),
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.infer_match(scrutinee, arms, *span, env),
            Expr::Raise { value, span } => self.infer_raise(value, *span, env),
            Expr::Catch {
                body,
                handler_param,
                handler_body,
                span,
            } => self.infer_catch(body, handler_param, handler_body, *span, env),
            Expr::BinaryOp { op, lhs, rhs, span } => self.infer_binop(*op, lhs, rhs, *span, env),
            Expr::UnaryOp { op, operand, span } => self.infer_unop(*op, operand, *span, env),
            Expr::Let {
                name, value, body, ..
            } => self.infer_let(name, value, body, env),
            Expr::LetRec { bindings, body, .. } => self.infer_letrec(bindings, body, env),
            Expr::ArrayLiteral { elements, span } => self.infer_array(elements, *span, env),
            Expr::Index { array, index, span } => self.infer_index(array, index, *span, env),
            Expr::Field { base, name, span } => self.infer_field(base, name, *span, env),
            Expr::Update { base, fields, span } => self.infer_update(base, fields, *span, env),
        }
    }

    fn infer_var(&mut self, name: &str, span: Span, env: &TypeEnv) -> Infer<Type> {
        match env.get(name) {
            Some(scheme) => {
                let scheme = scheme.clone();
                Ok(self.instantiate(&scheme, span))
            }
            None => reject(span, format!("unknown variable '{name}'")),
        }
    }

    /// An unannotated parameter is a fresh variable; an annotated one (§7.3)
    /// is that annotation, which constrains only this parameter. It is never
    /// a generalization rule of its own: the value restriction already
    /// governs that, and a concrete annotation simply leaves nothing free.
    fn infer_lambda(
        &mut self,
        param: &str,
        annotation: Option<&CoreType>,
        body: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let param_ty = match annotation {
            None => self.fresh(),
            Some(declared) => self.instantiate_lambda_annotation(declared, span)?,
        };
        let mut inner = env.clone();
        inner.insert(param.to_string(), Scheme::monomorphic(param_ty.clone()));
        let body_ty = self.infer(body, &inner)?;
        Ok(Type::function(param_ty, body_ty))
    }

    fn infer_apply(&mut self, func: &Expr, arg: &Expr, span: Span, env: &TypeEnv) -> Infer<Type> {
        let func_ty = self.infer(func, env)?;
        let arg_ty = self.infer(arg, env)?;
        match self.resolve(&func_ty) {
            Type::Function(param_ty, return_ty) => {
                self.expect_same_or(&param_ty, &arg_ty, span, |c| {
                    format!(
                        "argument type mismatch: expected {}, found {}",
                        c.show(&param_ty),
                        c.show(&arg_ty)
                    )
                })?;
                Ok(self.resolve(&return_ty))
            }
            Type::Var(_) => {
                let result_ty = self.fresh();
                let expected = Type::function(arg_ty, result_ty.clone());
                self.expect_same(&func_ty, &expected, span)?;
                Ok(self.resolve(&result_ty))
            }
            other => reject(
                span,
                format!(
                    "cannot apply a value of type {} as a function",
                    other.describe()
                ),
            ),
        }
    }

    fn infer_read(&mut self, cell: &Expr, span: Span, env: &TypeEnv) -> Infer<Type> {
        let cell_ty = self.infer(cell, env)?;
        match self.resolve(&cell_ty) {
            Type::Cell(inner) => Ok(*inner),
            Type::Var(_) => {
                let inner = self.fresh();
                self.expect_same(&cell_ty, &Type::cell(inner.clone()), span)?;
                Ok(inner)
            }
            other => reject(
                span,
                format!("cannot read: {} is not a mutable cell", other.describe()),
            ),
        }
    }

    /// Rebinding yields `Unit`, never the cell's element type.
    fn infer_rebind(
        &mut self,
        cell: &Expr,
        new_value: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let cell_ty = self.infer(cell, env)?;
        let value_ty = self.infer(new_value, env)?;
        match self.resolve(&cell_ty) {
            Type::Cell(inner) => {
                self.expect_same_or(&inner, &value_ty, span, |c| {
                    format!(
                        "cannot rebind: cell holds {}, but the new value has type {}",
                        c.show(&inner),
                        c.show(&value_ty)
                    )
                })?;
            }
            Type::Var(_) => self.expect_same(&cell_ty, &Type::cell(value_ty), span)?,
            other => {
                return reject(
                    span,
                    format!("cannot rebind: {} is not a mutable cell", other.describe()),
                );
            }
        }
        Ok(Type::Unit)
    }

    /// §15.2: the operand must be an `Exception`. The result unifies with
    /// whatever the context expects, so it is a fresh variable.
    fn infer_raise(&mut self, value: &Expr, span: Span, env: &TypeEnv) -> Infer<Type> {
        let value_ty = self.infer(value, env)?;
        self.expect_same(&Type::exception(), &value_ty, span)?;
        Ok(self.fresh())
    }

    /// Body and handler must agree on one result type; the handler's
    /// parameter is bound at `Exception`.
    fn infer_catch(
        &mut self,
        body: &Expr,
        handler_param: &str,
        handler_body: &Rc<Expr>,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let body_ty = self.infer(body, env)?;
        let mut handler_env = env.clone();
        handler_env.insert(
            handler_param.to_string(),
            Scheme::monomorphic(Type::exception()),
        );
        let handler_ty = self.infer(handler_body, &handler_env)?;
        self.expect_same(&body_ty, &handler_ty, span)?;
        Ok(body_ty)
    }

    /// The value is not in its own scope (a `Let` is non-recursive), and
    /// generalizes only if it is a syntactic value (§19.1).
    fn infer_let(
        &mut self,
        name: &str,
        value: &Expr,
        body: &Expr,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let value_ty = self.infer(value, env)?;
        let scheme = self
            .generalize_value(value, &value_ty, env)
            .unwrap_or_else(|| Scheme::monomorphic(value_ty));
        let mut inner = env.clone();
        inner.insert(name.to_string(), scheme);
        self.infer(body, &inner)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{apply, boolean, checker, int, lambda, span, string, var};
    use super::*;
    use obfusku_core::ast::Literal;

    fn infer(expr: &Expr) -> Infer<Type> {
        let mut c = checker();
        let ty = c.infer(expr, &TypeEnv::new())?;
        Ok(c.resolve(&ty))
    }

    fn message(result: Infer<Type>) -> String {
        result.expect_err("expected a type error")[0].message.clone()
    }

    #[test]
    fn literals_have_their_primitive_types() {
        assert_eq!(infer(&int(1)).unwrap_or(Type::Unit), Type::Int);
        assert_eq!(infer(&string("s")).unwrap_or(Type::Unit), Type::Str);
        let unit = Expr::Lit(Literal::Unit, span());
        assert_eq!(infer(&unit).unwrap_or(Type::Int), Type::Unit);
    }

    #[test]
    fn an_unbound_variable_is_reported_by_name() {
        assert_eq!(message(infer(&var("ghost"))), "unknown variable 'ghost'");
    }

    #[test]
    fn the_identity_lambda_maps_a_type_to_itself() {
        let ty = infer(&lambda("x", var("x"))).expect("infers");
        let Type::Function(parameter, result) = ty else {
            panic!("a lambda has a function type");
        };
        assert_eq!(parameter, result);
    }

    #[test]
    fn applying_resolves_to_the_result_type() {
        let call = apply(lambda("x", var("x")), int(3));
        assert_eq!(infer(&call).expect("infers"), Type::Int);
    }

    #[test]
    fn an_argument_of_the_wrong_type_names_both_types() {
        let wants_int = lambda("x", Expr::BinaryOp {
            op: obfusku_core::ast::BinOp::Mod,
            lhs: Box::new(var("x")),
            rhs: Box::new(int(2)),
            span: span(),
        });
        let text = message(infer(&apply(wants_int, string("no"))));
        assert_eq!(text, "argument type mismatch: expected Int, found String");
    }

    #[test]
    fn applying_a_non_function_is_rejected() {
        let text = message(infer(&apply(int(1), int(2))));
        assert_eq!(text, "cannot apply a value of type Int as a function");
    }

    #[test]
    fn a_cell_reads_back_its_element_type() {
        let cell = Expr::MutCell {
            initial: Box::new(int(0)),
            span: span(),
        };
        let read = Expr::MutRead {
            cell: Box::new(cell),
            span: span(),
        };
        assert_eq!(infer(&read).expect("infers"), Type::Int);
    }

    #[test]
    fn reading_a_non_cell_is_rejected() {
        let read = Expr::MutRead {
            cell: Box::new(int(0)),
            span: span(),
        };
        assert_eq!(message(infer(&read)), "cannot read: Int is not a mutable cell");
    }

    #[test]
    fn rebinding_yields_unit_and_rejects_a_value_of_another_type() {
        let cell = || {
            Box::new(Expr::MutCell {
                initial: Box::new(int(0)),
                span: span(),
            })
        };
        let rebind = |value| Expr::MutRebind {
            cell: cell(),
            new_value: Box::new(value),
            span: span(),
        };
        assert_eq!(infer(&rebind(int(1))).expect("infers"), Type::Unit);
        assert_eq!(
            message(infer(&rebind(boolean(true)))),
            "cannot rebind: cell holds Int, but the new value has type Bool"
        );
    }

    #[test]
    fn raising_requires_an_exception() {
        let raise = Expr::Raise {
            value: Box::new(int(1)),
            span: span(),
        };
        assert!(infer(&raise).is_err());
    }

    fn let_id(value: Expr, body: Expr) -> Expr {
        Expr::Let {
            name: "f".into(),
            value: Box::new(value),
            body: Rc::new(body),
            span: span(),
        }
    }

    #[test]
    fn a_let_bound_lambda_is_usable_at_two_types() {
        let at_int = apply(var("f"), int(1));
        let at_string = apply(var("f"), string("s"));
        let both = apply(lambda("ignored", at_string), at_int);
        let program = let_id(lambda("x", var("x")), both);
        assert_eq!(infer(&program).expect("generalizes"), Type::Str);
    }

    #[test]
    fn a_let_bound_call_result_is_not_generalized() {
        let identity_call = apply(lambda("x", var("x")), lambda("y", var("y")));
        let at_int = apply(var("f"), int(1));
        let at_string = apply(var("f"), string("s"));
        let both = apply(lambda("ignored", at_string), at_int);
        let program = let_id(identity_call, both);
        assert!(infer(&program).is_err());
    }
}
