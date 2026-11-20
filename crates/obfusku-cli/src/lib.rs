//! The Obfusku command-line toolchain as a library.
//!
//! **Orchestration and presentation only — no language logic**
//! (`IMPLEMENTATION_ARCHITECTURE.md` §12). Lexing, parsing, type inference
//! and evaluation belong to the crates that own them; this crate wires them
//! to files, Projects, the host and a terminal. The `obfusku` binary and the
//! language server are both thin clients of this API.
//!
//! # Map
//!
//! | Module | Role |
//! |---|---|
//! | [`pipeline`] | `run`/`check`/`build`/`fmt`/`inspect` over text and files, and the REPL |
//! | [`project`] | Project discovery, manifest, dependencies, imports, bundles |
//! | [`natives`] | the host side of the runtime boundary: effects and combinators |
//! | [`stdlib`] | the bundled standard library, written in Obfusku |
//! | [`report`] | diagnostics as text or JSON, and host-level errors |
//!
//! Data flows `project` (where is the source) → `pipeline` (check it, with
//! `stdlib` and `natives` as the ambient prelude) → `report` (say what went
//! wrong).

#![warn(missing_docs)]

pub mod natives;
pub mod pipeline;
pub mod project;
pub mod report;
pub mod stdlib;

pub use pipeline::{
    Failure, InspectKind, ReplSession, RunResult, analyze_text, build_project, check_file,
    check_source, fmt_source, inspect_source, run_file, run_file_with_args, run_source,
};
pub use report::render::{
    diagnostics as render_diagnostics, diagnostics_map as render_diagnostics_map,
};
