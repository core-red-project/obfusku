//! The function a constructor declaration implies (`SEMANTIC_CORE.md` §9).

use obfusku_core::ast::{Binding, Expr};
use obfusku_diagnostics::Span;
use std::rc::Rc;

/// The binding `tag ≔ λ$field0 → … → Constructor(tag, [$field0, …])`.
///
/// A constructor is an ordinary curried function in scope, so it can be
/// passed around and partially applied; only its *pattern* form is special.
/// The parameter names start with `$`, which no source identifier can, so
/// they cannot shadow anything. A nullary constructor has no lambda: the
/// binding is the value itself.
///
/// Constructors are exported (a module's types are part of its interface)
/// and always `Let`-bound, never part of a `LetRec` group.
pub(super) fn constructor_binding(tag: &str, arity: usize, span: Span) -> Binding {
    let params: Vec<String> = (0..arity).map(|i| format!("$field{i}")).collect();
    let body = Expr::Constructor {
        tag: tag.to_string(),
        args: params.iter().map(|p| Expr::Var(p.clone(), span)).collect(),
        span,
    };
    let value = params
        .into_iter()
        .rev()
        .fold(body, |inner, param| Expr::Lambda {
            param,
            param_ty: None,
            body: Rc::new(inner),
            span,
        });
    Binding {
        name: tag.to_string(),
        value,
        exported: true,
        span,
        declared_type: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> Span {
        Span::default()
    }

    #[test]
    fn a_nullary_constructor_is_the_value_itself() {
        let b = constructor_binding("None", 0, at());
        assert!(
            matches!(&b.value, Expr::Constructor { tag, args, .. } if tag == "None" && args.is_empty())
        );
    }

    #[test]
    fn an_n_ary_constructor_is_a_curried_lambda_chain_in_field_order() {
        let b = constructor_binding("Pair", 2, at());
        let Expr::Lambda {
            param: first, body, ..
        } = &b.value
        else {
            panic!("expected a lambda");
        };
        assert_eq!(first, "$field0");
        let Expr::Lambda {
            param: second,
            body,
            ..
        } = &**body
        else {
            panic!("expected a second lambda");
        };
        assert_eq!(second, "$field1");
        let Expr::Constructor { args, .. } = &**body else {
            panic!("expected the constructor");
        };
        assert!(matches!(&args[0], Expr::Var(n, _) if n == "$field0"));
        assert!(matches!(&args[1], Expr::Var(n, _) if n == "$field1"));
    }

    #[test]
    fn constructors_are_exported_and_unannotated() {
        let b = constructor_binding("T", 1, at());
        assert!(b.exported);
        assert!(b.declared_type.is_none());
        assert_eq!(b.name, "T");
    }
}
