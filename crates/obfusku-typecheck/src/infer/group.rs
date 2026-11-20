//! Mutually recursive binding groups (`SEMANTIC_CORE.md` §11, §8.2).
//!
//! Top-level `LetRec` groups and expression-level `LetRec` forms share one
//! inference, [`Checker::infer_group`]; they differ only in what happens to
//! the member types afterwards.

use super::{Checker, Infer, TypeEnv};
use crate::types::{Scheme, Type};
use obfusku_core::ast::{Binding, Expr};
use obfusku_diagnostics::Diagnostic;

impl Checker {
    /// Infers every member of a group against its own placeholder slot and
    /// returns the member types in order.
    ///
    /// Every member is in scope in every member's value, at a *monomorphic*
    /// slot. Generalizing is left to the caller, after the whole group has
    /// settled: doing it earlier would let `generalize` mistake a sibling's
    /// still-unconstrained slot for a genuinely free variable.
    ///
    /// A declared annotation is unified into the slot *before* any body is
    /// inferred, so it is a real constraint rather than a post-hoc check.
    /// Every failure goes to `errors`; the result is `None` if any member
    /// failed, so one broken function does not hide its siblings' errors.
    pub(crate) fn infer_group(
        &mut self,
        members: &[Binding],
        env: &TypeEnv,
        errors: &mut Vec<Diagnostic>,
    ) -> Option<Vec<Type>> {
        let mut slots = Vec::with_capacity(members.len());
        let mut declared = Vec::with_capacity(members.len());
        for member in members {
            let slot = self.fresh();
            let declared_ty = member.declared_type.as_ref().map(|written| {
                let ty = self.instantiate_declared(written, member.span, errors);
                if let Err(d) = self.unify(&slot, &ty, member.span) {
                    errors.push(d);
                }
                ty
            });
            slots.push(slot);
            declared.push(declared_ty);
        }

        let mut group_env = env.clone();
        for (member, slot) in members.iter().zip(&slots) {
            group_env.insert(member.name.clone(), Scheme::monomorphic(slot.clone()));
        }

        let mut inferred = Vec::with_capacity(members.len());
        let mut failed = false;
        for ((member, slot), declared_ty) in members.iter().zip(&slots).zip(&declared) {
            let result = match declared_ty {
                Some(expected) => {
                    self.infer_lambda_chain_against(&member.value, expected, &group_env)
                }
                None => self.infer(&member.value, &group_env),
            };
            match result.and_then(|ty| self.expect_same(slot, &ty, member.span).map(|()| ty)) {
                Ok(ty) => inferred.push(ty),
                Err(mut diagnostics) => {
                    errors.append(&mut diagnostics);
                    failed = true;
                }
            }
        }
        (!failed).then_some(inferred)
    }

    /// Infers a member's curried `Lambda` chain against its declared
    /// function type, peeling one parameter type off `expected` per nested
    /// `Lambda` and seeding it into that parameter's scope.
    ///
    /// Seeding parameter types *before* the body is inferred matters:
    /// operators that need a resolved operand type (`✚` on two parameters)
    /// would otherwise see an unconstrained variable and fail, and only the
    /// final unification with the signature would ever supply the type.
    pub(crate) fn infer_lambda_chain_against(
        &mut self,
        expr: &Expr,
        expected: &Type,
        env: &TypeEnv,
    ) -> Infer<Type> {
        if let (Expr::Lambda { param, body, .. }, Type::Function(param_ty, ret_ty)) =
            (expr, expected)
        {
            let mut inner = env.clone();
            inner.insert(param.clone(), Scheme::monomorphic((**param_ty).clone()));
            let body_ty = self.infer_lambda_chain_against(body, ret_ty, &inner)?;
            Ok(Type::Function(param_ty.clone(), Box::new(body_ty)))
        } else {
            let actual = self.infer(expr, env)?;
            self.expect_same(&actual, expected, expr.span())?;
            Ok(actual)
        }
    }

