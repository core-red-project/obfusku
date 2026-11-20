//! The checker's own type vocabulary.
//!
//! [`obfusku_core::types::Type`] is what a *finished* program carries, so it
//! has no unification variables and no schemes. Inference needs both, so it
//! works on the types defined here and converts to the core vocabulary only
//! once every variable has been resolved ([`Type::to_core`]).

use crate::adt::{ARRAY, EXCEPTION};
use crate::infer::instantiate_core_type;
use obfusku_core::ast::Literal;
use std::collections::{HashMap, HashSet};

/// The identity of a unification variable.
///
/// Identifiers are minted by one checker's counter, so they are meaningful
/// only to that checker; see the boundary invariant on [`Scheme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub u32);

/// [`obfusku_core::types::Type`]'s shapes plus [`Type::Var`].
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// 64-bit signed integer.
    Int,
    /// 64-bit float.
    Real,
    /// Text.
    Str,
    /// Boolean.
    Bool,
    /// The single-valued type of "no information".
    Unit,
    /// A curried function: parameter, then result.
    Function(Box<Type>, Box<Type>),
    /// A mutable cell. Invariant in its element (§12.1), which falls out of
    /// ordinary structural unification, so no variance machinery exists.
    Cell(Box<Type>),
    /// A declared ADT applied to its type arguments, e.g.
    /// `Adt("Option", [Int])` is `Option<Int>`.
    Adt(String, Vec<Type>),
    /// A unification variable, not yet known to be any concrete type.
    Var(TypeVarId),
}

impl Type {
    /// The function type `param → result`.
    #[must_use]
    pub fn function(param: Type, result: Type) -> Type {
        Type::Function(Box::new(param), Box::new(result))
    }

    /// The cell type `Cell<inner>`.
    #[must_use]
    pub fn cell(inner: Type) -> Type {
        Type::Cell(Box::new(inner))
    }

    /// The built-in `Array<element>` (§9.2).
    #[must_use]
    pub fn array(element: Type) -> Type {
        Type::Adt(ARRAY.to_string(), vec![element])
    }

    /// The built-in open `Exception` type (§15.2).
    #[must_use]
    pub fn exception() -> Type {
        Type::Adt(EXCEPTION.to_string(), Vec::new())
    }

    /// The type of a literal. Literals are never polymorphic, so the core
    /// answer ([`Literal::ty`]) converts without any variable supply.
    pub(crate) fn of_literal(literal: &Literal) -> Type {
        instantiate_core_type(&literal.ty(), &HashMap::new())
    }

    /// Adds every variable occurring in `self` to `out`.
    pub fn free_vars(&self, out: &mut HashSet<TypeVarId>) {
        match self {
            Type::Int | Type::Real | Type::Str | Type::Bool | Type::Unit => {}
            Type::Function(parameter, result) => {
                parameter.free_vars(out);
                result.free_vars(out);
            }
            Type::Cell(inner) => inner.free_vars(out),
            Type::Adt(_, args) => args.iter().for_each(|a| a.free_vars(out)),
            Type::Var(v) => {
                out.insert(*v);
            }
        }
    }

    /// Rebuilds `self` with every variable for which `lookup` answers
    /// `Some` replaced by that answer.
    ///
    /// The one structural traversal behind both applying a substitution
    /// (`lookup` chases the unifier's bindings) and instantiating a scheme
    /// (`lookup` maps its quantified variables to fresh ones).
    pub(crate) fn replace_vars(&self, lookup: &impl Fn(TypeVarId) -> Option<Type>) -> Type {
        match self {
            Type::Var(v) => lookup(*v).unwrap_or_else(|| self.clone()),
            Type::Function(parameter, result) => {
                Type::function(parameter.replace_vars(lookup), result.replace_vars(lookup))
            }
            Type::Cell(inner) => Type::cell(inner.replace_vars(lookup)),
            Type::Adt(name, args) => Type::Adt(
                name.clone(),
                args.iter().map(|a| a.replace_vars(lookup)).collect(),
            ),
            Type::Int | Type::Real | Type::Str | Type::Bool | Type::Unit => self.clone(),
        }
    }

