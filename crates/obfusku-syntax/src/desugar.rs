//! Surface AST to Core AST (`spec/SEMANTIC_CORE.md` §16's consolidated
//! desugaring table; `CONCRETE_SYMBOLIC_GRAMMAR.md` §7, §12).
//!
//! **Why lower at all.** The checker and the evaluator only ever see the
//! small Core. Every convenience of the surface (`∧`, pipes, holes, named
//! records, local function runs, mutable names) is removed here, once, so
//! those two stages stay small enough to be correct.
//!
//! **Errors are collected, not first-fail.** Lowering is the first pass that
//! can report several independent problems (an unknown record field here, a
//! bad rebind there), so independent subtrees are all lowered and their
//! errors concatenated. Within one subtree, the first error wins.
//!
//! # The three tricky lowerings
//!
//! **`◈` scoping** (§8). A bare-callable stage passes the ambient pipe name
//! through unchanged; an expression stage generates a fresh synthetic name,
//! lowers its body with that as the new ambient name, and so shadows any
//! outer `◈` lexically.
//!
//! **Mutability** (§12, §12.1, §13).
//! - `x ≔˚ init` lowers to a binding whose value is `MutCell(init)`.
//! - An ordinary reference to a mutable name lowers to `MutRead(Var(name))`.
//! - `˚name` lowers to bare `Var(name)`: the cell itself, no implicit read.
//! - `name ⚙︎ value` lowers to `MutRebind(Var(name), value)`, and is rejected
//!   if `name` was never mutable.
//! - A lambda that rebinds a mutable *free* variable it did not explicitly
//!   `˚`-capture is rejected.
//!
//! Capture is uniform whatever the syntax (§13, amended): every closure
//! captures the *same cell object* for a mutable free variable. Only the
//! body differs: an ordinary reference reads the cell's current contents at
//! call time (there is no snapshot), an explicit `˚` one hands over the cell.
//!
//! **Record reordering** (`ABSTRACT_GRAMMAR.md` §4). Named construction and
//! named patterns are written in any order and reordered into the record's
//! declared positional order, which is why a record must be declared before
//! it is used.
//!
//! | Module | Lowers |
//! |---|---|
//! | `module` | a whole module: bindings, groups, types, built-in exceptions |
//! | `expression` | every expression form |
//! | `pattern` | patterns, including record patterns |
//! | `types` | type annotations and declared function types |
//! | `constructor` | the curried constructor function a type declaration implies |
//! | `mutability` | the closure-capture rule's free-variable analysis |
//! | `lowerer` | shared state: fresh names, mutable names, record fields |

mod constructor;
mod expression;
mod lowerer;
mod module;
mod mutability;
mod pattern;
mod types;

use crate::ast as surface;
use obfusku_core::ast as core;
use obfusku_diagnostics::Diagnostic;
use std::collections::HashSet;

/// Lowers a surface module to Core.
pub fn desugar(surface: &surface::Module) -> Result<core::Module, Vec<Diagnostic>> {
    desugar_with_ambient_mut_names(surface, &HashSet::new())
}

/// [`desugar`], with `ambient_mut_names` already treated as mutable (`≔˚`)
/// before this module's own declarations are lowered.
///
/// For a caller whose "module" is a fragment of an ongoing session: an
/// incremental REPL desugars each accepted line on its own, so without this a
/// later line that reads a cell an earlier line declared would lower to a
/// bare `Var` instead of the `MutRead` or `MutRebind` it needs. Every other
/// caller uses plain [`desugar`].
pub fn desugar_with_ambient_mut_names(
    surface: &surface::Module,
    ambient_mut_names: &HashSet<String>,
) -> Result<core::Module, Vec<Diagnostic>> {
    desugar_with_ambient(surface, ambient_mut_names, &[])
}

/// [`desugar_with_ambient_mut_names`], plus the type declarations of
/// already-resolved modules (imports, the stdlib, earlier REPL lines), whose
/// records can then be constructed and matched by field name here.
pub fn desugar_with_ambient(
    surface: &surface::Module,
    ambient_mut_names: &HashSet<String>,
    ambient_types: &[core::TypeDecl],
) -> Result<core::Module, Vec<Diagnostic>> {
    module::lower_module(surface, ambient_mut_names, ambient_types)
}