    /// A local `LetRec`: infers the group, generalizes each member against
    /// the members before it, then types the body.
    pub(super) fn infer_letrec(
        &mut self,
        bindings: &[Binding],
        body: &Expr,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let mut errors = Vec::new();
        let types = self.infer_group(bindings, env, &mut errors);
        let Some(types) = types.filter(|_| errors.is_empty()) else {
            return Err(errors);
        };
        let mut body_env = env.clone();
        for (binding, ty) in bindings.iter().zip(&types) {
            let scheme = self.generalize(ty, &body_env);
            body_env.insert(binding.name.clone(), scheme);
        }
        self.infer(body, &body_env)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{apply, checker, int, lambda, span, string, var};
    use super::*;
    use obfusku_core::types::Type as CoreType;
    use std::rc::Rc;

    fn member(name: &str, value: Expr, declared: Option<CoreType>) -> Binding {
        Binding {
            name: name.to_string(),
            value,
            exported: false,
            span: span(),
            declared_type: declared,
        }
    }

    fn infer_types(members: &[Binding]) -> Result<Vec<Type>, Vec<Diagnostic>> {
        let mut c = checker();
        let mut errors = Vec::new();
        let types = c.infer_group(members, &TypeEnv::new(), &mut errors);
        match types {
            Some(types) if errors.is_empty() => {
                Ok(types.iter().map(|t| c.resolve(t)).collect())
            }
            _ => Err(errors),
        }
    }

    #[test]
    fn a_member_may_call_itself() {
        let looping = member("loop", lambda("n", apply(var("loop"), var("n"))), None);
        assert!(infer_types(&[looping]).is_ok());
    }

    #[test]
    fn members_see_each_other_regardless_of_order() {
        let first = member("a", lambda("x", apply(var("b"), var("x"))), None);
        let second = member("b", lambda("y", var("y")), None);
        assert_eq!(infer_types(&[first, second]).expect("infers").len(), 2);
    }

    #[test]
    fn a_declared_type_reaches_the_parameter_before_the_body() {
        let add = Expr::BinaryOp {
            op: obfusku_core::ast::BinOp::Add,
            lhs: Box::new(var("x")),
            rhs: Box::new(var("y")),
            span: span(),
        };
        let declared = CoreType::curried([CoreType::Int, CoreType::Int], CoreType::Int);
        let sum = member("sum", lambda("x", lambda("y", add)), Some(declared));
        let types = infer_types(&[sum]).expect("operands are known from the signature");
        assert_eq!(
            types[0],
            Type::function(Type::Int, Type::function(Type::Int, Type::Int))
        );
    }

    #[test]
    fn every_failing_member_is_reported() {
        let bad = |name| member(name, lambda("x", apply(var("x"), var("nowhere"))), None);
        let errors = infer_types(&[bad("a"), bad("b")]).expect_err("both fail");
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn a_declared_type_the_body_contradicts_is_rejected() {
        let declared = CoreType::function(CoreType::Int, CoreType::Int);
        let wrong = member("f", lambda("x", string("text")), Some(declared));
        assert!(infer_types(&[wrong]).is_err());
    }

    #[test]
    fn a_local_letrec_generalizes_its_members_for_the_body() {
        let id = member("id", lambda("x", var("x")), None);
        let at_int = apply(var("id"), int(1));
        let at_string = apply(var("id"), string("s"));
        let body = apply(lambda("ignored", at_string), at_int);
        let mut c = checker();
        let ty = c
            .infer_letrec(&[id], &body, &TypeEnv::new())
            .expect("generalized members are usable at two types");
        assert_eq!(c.resolve(&ty), Type::Str);
    }

    #[test]
    fn a_local_letrec_does_not_leak_its_members_past_its_body() {
        let id = member("id", lambda("x", var("x")), None);
        let letrec = Expr::LetRec {
            bindings: vec![id],
            body: Rc::new(int(0)),
            span: span(),
        };
        let mut c = checker();
        let env = TypeEnv::new();
        assert!(c.infer(&letrec, &env).is_ok());
        assert!(c.infer(&var("id"), &env).is_err());
    }
}
