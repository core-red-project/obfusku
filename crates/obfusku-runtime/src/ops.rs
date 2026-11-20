//! Generated structural operations over any value (`ADR-025`): total
//! ordering and canonical rendering.
//!
//! Both are defined by the language, never supplied by the user: the same
//! stance as generated equality (`SEMANTIC_CORE.md` §18). That is what lets
//! any orderable type be a map key or be sorted without declaring anything,
//! and what makes output deterministic. The type checker guarantees they are
//! only applied to types with no function-typed part, so the arms for
//! closures and cells exist only to keep the functions total.
//!
//! | Module | Provides |
//! |---|---|
//! | `order` | [`cmp_total`] (map keys, sorting) and [`cmp_operator`] (`<`, `>`, `≤`, `≥`) |
//! | `show` | [`show()`]: canonical source-like rendering |

mod order;
mod show;

pub use order::{cmp_operator, cmp_total};
pub use show::show;
