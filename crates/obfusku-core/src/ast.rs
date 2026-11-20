//! The Core abstract syntax (`spec/SEMANTIC_CORE.md` §1).
//!
//! Everything the checker and the evaluator ever see: the surface language is
//! desugared into this and nothing else, so adding surface sugar never
//! touches either of them. The tree is plain data — no parsing, no
//! evaluation, no glyphs.
//!
//! | Module | Contents |
//! |---|---|
//! | `literal` | [`Literal`] and its type |
//! | `operator` | [`BinOp`], [`UnOp`] and their typing family |
//! | `expr` | [`Expr`], with `children`/`walk` for passes that only visit |
//! | `pattern` | [`Pattern`], [`MatchArm`] and binder collection |
//! | `declaration` | [`TypeDecl`], [`VariantDecl`]: sums, tuples and records |
//! | `binding` | [`Binding`], [`BindingGroup`], [`Module`] |

mod binding;
mod declaration;
mod expr;
mod literal;
mod operator;
mod pattern;

pub use binding::{Binding, BindingGroup, Module};
pub use declaration::{TypeDecl, VariantDecl};
pub use expr::Expr;
pub use literal::Literal;
pub use operator::{BinOp, OpClass, UnOp};
pub use pattern::{MatchArm, Pattern};
