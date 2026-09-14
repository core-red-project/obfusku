//! Internal type representation used during inference — distinct from
//! [`obfusku_core::types::Type`], which has no variables/schemes.
//! Nothing here is `pub` outside this crate.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub u32);

/// [`obfusku_core::types::Type`]'s shapes plus [`Type::Var`].
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Real,
    Str,
    Bool,
    Unit,
    Function(Box<Type>, Box<Type>),
    /// Invariant in `T` (§12.1) — ordinary structural unification, no
    /// separate variance machinery.
    Cell(Box<Type>),
    /// A declared ADT applied to its type arguments, e.g.
    /// `Adt("Option", [Int])` = `Option<Int>`.
    Adt(String, Vec<Type>),
    Var(TypeVarId),
}

impl Type {
    pub fn free_vars(&self, out: &mut HashSet<TypeVarId>) {
        match self {
            Type::Int | Type::Real | Type::Str | Type::Bool | Type::Unit => {}
            Type::Function(a, b) => {
                a.free_vars(out);
                b.free_vars(out);
            }
            Type::Cell(t) => t.free_vars(out),
            Type::Adt(_, args) => {
                for a in args {
                    a.free_vars(out);
                }
            }
            Type::Var(v) => {
                out.insert(*v);
            }
        }
    }

    /// Converts to the concrete Core vocabulary, if fully resolved (no
    /// remaining `Var`). `None` means an ambiguous/never-constrained
    /// type — for a *monomorphic* binding that's an error condition
    /// (`obfusku-typecheck`'s `check` reports it), not something this
    /// conversion silently papers over.
    pub fn to_core(&self) -> Option<obfusku_core::types::Type> {
        use obfusku_core::types::Type as Core;
        Some(match self {
            Type::Int => Core::Int,
            Type::Real => Core::Real,
            Type::Str => Core::Str,
            Type::Bool => Core::Bool,
            Type::Unit => Core::Unit,
            Type::Function(a, b) => Core::Function(Box::new(a.to_core()?), Box::new(b.to_core()?)),
            Type::Cell(t) => Core::Cell(Box::new(t.to_core()?)),
            Type::Adt(name, args) => {
                let mut core_args = Vec::with_capacity(args.len());
                for a in args {
                    core_args.push(a.to_core()?);
                }
                Core::Adt(name.clone(), core_args)
            }
            Type::Var(_) => return None,
        })
    }

    /// Human-readable rendering for diagnostics — e.g. `Cell<Int>`,
    /// `Int → Int`. An unresolved variable renders as `?`, since a
    /// program a reader is looking at should never need to know a raw
    /// variable id.
    pub fn describe(&self) -> String {
        match self {
            Type::Int => "Int".to_string(),
            Type::Real => "Real".to_string(),
            Type::Str => "String".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::Unit => "Unit".to_string(),
            Type::Function(a, b) => format!("{} → {}", a.describe(), b.describe()),
            Type::Cell(t) => format!("Cell<{}>", t.describe()),
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

/// `∀vars. ty` — a type scheme. `vars` is empty for a monomorphic type;
/// `SEMANTIC_CORE.md` §19.1's value restriction governs generalization.
///
/// **Boundary invariant**: `TypeVarId`s are checker-local. Cross-checker
/// schemes must be rebound via `normalize_scheme` before insertion into
/// another `TypeEnv` (see ADR-001).
#[derive(Debug, Clone)]
pub struct Scheme {
    pub vars: Vec<TypeVarId>,
    pub ty: Type,
}

impl Scheme {
    pub fn monomorphic(ty: Type) -> Self {
        Scheme {
            vars: Vec::new(),
            ty,
        }
    }
}
