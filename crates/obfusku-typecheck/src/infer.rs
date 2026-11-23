//! The inference engine: Algorithm W over the Core AST (`SEMANTIC_CORE.md`
//! §17, §19).
//!
//! One [`Checker`] owns everything inference mutates: the substitution built
//! by unification, the supply of fresh variables, the outstanding
//! generated-operation obligations (`ADR-025`) and the warnings found on the
//! way. The submodules each add the methods of one concern to it:
//!
//! | module | concern |
//! |---|---|
//! | `unify` | the substitution, unification, and reporting a mismatch |
//! | `scheme` | instantiation, generalization (§19.1), the value restriction |
//! | `obligation` | deferring and discharging `≡`/`<`/`⍕` requirements |
//! | `annotation` | turning a written type into checker types |
//! | `expression` | the typing rule of each core form |
//! | `group` | mutually recursive `LetRec` groups |
//! | `constructors` | constructors, arrays, record fields |
//! | `operator` | the closed operator table (§20.2) |
//! | `matching` | match typing and unreachable-arm detection |
//! | `exhaustive` | match exhaustiveness (§17) |
//!
//! Failures are `Vec<Diagnostic>` rather than one diagnostic because a few
//! rules (annotation arity, a group's members) legitimately report several
//! problems from one form.

mod annotation;
mod constructors;
mod exhaustive;
mod expression;
mod group;
mod matching;
mod obligation;
mod operator;
mod scheme;
mod unify;

pub use annotation::instantiate_core_type;

use crate::adt::AdtRegistry;
use crate::types::{Constraint, Scheme, Type, TypeVarId};
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashMap;

/// Names in scope, each with its type scheme.
pub(crate) type TypeEnv = HashMap<String, Scheme>;

/// The result of a typing rule: the answer, or every diagnostic it raised.
pub(crate) type Infer<T> = Result<T, Vec<Diagnostic>>;

/// Fails a typing rule with one error at `span`.
pub(crate) fn reject<T>(span: Span, message: impl Into<String>) -> Infer<T> {
    Err(vec![Diagnostic::error(message, span)])
}

/// A generated operation required of `ty`, discharged once `ty` is known
/// (`ADR-025`). Deferred because the operand's type is often still a
/// variable at the point the operator is seen.
struct Obligation {
    ty: Type,
    constraint: Constraint,
    span: Span,
}

/// The mutable state of one module's type check.
pub(crate) struct Checker {
    obligations: Vec<Obligation>,
    /// Errors found while discharging obligations inside `generalize`, which
    /// cannot itself fail; surfaced by `finish`.
    deferred: Vec<Diagnostic>,
    subst: HashMap<TypeVarId, Type>,
    next_var: u32,
    adt_registry: AdtRegistry,
    /// Non-fatal findings, currently unreachable `Match` arms.
    pub(crate) warnings: Vec<Diagnostic>,
    /// One shared variable per type-variable name written in a `Lambda`
    /// annotation. Cached checker-wide because desugaring renames them to be
    /// module-unique, so equal names mean the same variable by construction.
    lambda_param_vars: HashMap<String, Type>,
}

impl Checker {
    /// A checker with no variables, bindings or obligations yet.
    pub(crate) fn new(adt_registry: AdtRegistry) -> Self {
        Checker {
            obligations: Vec::new(),
            deferred: Vec::new(),
            subst: HashMap::new(),
            next_var: 0,
            adt_registry,
            warnings: Vec::new(),
            lambda_param_vars: HashMap::new(),
        }
    }

    /// A variable no other call has returned.
    pub(crate) fn fresh_id(&mut self) -> TypeVarId {
        let id = TypeVarId(self.next_var);
        self.next_var += 1;
        id
    }

    /// A fresh unification variable, as a type.
    pub(crate) fn fresh(&mut self) -> Type {
        Type::Var(self.fresh_id())
    }

    /// `ty` as a diagnostic should print it: with everything learned so far
    /// applied.
    pub(crate) fn show(&self, ty: &Type) -> String {
        self.resolve(ty).describe()
    }
}

#[cfg(test)]
mod testing {
    //! Builders shared by the engine's unit tests.

    use super::Checker;
    use crate::adt::AdtRegistry;
    use obfusku_core::ast::{Expr, Literal};
    use obfusku_diagnostics::{SourceId, Span};
    use std::rc::Rc;

    pub(super) fn span() -> Span {
        Span::point(SourceId(0), 0)
    }

    pub(super) fn checker() -> Checker {
        Checker::new(AdtRegistry::build(&[]).expect("built-ins always build"))
    }

    pub(super) fn int(value: i64) -> Expr {
        Expr::Lit(Literal::Int(value), span())
    }

    pub(super) fn boolean(is_true: bool) -> Expr {
        Expr::Lit(Literal::Bool(is_true), span())
    }

    pub(super) fn string(value: &str) -> Expr {
        Expr::Lit(Literal::Str(value.to_string()), span())
    }

    pub(super) fn var(name: &str) -> Expr {
        Expr::Var(name.to_string(), span())
    }

    pub(super) fn lambda(param: &str, body: Expr) -> Expr {
        Expr::Lambda {
            param: param.to_string(),
            param_ty: None,
            body: Rc::new(body),
            span: span(),
        }
    }

    pub(super) fn apply(func: Expr, arg: Expr) -> Expr {
        Expr::Apply {
            func: Box::new(func),
            arg: Box::new(arg),
            span: span(),
        }
    }
}
