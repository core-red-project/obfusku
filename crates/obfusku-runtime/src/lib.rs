//! Execution of the Core AST.
//!
//! Takes a *checked* [`Module`](obfusku_core::ast::Module) and runs it:
//! values, closures, `Cell`s, `Raise`/`Catch` unwinding and the tail-call
//! trampoline (`spec/SEMANTIC_CORE.md` §12–§15).
//!
//! **Why independent of the syntax crate.** By the time a program runs it is
//! fully desugared Core; there is nothing here that needs to know surface
//! syntax exists (`IMPLEMENTATION_ARCHITECTURE.md` §6, §11). That also lets
//! the tests build Core directly, with no parser in the loop.
//!
//! **Why natives are values.** Effects and combinators the language cannot
//! express are host functions wrapped as [`Value::Native`]; the evaluator
//! calls them with the same one-argument convention as closures and knows
//! nothing about what they do (`ADR-021`).
//!
//! # Pipeline inside the crate
//!
//! ```text
//! Core AST ──ir::Compiler──▶ ir::Node ──eval::eval──▶ Value
//!   (names)                  (slots, cells,           (persistent data,
//!                             interned tags)           closures, natives)
//! ```
//!
//! # Map
//!
//! | Module | Role |
//! |---|---|
//! | `program` | entry points: [`evaluate`], [`evaluate_with_prelude`], … |
//! | `value` | [`Value`], closures and natives |
//! | `ir` | the resolved tree the evaluator runs |
//! | `eval` | the evaluator: trampoline, limits, traces |
//! | `env` | lexical environments (frame chains) |
//! | `ops` | generated ordering and rendering ([`show`]) |
//! | `persistent` | immutable data structures: [`PVec`], [`PMap`], [`Text`] |
//! | `variants` | constructor order and record fields, known at run time |

#![warn(missing_docs)]

pub mod env;
pub mod eval;
pub mod ir;
pub mod ops;
pub mod persistent;
mod program;
pub mod value;
pub mod variants;

pub use ops::show;
pub use persistent::{PMap, PVec, Text};
pub use program::{
    Evaluated, Fault, closure_from_core, evaluate, evaluate_with_prelude,
    evaluate_with_prelude_traced,
};
pub use value::{NativeFn, Value};
