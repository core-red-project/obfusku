//! Type vocabulary — `spec/SEMANTIC_CORE.md` §3 (types), §4 (kinds,
//! kept internal to `obfusku-typecheck`), §12.1 (`Cell<T>`).
//!
//! Concrete, variable-free — the final shape a resolved type takes.
//! Type variables/substitutions/schemes stay inside `obfusku-typecheck`,
//! which has its own richer `Type` (this vocabulary plus `Var`) and
//! converts down to this one once resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Real,
    Str,
    Bool,
    Unit,
    /// Curried, right-associative.
    Function(Box<Type>, Box<Type>),
    /// One-parameter generic, invariant in `T` (no subtyping anywhere
    /// in this design). No constructors, no pattern form.
    Cell(Box<Type>),
    /// A user-declared ADT applied to its type arguments, e.g.
    /// `Adt("Option", [Int])` = `Option<Int>`.
    Adt(String, Vec<Type>),
    /// A reference to one of a [`crate::ast::TypeDecl`]'s own
    /// `type_params`, meaningful only inside a
    /// [`crate::ast::VariantDecl`]'s field types — never in a resolved
    /// type. `obfusku-typecheck` substitutes it with a fresh variable
    /// at each constructor use.
    Param(String),
}
