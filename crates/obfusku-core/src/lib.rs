//! Core AST and type vocabulary — `spec/SEMANTIC_CORE.md` §1 (grammar),
//! §3 (types), §4 (kinds).
//!
//! Data and typing rules only — no parsing logic, no execution logic,
//! no glyphs. See `spec/IMPLEMENTATION_ARCHITECTURE.md` §9.

pub mod ast;
pub mod types;
