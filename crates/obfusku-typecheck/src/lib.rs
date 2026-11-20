//! Hindley–Milner type checking over the Core AST (`SEMANTIC_CORE.md` §17,
//! §19).
//!
//! The checker sits between desugaring and evaluation. It sees only
//! [`obfusku_core::ast`] and [`obfusku_diagnostics`], never the surface
//! syntax or the runtime, so it can be tested and reasoned about as a pure
//! function of a Core module (`check`) plus whatever names the caller puts
//! in scope (`Prelude`).
//!
//! # How a module is checked
//!
//! 1. Duplicate binding names, and duplicate type names or constructor tags,
//!    are rejected up front.
//! 2. An ADT registry is built from the declarations: the module's own, the
//!    imported ones, and the built-in `Exception`, `Array`, `Map` and `Set`.
//! 3. Each binding group is inferred in order with Algorithm W. A binding
//!    generalizes only if it is a syntactic value, the value restriction
//!    (§19.1) that keeps mutable cells monomorphic.
//! 4. Once every binding is seen, each type is resolved to a variable-free
//!    [`obfusku_core::types::Type`] or kept as a [`Scheme`].
//!
//! Errors accumulate: one ill-typed binding does not hide the others.
//!
//! # Why the checker has its own `Type`
//!
//! The core type describes finished programs and has no variables. Inference
//! needs unification variables and schemes, so it uses [`Type`] and converts
//! at the end ([`Type::to_core`]).

#![warn(missing_docs)]

mod adt;
mod check;
mod infer;
mod typed;
mod types;

pub use check::{
    Prelude, check, check_shadowing, check_with_prelude, check_with_prelude_and_types,
};
pub use infer::instantiate_core_type;
pub use typed::{TypeResult, TypedBinding, TypedModule};
pub use types::{Constraint, Scheme, Type, TypeVarId};
