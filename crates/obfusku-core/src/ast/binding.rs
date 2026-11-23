//! Top-level bindings and the module that holds them (`SEMANTIC_CORE.md` §11, §20).

use super::declaration::TypeDecl;
use super::expr::Expr;
use crate::types::Type;
use obfusku_diagnostics::Span;

/// A named value at module level (or one member of a `LetRec`).
#[derive(Debug, Clone)]
pub struct Binding {
    /// The bound name.
    pub name: String,
    /// The bound value.
    pub value: Expr,
    /// Whether importers can see it (`ADR-005`, `SEMANTIC_CORE.md` §20).
    pub is_exported: bool,
    /// The whole binding.
    pub span: Span,
    /// The surface type annotation, if any. A `FunctionDeclaration`'s
    /// mandatory annotation is the only source today. It is a constraint for
    /// `obfusku-typecheck` to unify against; `obfusku-runtime` never reads it.
    pub declared_type: Option<Type>,
}

/// A run of bindings that scope together.
///
/// Group membership is decided **once**, in `obfusku-syntax::desugar`, from
/// declaration form alone (`ValueDeclaration` versus adjacent
/// `FunctionDeclaration` runs). It is never reconstructed from a binding's
/// value shape or a dependency/SCC analysis, so what the author wrote is
/// what recursion means.
#[derive(Debug, Clone)]
pub enum BindingGroup {
    /// An ordinary, non-recursive binding: not in its own scope.
    Let(Binding),
    /// Every member is in scope within every other member's value. Each must
    /// be a lambda, which construction guarantees. A single-member group is
    /// direct self-recursion and does **not** collapse to `Let`, whose value
    /// is never in its own scope.
    LetRec(Vec<Binding>),
}

impl BindingGroup {
    /// The group's bindings, whatever its kind.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        match self {
            BindingGroup::Let(b) => std::slice::from_ref(b),
            BindingGroup::LetRec(bs) => bs,
        }
    }

    /// Whether members may refer to themselves and each other.
    #[must_use]
    pub fn is_recursive(&self) -> bool {
        matches!(self, BindingGroup::LetRec(_))
    }
}

/// One compilation unit after desugaring: its type declarations and its
/// binding groups, in source order.
///
/// Order is meaningful: a group sees only the groups before it, plus itself
/// if recursive. That is what lets evaluation proceed top to bottom with no
/// dependency analysis (§11).
#[derive(Debug, Clone, Default)]
pub struct Module {
    /// ADTs declared by this module (§9).
    pub type_decls: Vec<TypeDecl>,
    /// Value bindings, in the order they evaluate.
    pub bindings: Vec<BindingGroup>,
}

impl Module {
    /// Every binding of every group, in source order.
    pub fn all_bindings(&self) -> impl Iterator<Item = &Binding> {
        self.bindings.iter().flat_map(BindingGroup::bindings)
    }

    /// The names this module binds, in source order.
    pub fn bound_names(&self) -> impl Iterator<Item = &str> {
        self.all_bindings().map(|b| b.name.as_str())
    }

    /// The names this module exports.
    pub fn exported_names(&self) -> impl Iterator<Item = &str> {
        self.all_bindings()
            .filter(|b| b.is_exported)
            .map(|b| b.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::literal::Literal;

    fn binding(name: &str, is_exported: bool) -> Binding {
        Binding {
            name: name.to_string(),
            value: Expr::Lit(Literal::Unit, Span::default()),
            is_exported,
            span: Span::default(),
            declared_type: None,
        }
    }

    #[test]
    fn a_let_group_has_one_binding_and_is_not_recursive() {
        let g = BindingGroup::Let(binding("a", false));
        assert_eq!(g.bindings().len(), 1);
        assert!(!g.is_recursive());
    }

    #[test]
    fn a_single_member_letrec_stays_recursive() {
        let g = BindingGroup::LetRec(vec![binding("f", false)]);
        assert!(g.is_recursive());
        assert_eq!(g.bindings().len(), 1);
    }

    #[test]
    fn module_iteration_flattens_groups_in_source_order() {
        let m = Module {
            type_decls: vec![],
            bindings: vec![
                BindingGroup::Let(binding("a", true)),
                BindingGroup::LetRec(vec![binding("f", false), binding("g", true)]),
                BindingGroup::Let(binding("b", false)),
            ],
        };
        assert_eq!(m.bound_names().collect::<Vec<_>>(), ["a", "f", "g", "b"]);
        assert_eq!(m.exported_names().collect::<Vec<_>>(), ["a", "g"]);
    }

    #[test]
    fn an_empty_module_binds_nothing() {
        let m = Module::default();
        assert_eq!(m.all_bindings().count(), 0);
        assert!(m.type_decls.is_empty());
    }
}
