//! The type vocabulary (`spec/SEMANTIC_CORE.md` §3, §12.1; §4 stays internal
//! to `obfusku-typecheck`).
//!
//! **Concrete and variable-free**: the final shape a resolved type takes.
//! Type variables, substitutions and schemes live inside `obfusku-typecheck`,
//! which has its own richer `Type` (this vocabulary plus `Var`) and converts
//! down to this one once a type is fully resolved. Keeping variables out of
//! the Core is what lets the AST, the declarations and the runtime talk
//! about types without depending on the inference engine.

use std::fmt;

/// A resolved type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// 64-bit signed integer.
    Int,
    /// IEEE-754 double.
    Real,
    /// UTF-8 string.
    Str,
    /// Boolean.
    Bool,
    /// The type of the single value `Unit`.
    Unit,
    /// A function from the first type to the second. Curried and
    /// right-associative: `a → b → c` is `a → (b → c)`, so every function has
    /// exactly one parameter (§1, §5).
    Function(Box<Type>, Box<Type>),
    /// A mutable cell (§12.1). One-parameter generic and **invariant** in
    /// `T`: there is no subtyping anywhere in this design. It has no
    /// constructors and no pattern form — a cell is made only by `MutCell`.
    Cell(Box<Type>),
    /// A user-declared ADT applied to its type arguments, e.g.
    /// `Adt("Option", [Int])` is `Option<Int>`. The built-in `Array`, `Map`
    /// and `Set` are represented the same way.
    Adt(String, Vec<Type>),
    /// A reference to one of a [`TypeDecl`](crate::ast::TypeDecl)'s own
    /// `type_params`, meaningful only inside a
    /// [`VariantDecl`](crate::ast::VariantDecl)'s field types and never in a
    /// resolved type. `obfusku-typecheck` substitutes a fresh variable for it
    /// at each constructor use.
    Param(String),
}

impl Type {
    /// The function type `from → to`.
    #[must_use]
    pub fn function(from: Type, to: Type) -> Type {
        Type::Function(Box::new(from), Box::new(to))
    }

    /// The curried function type taking `params` in order and returning
    /// `ret`; with no parameters it is just `ret`.
    #[must_use]
    pub fn curried(params: impl IntoIterator<Item = Type>, ret: Type) -> Type {
        let params: Vec<Type> = params.into_iter().collect();
        params
            .into_iter()
            .rev()
            .fold(ret, |acc, p| Type::function(p, acc))
    }

    /// The cell type `Cell<inner>`.
    #[must_use]
    pub fn cell(inner: Type) -> Type {
        Type::Cell(Box::new(inner))
    }

    /// An ADT with no type arguments.
    #[must_use]
    pub fn named(tag: impl Into<String>) -> Type {
        Type::Adt(tag.into(), Vec::new())
    }

    /// How many arguments the type takes before yielding a non-function.
    #[must_use]
    pub fn arity(&self) -> usize {
        let mut n = 0;
        let mut t = self;
        while let Type::Function(_, to) = t {
            n += 1;
            t = to;
        }
        n
    }

    /// The type after all arguments are supplied; the type itself if it is
    /// not a function.
    #[must_use]
    pub fn result(&self) -> &Type {
        let mut t = self;
        while let Type::Function(_, to) = t {
            t = to;
        }
        t
    }

    /// The parameter types of a curried function, outermost first.
    #[must_use]
    pub fn params(&self) -> Vec<&Type> {
        let mut out = Vec::new();
        let mut t = self;
        while let Type::Function(from, to) = t {
            out.push(&**from);
            t = to;
        }
        out
    }
}

/// Renders in the same words diagnostics use (`Int`, `String`, `Cell<Int>`,
/// `Option<Int>`), so a message built from a Core type reads like one built by
/// the checker.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => f.write_str("Int"),
            Type::Real => f.write_str("Real"),
            Type::Str => f.write_str("String"),
            Type::Bool => f.write_str("Bool"),
            Type::Unit => f.write_str("Unit"),
            // A function on the left of an arrow needs parentheses to keep
            // right-associativity unambiguous: `(a → b) → c`.
            Type::Function(from, to) if matches!(**from, Type::Function(..)) => {
                write!(f, "({from}) → {to}")
            }
            Type::Function(from, to) => write!(f, "{from} → {to}"),
            Type::Cell(inner) => write!(f, "Cell<{inner}>"),
            Type::Adt(name, args) if args.is_empty() => f.write_str(name),
            Type::Adt(name, args) => {
                write!(f, "{name}<")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(">")
            }
            Type::Param(name) => f.write_str(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curried_builds_a_right_nested_chain_in_parameter_order() {
        let t = Type::curried([Type::Int, Type::Str], Type::Bool);
        assert_eq!(
            t,
            Type::function(Type::Int, Type::function(Type::Str, Type::Bool))
        );
    }

    #[test]
    fn curried_with_no_parameters_is_the_result() {
        assert_eq!(Type::curried([], Type::Int), Type::Int);
    }

    #[test]
    fn arity_result_and_params_agree() {
        let t = Type::curried([Type::Int, Type::Real, Type::Str], Type::Unit);
        assert_eq!(t.arity(), 3);
        assert_eq!(t.result(), &Type::Unit);
        assert_eq!(t.params(), [&Type::Int, &Type::Real, &Type::Str]);
    }

    #[test]
    fn a_non_function_has_arity_zero_and_is_its_own_result() {
        assert_eq!(Type::Int.arity(), 0);
        assert_eq!(Type::Int.result(), &Type::Int);
        assert!(Type::Int.params().is_empty());
    }

    #[test]
    fn a_function_returning_a_function_argument_counts_only_the_spine() {
        // (Int → Int) → Int takes one argument, not two.
        let t = Type::function(Type::function(Type::Int, Type::Int), Type::Int);
        assert_eq!(t.arity(), 1);
    }

    #[test]
    fn display_uses_the_diagnostic_vocabulary() {
        assert_eq!(Type::Str.to_string(), "String");
        assert_eq!(Type::cell(Type::Int).to_string(), "Cell<Int>");
        assert_eq!(Type::named("Shape").to_string(), "Shape");
        assert_eq!(
            Type::Adt("Map".into(), vec![Type::Str, Type::Int]).to_string(),
            "Map<String, Int>"
        );
        assert_eq!(Type::Param("t".into()).to_string(), "t");
    }

    #[test]
    fn display_parenthesizes_a_function_on_the_left_only() {
        let curried = Type::curried([Type::Int, Type::Int], Type::Int);
        assert_eq!(curried.to_string(), "Int → Int → Int");
        let higher = Type::function(Type::function(Type::Int, Type::Int), Type::Int);
        assert_eq!(higher.to_string(), "(Int → Int) → Int");
    }

    #[test]
    fn cells_are_invariant_so_equality_is_exact() {
        assert_ne!(Type::cell(Type::Int), Type::cell(Type::Real));
        assert_eq!(Type::cell(Type::Int), Type::cell(Type::Int));
    }
}