    /// Converts to the core vocabulary, or `None` while any variable
    /// remains.
    ///
    /// `None` is deliberately not papered over: for a *monomorphic* binding
    /// it means the type was never fully constrained, which `check` reports
    /// as an error rather than letting an ambiguous type escape.
    #[must_use]
    pub fn to_core(&self) -> Option<obfusku_core::types::Type> {
        use obfusku_core::types::Type as Core;
        Some(match self {
            Type::Int => Core::Int,
            Type::Real => Core::Real,
            Type::Str => Core::Str,
            Type::Bool => Core::Bool,
            Type::Unit => Core::Unit,
            Type::Function(parameter, result) => {
                Core::function(parameter.to_core()?, result.to_core()?)
            }
            Type::Cell(inner) => Core::cell(inner.to_core()?),
            Type::Adt(name, args) => Core::Adt(
                name.clone(),
                args.iter()
                    .map(Type::to_core)
                    .collect::<Option<Vec<_>>>()?,
            ),
            Type::Var(_) => return None,
        })
    }

    /// Human-readable rendering for diagnostics, e.g. `Cell<Int>` or
    /// `Int → Int`.
    ///
    /// An unresolved variable prints as `?`: a reader looking at their own
    /// program has no use for a raw variable number.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Type::Int => "Int".to_string(),
            Type::Real => "Real".to_string(),
            Type::Str => "String".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::Unit => "Unit".to_string(),
            // Same parenthesization as the core type's `Display`, so a
            // message reads alike whichever side built it.
            Type::Function(parameter, result) if matches!(**parameter, Type::Function(..)) => {
                format!("({}) → {}", parameter.describe(), result.describe())
            }
            Type::Function(parameter, result) => {
                format!("{} → {}", parameter.describe(), result.describe())
            }
            Type::Cell(inner) => format!("Cell<{}>", inner.describe()),
            Type::Adt(name, args) if args.is_empty() => name.clone(),
            Type::Adt(name, args) => format!(
                "{name}<{}>",
                args.iter()
                    .map(Type::describe)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Type::Var(_) => "?".to_string(),
        }
    }
}

/// `∀vars. constraints ⇒ ty`: a type scheme.
///
/// `vars` is empty for a monomorphic type. Which bindings may generalize at
/// all is the value restriction (`SEMANTIC_CORE.md` §19.1).
///
/// **Boundary invariant**: [`TypeVarId`]s are checker-local, so a scheme
/// minted by one checker must be rebound to the receiving checker's own
/// variables before it enters that checker's environment (`ADR-001`);
/// otherwise a numeric coincidence between a foreign "bound" variable and a
/// live local one could strip a local function of its polymorphism.
#[derive(Debug, Clone)]
pub struct Scheme {
    /// The quantified variables.
    pub vars: Vec<TypeVarId>,
    /// The body, which mentions `vars`.
    pub ty: Type,
    /// Generated-operation obligations on `vars` (`ADR-025`): instantiating
    /// the scheme requires each listed variable's type to support the
    /// constraint, exactly as `≡`, `<` and `⍕` require of their operands.
    pub constraints: Vec<(TypeVarId, Constraint)>,
}

/// An operation the language generates for every type built from
/// primitives, ADTs and collections, and never for a function type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Constraint {
    /// `≡` and `≠` (§18).
    Eq,
    /// `<`, `>`, `≤`, `≥`, `⍋`, and `Map`/`Set` keys.
    Ord,
    /// `⍕`, the canonical rendering.
    Show,
}

impl Constraint {
    /// The noun used in diagnostics: "'… → …' has no *equality*".
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Constraint::Eq => "equality",
            Constraint::Ord => "ordering",
            Constraint::Show => "rendering",
        }
    }
}

