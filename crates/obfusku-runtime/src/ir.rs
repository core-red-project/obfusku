//! The resolved form of the Core AST that the evaluator runs.
//!
//! **Why a second tree.** The Core AST is for checking: it names variables
//! and constructors by text. Running it directly would hash and compare
//! identifier strings on every variable read and every match. Instead each
//! top-level binding is compiled once into [`Node`]s where
//!
//! - a local variable is a `(frame depth, slot)` pair ([`Env`](crate::env::Env)),
//! - a top-level name is a shared [`GlobalCell`],
//! - a constructor tag is an interned `Rc<str>`.
//!
//! so evaluation never touches identifier text.
//!
//! | Module | Contents |
//! |---|---|
//! | `node` | the tree: [`Node`], [`Arm`], [`Pat`] |
//! | `globals` | top-level name storage: [`GlobalCell`], [`Globals`] |
//! | `intern` | one shared `Rc<str>` per tag |
//! | `compile` | Core AST to IR: [`Compiler`] |

mod compile;
mod globals;
mod intern;
mod node;

pub use compile::{Compiler, literal_value};
pub use globals::{GlobalCell, Globals};
pub use intern::intern;
pub use node::{Arm, Node, Pat};