impl Scheme {
    /// `ty` with nothing quantified.
    #[must_use]
    pub fn monomorphic(ty: Type) -> Self {
        Scheme::poly(Vec::new(), ty)
    }

    /// `∀vars. ty` with no constraints.
    #[must_use]
    pub fn poly(vars: Vec<TypeVarId>, ty: Type) -> Self {
        Scheme::constrained(vars, Vec::new(), ty)
    }

    /// `∀vars. constraints ⇒ ty`.
    #[must_use]
    pub fn constrained(
        vars: Vec<TypeVarId>,
        constraints: Vec<(TypeVarId, Constraint)>,
        ty: Type,
    ) -> Self {
        Scheme {
            vars,
            ty,
            constraints,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(n: u32) -> Type {
        Type::Var(TypeVarId(n))
    }

    #[test]
    fn describing_a_function_type_uses_the_arrow() {
        let ty = Type::function(Type::Int, Type::function(Type::Str, Type::Bool));
        assert_eq!(ty.describe(), "Int → String → Bool");
    }

    #[test]
    fn a_function_parameter_that_is_a_function_is_parenthesized() {
        let ty = Type::function(Type::function(Type::Int, Type::Int), Type::Bool);
        assert_eq!(ty.describe(), "(Int → Int) → Bool");
    }

    #[test]
    fn describing_an_adt_lists_its_arguments() {
        let ty = Type::Adt("Pair".into(), vec![Type::Int, Type::cell(Type::Real)]);
        assert_eq!(ty.describe(), "Pair<Int, Cell<Real>>");
        assert_eq!(Type::exception().describe(), "Exception");
    }

    #[test]
    fn an_unresolved_variable_prints_without_its_number() {
        assert_eq!(var(41).describe(), "?");
    }

    #[test]
    fn free_vars_collects_through_every_constructor() {
        let ty = Type::function(
            var(0),
            Type::Adt("T".into(), vec![Type::cell(var(1)), var(0)]),
        );
        let mut found = HashSet::new();
        ty.free_vars(&mut found);
        assert_eq!(found, HashSet::from([TypeVarId(0), TypeVarId(1)]));
    }

    #[test]
    fn replace_vars_leaves_unmapped_variables_alone() {
        let ty = Type::function(var(0), var(1));
        let replaced = ty.replace_vars(&|v| (v == TypeVarId(0)).then_some(Type::Int));
        assert_eq!(replaced, Type::function(Type::Int, var(1)));
    }

    #[test]
    fn a_closed_type_converts_to_core() {
        let ty = Type::function(Type::Int, Type::array(Type::Str));
        let core = ty.to_core().expect("no variables remain");
        assert_eq!(core.to_string(), "Int → Array<String>");
    }

    #[test]
    fn a_type_with_a_variable_does_not_convert() {
        assert!(Type::cell(var(3)).to_core().is_none());
    }

    #[test]
    fn literals_have_the_type_the_core_assigns_them() {
        assert_eq!(Type::of_literal(&Literal::Int(1)), Type::Int);
        assert_eq!(Type::of_literal(&Literal::Unit), Type::Unit);
    }

    #[test]
    fn scheme_constructors_differ_only_in_what_they_quantify() {
        let mono = Scheme::monomorphic(Type::Int);
        assert!(mono.vars.is_empty() && mono.constraints.is_empty());
        let constrained = Scheme::constrained(
            vec![TypeVarId(0)],
            vec![(TypeVarId(0), Constraint::Ord)],
            var(0),
        );
        assert_eq!(constrained.constraints.len(), 1);
    }

    #[test]
    fn constraints_name_their_operation() {
        assert_eq!(Constraint::Eq.describe(), "equality");
        assert_eq!(Constraint::Ord.describe(), "ordering");
        assert_eq!(Constraint::Show.describe(), "rendering");
    }
}
